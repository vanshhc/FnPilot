import AppKit
import ApplicationServices
import CoreAudio
import FnPilotCore

struct ActionOutcome {
    let message: String
    let verified: Bool
}

@MainActor
final class MacExecutor {
    private(set) var apps: [AppInfo] = []
    private var appURLs: [String: URL] = [:]
    private var elements: [String: AXUIElement] = [:]
    private var signatures: [String: String] = [:]
    private var savedFrames: [CFHashCode: CGRect] = [:]
    private var chosenWindows: [pid_t: AXUIElement] = [:]

    func refreshApps() {
        var found: [String: (AppInfo, URL)] = [:]
        let roots = ["/Applications", "/System/Applications", NSHomeDirectory() + "/Applications"]
        for root in roots {
            guard let walker = FileManager.default.enumerator(at: URL(fileURLWithPath: root), includingPropertiesForKeys: [.isDirectoryKey], options: [.skipsHiddenFiles]) else { continue }
            for case let url as URL in walker where url.pathExtension == "app" {
                walker.skipDescendants()
                guard let bundle = Bundle(url: url), let id = bundle.bundleIdentifier else { continue }
                let name = (bundle.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String) ?? (bundle.object(forInfoDictionaryKey: "CFBundleName") as? String) ?? url.deletingPathExtension().lastPathComponent
                found[id] = (AppInfo(name: name, bundleID: id), url)
            }
        }
        apps = found.values.map(\.0).sorted { $0.name.localizedStandardCompare($1.name) == .orderedAscending }
        appURLs = found.mapValues(\.1)
    }

    func context(for target: NSRunningApplication?, clarification: String? = nil) -> CommandContext {
        elements.removeAll(); signatures.removeAll()
        var controls: [UIControl] = []
        if AXIsProcessTrusted(), let target {
            let app = AXUIElementCreateApplication(target.processIdentifier)
            AXUIElementSetMessagingTimeout(app, 0.3)
            // Chrome exposes its full accessibility tree when manual accessibility is enabled.
            if target.bundleIdentifier == "com.google.Chrome" { _ = AXUIElementSetAttributeValue(app, "AXManualAccessibility" as CFString, kCFBooleanTrue) }
            let roots = [window(for: target), element(app, kAXMenuBarAttribute)].compactMap { $0 }
            var visited = Set<CFHashCode>()
            for root in roots { scan(root, depth: 0, visited: &visited, controls: &controls) }
        }
        return CommandContext(appName: target?.localizedName ?? "", bundleID: target?.bundleIdentifier ?? "", apps: apps, controls: controls, clarification: clarification)
    }

    private func scan(_ node: AXUIElement, depth: Int, visited: inout Set<CFHashCode>, controls: inout [UIControl]) {
        let hash = CFHash(node)
        guard depth < 14, visited.count < 700, controls.count < 90, visited.insert(hash).inserted else { return }
        let role = string(node, kAXRoleAttribute)
        if string(node, kAXSubroleAttribute) == (kAXSecureTextFieldSubrole as String) { return }
        let label = [string(node, kAXTitleAttribute), string(node, kAXDescriptionAttribute), string(node, kAXHelpAttribute)].first { !$0.isEmpty } ?? ""
        let actionable = ["AXButton", "AXMenuItem", "AXMenuBarItem", "AXCheckBox", "AXRadioButton", "AXTextField", "AXTextArea", "AXComboBox", "AXSlider", "AXLink", "AXPopUpButton"].contains(role)
        if actionable && !label.isEmpty && (number(node, kAXEnabledAttribute)?.boolValue ?? true) {
            let id = "c" + String(hash, radix: 16)
            elements[id] = node; signatures[id] = role + ":" + label
            let value = ["AXCheckBox", "AXRadioButton", "AXSlider"].contains(role) ? number(node, kAXValueAttribute)?.stringValue : nil
            let subrole = string(node, kAXSubroleAttribute)
            let exposedRole = ["AXCloseButton", "AXMinimizeButton", "AXFullScreenButton", "AXZoomButton"].contains(subrole) ? subrole : role
            controls.append(UIControl(id: id, role: exposedRole, label: String(label.prefix(140)), value: value))
        }
        for child in children(node) { scan(child, depth: depth + 1, visited: &visited, controls: &controls) }
    }

    func execute(_ action: Action, target: inout NSRunningApplication?) async throws -> ActionOutcome {
        try Task.checkCancellation()
        try ActionRegistry.validate(Interpretation(actions: [action]))
        if ["volume_up", "volume_down", "set_volume", "mute", "unmute"].contains(action.kind) { return try audio(action) }
        if action.kind.contains("brightness") { return try brightness(action) }
        if ["open_app", "focus_app"].contains(action.kind) {
            let info = try resolve(action.app!)
            if let running = NSRunningApplication.runningApplications(withBundleIdentifier: info.bundleID).first {
                running.activate(options: [.activateAllWindows]); target = running
            } else if action.kind == "open_app", let url = appURLs[info.bundleID] {
                let configuration = NSWorkspace.OpenConfiguration(); configuration.activates = true
                target = try await NSWorkspace.shared.openApplication(at: url, configuration: configuration)
            } else { throw PilotError("\(info.name) is not running. Say ‘open \(info.name)’ to launch it.") }
            try await waitUntil { NSWorkspace.shared.frontmostApplication?.bundleIdentifier == info.bundleID }
            return ActionOutcome(message: "Opened \(info.name)", verified: true)
        }
        if let appName = action.app {
            let info = try resolve(appName)
            guard let running = NSRunningApplication.runningApplications(withBundleIdentifier: info.bundleID).first else { throw PilotError("\(info.name) is not running.") }
            target = running
        }
        guard let current = target, !current.isTerminated else { throw PilotError("There is no target app. Name an app to control.") }
        if action.kind == "quit_app" {
            guard current.terminate() else { throw PilotError("\(current.localizedName ?? "The app") declined to quit.") }
            try await Task.sleep(nanoseconds: 300_000_000)
            return ActionOutcome(message: current.isTerminated ? "Quit \(current.localizedName ?? "app")" : "Quit requested; the app may be showing a save dialog", verified: current.isTerminated)
        }
        guard AXIsProcessTrusted() else { throw PilotError("Enable FnPilot in System Settings → Privacy & Security → Accessibility, then retry.") }
        if ["press_control", "set_control_value"].contains(action.kind) {
            guard let id = action.controlID, let node = elements[id], let signature = signatures[id] else { throw PilotError("That control is no longer visible. Hold Fn and try again.") }
            var pid: pid_t = 0
            guard AXUIElementGetPid(node, &pid) == .success, pid == current.processIdentifier else { throw PilotError("The control belongs to a different app. Nothing was pressed.") }
            let label = [string(node, kAXTitleAttribute), string(node, kAXDescriptionAttribute), string(node, kAXHelpAttribute)].first { !$0.isEmpty } ?? ""
            guard signature == string(node, kAXRoleAttribute) + ":" + label else { throw PilotError("The control changed while interpreting the command.") }
            if action.kind == "press_control" {
                try press(node)
                return ActionOutcome(message: "Pressed \(label)", verified: false)
            }
            try set(node, kAXValueAttribute, action.text! as CFString)
            return ActionOutcome(message: "Set \(label)", verified: string(node, kAXValueAttribute) == action.text)
        }
        if action.kind == "type_text" {
            current.activate(options: [.activateAllWindows])
            try await waitUntil { NSWorkspace.shared.frontmostApplication?.processIdentifier == current.processIdentifier }
            let app = AXUIElementCreateApplication(current.processIdentifier)
            guard let field = element(app, kAXFocusedUIElementAttribute), string(field, kAXSubroleAttribute) != (kAXSecureTextFieldSubrole as String) else { throw PilotError("Focus an editable, non-password field and retry.") }
            let role = string(field, kAXRoleAttribute)
            var editable = DarwinBoolean(false)
            _ = AXUIElementIsAttributeSettable(field, kAXValueAttribute as CFString, &editable)
            guard ["AXTextField", "AXTextArea", "AXComboBox"].contains(role) || editable.boolValue else { throw PilotError("The focused control is not an editable text field.") }
            // Recheck focus immediately before injecting Unicode. Do not use or replace the clipboard.
            guard NSWorkspace.shared.frontmostApplication?.processIdentifier == current.processIdentifier,
                  let focused = element(app, kAXFocusedUIElementAttribute), CFEqual(field, focused) else { throw PilotError("Focus changed. Nothing was typed.") }
            let text = action.text!
            let before = string(field, kAXValueAttribute)
            let utf16 = Array(text.utf16)
            // One event per chunk, never a Return key. Avoid splitting surrogate pairs.
            var offset = 0
            while offset < utf16.count {
                try Task.checkCancellation()
                guard NSWorkspace.shared.frontmostApplication?.processIdentifier == current.processIdentifier,
                      let focused = element(app, kAXFocusedUIElementAttribute), CFEqual(field, focused) else { throw PilotError("Focus changed during typing. Remaining text was stopped.") }
                var end = min(offset + 128, utf16.count)
                if end < utf16.count && (0xD800...0xDBFF).contains(utf16[end - 1]) { end -= 1 }
                let chunk = Array(utf16[offset..<end])
                guard let down = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true), let up = CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: false) else { throw PilotError("Could not create a typing event.") }
                chunk.withUnsafeBufferPointer { pointer in
                    down.keyboardSetUnicodeString(stringLength: chunk.count, unicodeString: pointer.baseAddress)
                    up.keyboardSetUnicodeString(stringLength: chunk.count, unicodeString: pointer.baseAddress)
                }
                down.post(tap: .cghidEventTap); up.post(tap: .cghidEventTap)
                offset = end
                try await Task.sleep(nanoseconds: 20_000_000)
            }
            try await Task.sleep(nanoseconds: 100_000_000)
            let after = string(field, kAXValueAttribute)
            return ActionOutcome(message: "Typed text", verified: after != before && after.contains(text))
        }
        if ["play", "pause", "toggle_playback", "fullscreen_video", "exit_fullscreen_video"].contains(action.kind) { return try await video(action, current) }
        if action.kind == "scroll" {
            current.activate(options: [.activateAllWindows])
            try await waitUntil { NSWorkspace.shared.frontmostApplication?.processIdentifier == current.processIdentifier }
            guard let win = window(for: current), let frame = frame(win) else { throw PilotError("No target window is available to scroll.") }
            // Scroll events target the pointer position. Place the event inside the intended window without moving the pointer.
            let direction = action.direction!
            let vertical: Int32 = direction == "up" ? 6 : direction == "down" ? -6 : 0
            let horizontal: Int32 = direction == "left" ? 6 : direction == "right" ? -6 : 0
            guard let event = CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 2, wheel1: vertical, wheel2: horizontal, wheel3: 0) else { throw PilotError("Could not create scroll event.") }
            event.location = CGPoint(x: frame.midX, y: frame.midY)
            event.post(tap: .cghidEventTap)
            return ActionOutcome(message: "Scrolled \(direction)", verified: false)
        }
        guard let win = window(for: current) else { throw PilotError("No window is available in \(current.localizedName ?? "the target app").") }
        switch action.kind {
        case "close_window":
            guard let button = element(win, kAXCloseButtonAttribute) else { throw PilotError("This window has no accessible close button.") }
            try press(button)
            try await Task.sleep(nanoseconds: 150_000_000)
            let stillThere = (attribute(AXUIElementCreateApplication(current.processIdentifier), kAXWindowsAttribute) as? [AXUIElement] ?? []).contains { CFEqual($0, win) }
            return ActionOutcome(message: stillThere ? "Close requested; check for an app dialog" : "Closed window", verified: !stillThere)
        case "minimize_window": try set(win, kAXMinimizedAttribute, kCFBooleanTrue)
        case "restore_window":
            try set(win, kAXMinimizedAttribute, kCFBooleanFalse)
            if number(win, "AXFullScreen")?.boolValue == true { try set(win, "AXFullScreen", kCFBooleanFalse) }
            if let saved = savedFrames[CFHash(win)] { try await setFrame(win, saved); savedFrames.removeValue(forKey: CFHash(win)) }
            current.activate(options: [.activateAllWindows]); _ = AXUIElementPerformAction(win, kAXRaiseAction as CFString)
        case "maximize_window":
            guard number(win, "AXFullScreen")?.boolValue != true else { throw PilotError("Exit full screen before maximising the window.") }
            if savedFrames[CFHash(win)] == nil, let old = frame(win) { savedFrames[CFHash(win)] = old }
            try set(win, kAXMinimizedAttribute, kCFBooleanFalse)
            let winFrame = frame(win) ?? .zero
            let screen = NSScreen.screens.first { screen in
                let bounds = CGDisplayBounds(displayID(screen))
                return bounds.intersects(winFrame)
            } ?? NSScreen.main
            guard let screen else { throw PilotError("No display available.") }
            let mainHeight = CGDisplayBounds(CGMainDisplayID()).height
            let visible = screen.visibleFrame
            try await setFrame(win, CGRect(x: visible.minX, y: mainHeight - visible.maxY, width: visible.width, height: visible.height))
        case "fullscreen_window", "exit_fullscreen_window":
            let wanted = action.kind == "fullscreen_window"
            if number(win, "AXFullScreen")?.boolValue != wanted {
                try set(win, "AXFullScreen", wanted ? kCFBooleanTrue : kCFBooleanFalse)
                try await waitUntil { self.number(win, "AXFullScreen")?.boolValue == wanted }
            }
        default: throw PilotError("Unsupported action: \(action.kind)")
        }
        try await Task.sleep(nanoseconds: 200_000_000)
        let verified: Bool
        switch action.kind {
        case "minimize_window": verified = number(win, kAXMinimizedAttribute)?.boolValue == true
        case "restore_window": verified = number(win, kAXMinimizedAttribute)?.boolValue == false && number(win, "AXFullScreen")?.boolValue != true
        case "fullscreen_window": verified = number(win, "AXFullScreen")?.boolValue == true
        case "exit_fullscreen_window": verified = number(win, "AXFullScreen")?.boolValue == false
        case "maximize_window": verified = (frame(win)?.width ?? 0) > 0 // Detailed frame verification below.
        default: verified = false
        }
        return ActionOutcome(message: action.kind.replacingOccurrences(of: "_", with: " "), verified: verified)
    }

    private func resolve(_ name: String) throws -> AppInfo {
        let aliases = ["chrome":"com.google.Chrome", "notes":"com.apple.Notes", "safari":"com.apple.Safari", "finder":"com.apple.finder", "vs code":"com.microsoft.VSCode", "vscode":"com.microsoft.VSCode", "system settings":"com.apple.systempreferences"]
        let query = name.lowercased()
        if let id = aliases[query], let match = apps.first(where: { $0.bundleID == id }) { return match }
        let exact = apps.filter { $0.name.caseInsensitiveCompare(name) == .orderedSame || $0.bundleID.caseInsensitiveCompare(name) == .orderedSame }
        if exact.count == 1 { return exact[0] }
        let partial = apps.filter { $0.name.lowercased().contains(query) }
        if exact.isEmpty && partial.count == 1 { return partial[0] }
        if exact.count > 1 || partial.count > 1 { throw PilotError("Several apps match ‘\(name)’. Repeat the instruction using a full app name: \((exact.isEmpty ? partial : exact).map(\.name).joined(separator: ", ")).") }
        throw PilotError("No installed app matches ‘\(name)’.")
    }

    private func video(_ action: Action, _ app: NSRunningApplication) async throws -> ActionOutcome {
        let context = context(for: app)
        func matching(_ pattern: String) -> [UIControl] {
            context.controls.filter { ["AXButton", "AXMenuItem"].contains($0.role) && $0.label.range(of: pattern, options: [.regularExpression, .caseInsensitive]) != nil }
        }
        let play = matching(#"^(play|resume)(?:\b|$)"#)
        let pause = matching(#"^pause(?:\b|$)"#)
        let enter = matching(#"^(?:enter )?(?:full.?screen)(?:\b|$)"#)
        let exit = matching(#"^(?:exit|leave) full.?screen(?:\b|$)"#)
        let wanted: [UIControl]
        switch action.kind {
        case "play":
            if pause.count == 1 && play.isEmpty { return ActionOutcome(message: "Already playing", verified: true) }
            wanted = play
        case "pause":
            if play.count == 1 && pause.isEmpty { return ActionOutcome(message: "Already paused", verified: true) }
            wanted = pause
        case "toggle_playback": wanted = play + pause
        case "fullscreen_video":
            if exit.count == 1 && enter.isEmpty { return ActionOutcome(message: "Video is already full screen", verified: true) }
            wanted = enter
        default:
            if enter.count == 1 && exit.isEmpty { return ActionOutcome(message: "Video is already outside full screen", verified: true) }
            wanted = exit
        }
        guard wanted.count == 1, let control = wanted.first, let node = elements[control.id] else {
            throw PilotError(wanted.count > 1 ? "Several video players match. Focus the intended player and retry." : "No accessible player control was found. Show the player's controls and retry. Window full screen is a separate command.")
        }
        try press(node)
        try await Task.sleep(nanoseconds: 250_000_000)
        let after = self.context(for: app)
        let expectedPattern = action.kind == "play" ? #"^pause\b"# : action.kind == "pause" ? #"^(play|resume)\b"# : action.kind == "fullscreen_video" ? #"^(exit|leave) full.?screen"# : action.kind == "exit_fullscreen_video" ? #"^(?:enter )?full.?screen"# : nil
        let verified = expectedPattern.map { pattern in after.controls.contains { $0.label.range(of: pattern, options: [.regularExpression, .caseInsensitive]) != nil } } ?? false
        return ActionOutcome(message: action.kind.replacingOccurrences(of: "_", with: " "), verified: verified)
    }

    private func audio(_ action: Action) throws -> ActionOutcome {
        var device = AudioDeviceID(0)
        var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultOutputDevice, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var size = UInt32(MemoryLayout<AudioDeviceID>.size)
        guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &device) == noErr, device != 0 else { throw PilotError("No audio output device.") }
        if ["mute", "unmute"].contains(action.kind) {
            address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyMute, mScope: kAudioDevicePropertyScopeOutput, mElement: kAudioObjectPropertyElementMain)
            var value: UInt32 = action.kind == "mute" ? 1 : 0
            guard AudioObjectSetPropertyData(device, &address, 0, nil, UInt32(MemoryLayout<UInt32>.size), &value) == noErr else { throw PilotError("This output device does not expose a mute control.") }
            var actual: UInt32 = 0; size = UInt32(MemoryLayout<UInt32>.size)
            let read = AudioObjectGetPropertyData(device, &address, 0, nil, &size, &actual)
            return ActionOutcome(message: action.kind, verified: read == noErr && actual == value)
        }
        address = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyVolumeScalar, mScope: kAudioDevicePropertyScopeOutput, mElement: kAudioObjectPropertyElementMain)
        var old = Float32(0); size = UInt32(MemoryLayout<Float32>.size)
        guard AudioObjectGetPropertyData(device, &address, 0, nil, &size, &old) == noErr else { throw PilotError("This output device does not expose system volume. Use its hardware volume control.") }
        var value = Float32(action.value.map { $0 / 100 } ?? Double(old + (action.kind == "volume_up" ? 0.1 : -0.1)))
        value = max(0, min(1, value))
        guard AudioObjectSetPropertyData(device, &address, 0, nil, size, &value) == noErr else { throw PilotError("Could not change output volume.") }
        var actual = Float32(0)
        let read = AudioObjectGetPropertyData(device, &address, 0, nil, &size, &actual)
        return ActionOutcome(message: "Volume \(Int(value * 100))%", verified: read == noErr && abs(actual - value) < 0.02)
    }

    private func brightness(_ action: Action) throws -> ActionOutcome {
        guard let lib = dlopen("/System/Library/PrivateFrameworks/DisplayServices.framework/DisplayServices", RTLD_LAZY),
              let getSymbol = dlsym(lib, "DisplayServicesGetBrightness"), let setSymbol = dlsym(lib, "DisplayServicesSetBrightness") else { throw PilotError("Built-in brightness control is unavailable on this macOS version.") }
        defer { dlclose(lib) }
        typealias Get = @convention(c) (UInt32, UnsafeMutablePointer<Float>) -> Int32
        typealias Set = @convention(c) (UInt32, Float) -> Int32
        let get = unsafeBitCast(getSymbol, to: Get.self), set = unsafeBitCast(setSymbol, to: Set.self)
        guard let screen = NSScreen.screens.first(where: { CGDisplayIsBuiltin(displayID($0)) != 0 }) else { throw PilotError("No built-in display found. External-monitor brightness is not supported.") }
        let id = displayID(screen)
        var old: Float = 0
        guard get(id, &old) == 0 else { throw PilotError("Could not read built-in brightness.") }
        let value = max(0, min(1, action.value.map { Float($0 / 100) } ?? old + (action.kind == "brightness_up" ? 0.1 : -0.1)))
        guard set(id, value) == 0 else { throw PilotError("Could not change brightness.") }
        var actual: Float = 0
        return ActionOutcome(message: "Brightness \(Int(value * 100))%", verified: get(id, &actual) == 0 && abs(actual - value) < 0.03)
    }

    func window(for app: NSRunningApplication) -> AXUIElement? {
        let ax = AXUIElementCreateApplication(app.processIdentifier)
        let windows = attribute(ax, kAXWindowsAttribute) as? [AXUIElement] ?? []
        if let cached = chosenWindows[app.processIdentifier], windows.contains(where: { CFEqual($0, cached) }), number(cached, kAXMinimizedAttribute)?.boolValue == true { return cached }
        let selected = element(ax, kAXFocusedWindowAttribute) ?? windows.first
        if let selected { chosenWindows[app.processIdentifier] = selected }
        return selected
    }
    private func attribute(_ node: AXUIElement, _ key: String) -> CFTypeRef? {
        var value: CFTypeRef?
        return AXUIElementCopyAttributeValue(node, key as CFString, &value) == .success ? value : nil
    }
    private func string(_ node: AXUIElement, _ key: String) -> String { attribute(node, key) as? String ?? "" }
    private func number(_ node: AXUIElement, _ key: String) -> NSNumber? { attribute(node, key) as? NSNumber }
    private func element(_ node: AXUIElement, _ key: String) -> AXUIElement? {
        guard let value = attribute(node, key), CFGetTypeID(value) == AXUIElementGetTypeID() else { return nil }
        return unsafeBitCast(value, to: AXUIElement.self)
    }
    private func children(_ node: AXUIElement) -> [AXUIElement] { attribute(node, kAXChildrenAttribute) as? [AXUIElement] ?? [] }
    private func set(_ node: AXUIElement, _ key: String, _ value: CFTypeRef) throws {
        let error = AXUIElementSetAttributeValue(node, key as CFString, value)
        guard error == .success else { throw PilotError("The app rejected \(key) (Accessibility \(error.rawValue)).") }
    }
    private func press(_ node: AXUIElement) throws {
        guard AXUIElementPerformAction(node, kAXPressAction as CFString) == .success else { throw PilotError("The app rejected this control action.") }
    }
    private func frame(_ node: AXUIElement) -> CGRect? {
        guard let p = attribute(node, kAXPositionAttribute), CFGetTypeID(p) == AXValueGetTypeID(), let s = attribute(node, kAXSizeAttribute), CFGetTypeID(s) == AXValueGetTypeID() else { return nil }
        var point = CGPoint.zero, size = CGSize.zero
        guard AXValueGetValue(unsafeBitCast(p, to: AXValue.self), .cgPoint, &point), AXValueGetValue(unsafeBitCast(s, to: AXValue.self), .cgSize, &size) else { return nil }
        return CGRect(origin: point, size: size)
    }
    private func setFrame(_ node: AXUIElement, _ rect: CGRect) async throws {
        var p = rect.origin, s = rect.size
        try set(node, kAXSizeAttribute, AXValueCreate(.cgSize, &s)!)
        try await Task.sleep(nanoseconds: 100_000_000)
        try set(node, kAXPositionAttribute, AXValueCreate(.cgPoint, &p)!)
        // Window layout and Space transitions are asynchronous in the target app.
        for _ in 0..<20 {
            try Task.checkCancellation()
            if let actual = frame(node), abs(actual.width - rect.width) < 5, abs(actual.height - rect.height) < 5, abs(actual.minX - rect.minX) < 5, abs(actual.minY - rect.minY) < 5 { return }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        throw PilotError("The app constrained the requested window frame.")
    }
    private func displayID(_ screen: NSScreen) -> CGDirectDisplayID { (screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber)?.uint32Value ?? CGMainDisplayID() }
    private func waitUntil(_ condition: () -> Bool) async throws {
        for _ in 0..<20 { try Task.checkCancellation(); if condition() { return }; try await Task.sleep(nanoseconds: 50_000_000) }
        throw PilotError("The app did not become active. Retry when macOS finishes switching windows.")
    }
}
