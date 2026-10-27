import AppKit
import SwiftUI
import AVFoundation
import ApplicationServices
import ServiceManagement
import FnPilotCore

@MainActor
final class AppController: ObservableObject {
    @Published var configuration: ModelConfiguration {
        didSet { if let data = try? JSONEncoder().encode(configuration) { UserDefaults.standard.set(data, forKey: "modelConfiguration") } }
    }
    @Published var allowModelActions = UserDefaults.standard.bool(forKey: "allowModelActions") {
        didSet { UserDefaults.standard.set(allowModelActions, forKey: "allowModelActions") }
    }
    @Published var status = "Setting up FnPilot…"
    @Published var transcript = ""
    @Published var phase = "Ready"
    @Published var speechReady = false
    @Published var speechBusy = false
    @Published var hotkeyReady = false
    @Published var paused = false
    @Published var working = false
    @Published var microphoneGranted = AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
    @Published var accessibilityGranted = AXIsProcessTrusted()
    @Published var inputGranted = CGPreflightListenEventAccess()
    @Published var diagnostic = ""
    @Published var testInstruction = ""
    @Published var launchAtLogin = SMAppService.mainApp.status == .enabled
    @Published var downloadingModel = false
    let managedRuntime = ManagedRuntime()
    @Published var serverBusy = false
    @Published var serverRunning = false
    let executor = MacExecutor()
    let speech: any SpeechRecognizer = WhisperSpeechRecognizer()
    private let microphone = MicrophoneRecorder()
    private let hotkey = HotkeyMonitor()
    private var task: Task<Void, Never>?
    private var cleanupTask: Task<Void, Never>?
    private var recording = false
    private var originalTarget: NSRunningApplication?
    private var lastExternalTarget: NSRunningApplication?
    private var workspaceObserver: NSObjectProtocol?
    private var settingsWindow: NSWindow?
    private var overlay: NSPanel?
    private var clarification: (part: String, message: String, remaining: [String], target: NSRunningApplication?)?
    private var recordingID = UUID()
    var menuChanged: (() -> Void)?

    init() {
        let initial = ManagedRuntime.binary == nil ? ModelConfiguration() : ModelConfiguration(runtime: .openAICompatible, endpoint: ManagedRuntime.endpoint, outputMode: .toolCalls)
        configuration = UserDefaults.standard.data(forKey: "modelConfiguration").flatMap { try? JSONDecoder().decode(ModelConfiguration.self, from: $0) } ?? initial
        executor.refreshApps()
        lastExternalTarget = NSWorkspace.shared.frontmostApplication
        workspaceObserver = NSWorkspace.shared.notificationCenter.addObserver(forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main) { [weak self] notification in
            guard let app = notification.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication, app.processIdentifier != ProcessInfo.processInfo.processIdentifier else { return }
            MainActor.assumeIsolated { self?.lastExternalTarget = app }
        }
        hotkey.onSignal = { [weak self] signal in
            switch signal { case .start: self?.beginRecording(); case .finish: self?.finishRecording(); case .cancel: self?.cancel() }
        }
        hotkey.onInterruption = { [weak self] in self?.cancel() }
        if inputGranted { enableHotkey(request: false) }
        prepareSpeech(download: false)
        if UserDefaults.standard.bool(forKey: "useBundledRuntime") { startBundledServer() }
    }
    func refreshPermissions() {
        microphoneGranted = AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
        accessibilityGranted = AXIsProcessTrusted()
        inputGranted = CGPreflightListenEventAccess()
        if inputGranted && !hotkeyReady { enableHotkey(request: false) }
    }
    func requestMicrophone() {
        Task { microphoneGranted = await AVCaptureDevice.requestAccess(for: .audio); refreshPermissions() }
    }
    func requestAccessibility() {
        _ = AXIsProcessTrustedWithOptions([kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary)
        openPrivacy("Privacy_Accessibility")
    }
    func enableHotkey(request: Bool = true) {
        if request { _ = CGRequestListenEventAccess() }
        do { try hotkey.start(); hotkeyReady = true; inputGranted = true; status = "Fn key enabled" }
        catch { hotkeyReady = false; status = error.localizedDescription; if request { openPrivacy("Privacy_ListenEvent") } }
        menuChanged?()
    }
    func openPrivacy(_ pane: String) { if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?\(pane)") { NSWorkspace.shared.open(url) } }
    func openKeyboardSettings() { if let url = URL(string: "x-apple.systempreferences:com.apple.Keyboard-Settings.extension") { NSWorkspace.shared.open(url) } }
    func prepareSpeech(download: Bool) {
        guard !speechBusy else { return }
        speechBusy = true; status = download ? "Downloading / loading English speech model…" : "Loading local speech model…"
        Task {
            do { try await speech.prepare(download: download); speechReady = true; status = "Hold Fn to speak" }
            catch { speechReady = false; status = error.localizedDescription }
            speechBusy = false; menuChanged?()
        }
    }
    func beginRecording() {
        guard !paused else { return }
        guard !recording else { return }
        task?.cancel(); cleanupTask?.cancel()
        let front = NSWorkspace.shared.frontmostApplication
        originalTarget = front?.processIdentifier == ProcessInfo.processInfo.processIdentifier ? lastExternalTarget : front
        refreshPermissions()
        guard speechReady else { status = "Load the speech model in Settings first"; phase = "Setup needed"; showOverlay(); return }
        do {
            try microphone.start()
            recording = true; recordingID = UUID(); working = true
            transcript = ""; phase = "Listening"; status = "Release Fn to act · Esc to cancel"; showOverlay()
        } catch { status = error.localizedDescription; phase = "Setup needed"; showOverlay(); scheduleClear() }
    }
    func finishRecording() {
        guard recording else { return }
        recording = false
        let id = recordingID
        do {
            let samples = try microphone.stop()
            phase = "Transcribing"; status = "Recognizing speech locally…"
            task = Task {
                do {
                    let text = try await speech.transcribe(samples)
                    try Task.checkCancellation()
                    guard id == recordingID else { return }
                    transcript = text
                    await perform(text, target: originalTarget)
                } catch { if !Task.isCancelled { fail(error) } }
            }
        } catch { fail(error) }
    }
    func runTypedInstruction() {
        let text = testInstruction.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !working else { return }
        let target = lastExternalTarget
        recordingID = UUID()
        testInstruction = ""; transcript = text; working = true; showOverlay()
        task = Task { await perform(text, target: target) }
    }
    private func perform(_ input: String, target initial: NSRunningApplication?) async {
        working = true
        let operation = recordingID
        defer { if operation == recordingID { working = false; menuChanged?(); scheduleClear() } }
        var target = initial
        var parts = CommandSequence.parts(input)
        var replyContext: String?
        var resolvedReply: Interpretation?
        if let pending = clarification {
            target = pending.target
            clarification = nil
            if ["cancel", "stop", "never mind"].contains(input.lowercased().trimmingCharacters(in: .whitespacesAndNewlines)) { cancel(); return }
            if DirectParser().parse(input)?.status == "actions" {
                parts = CommandSequence.parts(input)
            } else if DirectParser().parse(pending.part)?.status == "clarify", let range = pending.part.range(of: #"^(?:please\s+)?(?:open|launch|start|switch to|focus|quit|exit|close)\s+"#, options: [.regularExpression, .caseInsensitive]) {
                parts = [String(pending.part[range]) + input] + pending.remaining
            } else if DirectUIParser().parse(pending.part, context: executor.context(for: target))?.status == "clarify", let answer = DirectUIParser().answer(original: pending.part, reply: input, context: executor.context(for: target)) {
                resolvedReply = answer
                parts = [pending.part] + pending.remaining
            } else {
                parts = ["\(pending.part)\nAnswer to clarification: \(input)"] + pending.remaining
                replyContext = pending.message
            }
        }
        var total = 0
        var outcomes: [ActionOutcome] = []
        do {
            while !parts.isEmpty {
                try Task.checkCancellation()
                let part = parts.removeFirst()
                phase = "Understanding"; status = "Interpreting locally…"
                let direct = replyContext == nil ? DirectParser().parse(part) : nil
                var context = direct == nil ? executor.context(for: target, clarification: replyContext) : CommandContext(appName: target?.localizedName ?? "", bundleID: target?.bundleIdentifier ?? "")
                let result: Interpretation
                if let answer = resolvedReply { result = answer; resolvedReply = nil }
                else if let direct { result = direct }
                else if replyContext == nil, let namedControl = DirectUIParser().parse(part, context: context) { result = namedControl }
                else {
                    guard allowModelActions else { throw PilotError("Model fallback is off because the tested small models did not meet the accuracy target. Use an explicit command or an exact control label. You can opt into experimental model actions in Settings.") }
                    result = try await LocalModelInterpreter(configuration: configuration).interpret(part, context: context)
                }
                try Task.checkCancellation(); try ActionRegistry.validate(result)
                if result.status == "clarify" {
                    clarification = (part, result.message ?? "Which target?", parts, target)
                    phase = "Clarification needed"; status = (result.message ?? "Which target?") + " Hold Fn to answer."
                    return
                }
                if result.status == "unsupported" { throw PilotError(result.message ?? "That action is not supported.") }
                replyContext = nil
                for action in result.actions {
                    try Task.checkCancellation()
                    total += 1
                    guard total <= 20 else { throw PilotError("Stopped after 20 actions. Give a shorter instruction.") }
                    phase = "Acting"; status = action.kind.replacingOccurrences(of: "_", with: " ")
                    // A freshly observed tree must still contain any control returned by the interpreter.
                    if let controlID = action.controlID, !context.controls.contains(where: { $0.id == controlID }) { throw PilotError("The model selected a control that is not currently available.") }
                    let outcome = try await executor.execute(action, target: &target)
                    outcomes.append(outcome)
                    try Task.checkCancellation()
                    if result.actions.contains(where: { $0.controlID != nil }) { context = executor.context(for: target).compacted(for: part) }
                }
            }
            phase = outcomes.allSatisfy(\.verified) ? "Done" : "Action sent"
            status = outcomes.map { $0.message + ($0.verified ? " ✓" : " (check app)") }.joined(separator: " · ")
        } catch { if !Task.isCancelled { fail(error) } }
    }
    func cancel() {
        task?.cancel(); recordingID = UUID(); microphone.cancel(); recording = false; working = false; clarification = nil
        transcript = ""; overlay?.orderOut(nil); phase = "Ready"; status = paused ? "Paused" : "Hold Fn to speak"
        menuChanged?()
    }
    func togglePause() { paused.toggle(); cancel() }
    private func fail(_ error: Error) { working = false; phase = "Could not finish"; status = error.localizedDescription; showOverlay(); scheduleClear() }
    private func scheduleClear() {
        cleanupTask?.cancel()
        let delay: UInt64
        switch phase {
        case "Done": delay = 850_000_000
        case "Action sent": delay = 1_500_000_000
        case "Clarification needed": delay = 5_000_000_000
        default: delay = 3_000_000_000
        }
        cleanupTask = Task {
            try? await Task.sleep(nanoseconds: delay)
            guard !Task.isCancelled, !recording, !working else { return }
            transcript = ""; overlay?.orderOut(nil)
        }
    }
    func testConnection() {
        diagnostic = "Checking model; no actions will execute…"
        let config = configuration
        Task {
            do {
                let result = try await LocalModelInterpreter(configuration: config).interpret("volume up", context: CommandContext())
                guard result.actions == [Action("volume_up")] else { throw PilotError("The model connected but did not interpret the test command correctly. Try another model or output mode.") }
                diagnostic = "Local model connected and returned a valid command. No actions executed."
            } catch { diagnostic = error.localizedDescription }
        }
    }
    func downloadCommandModel() {
        guard !downloadingModel else { return }
        do { _ = try configuration.validatedURL(path: "api/tags") } catch { diagnostic = error.localizedDescription; return }
        guard configuration.runtime == .ollama else { diagnostic = "Download the model using your selected runtime, then test the connection."; return }
        downloadingModel = true; diagnostic = "Downloading selected model with Ollama…"
        let model = configuration.model
        Task {
            let result = await Task.detached { () -> String in
                let candidates = ["/opt/homebrew/bin/ollama", "/usr/local/bin/ollama", "/Applications/Ollama.app/Contents/Resources/ollama"]
                guard let binary = candidates.first(where: { FileManager.default.isExecutableFile(atPath: $0) }) else { return "Ollama was not found. Install it or choose another local runtime." }
                let process = Process(); process.executableURL = URL(fileURLWithPath: binary); process.arguments = ["pull", model]
                let pipe = Pipe(); process.standardOutput = pipe; process.standardError = pipe
                do { try process.run(); _ = pipe.fileHandleForReading.readDataToEndOfFile(); process.waitUntilExit(); return process.terminationStatus == 0 ? "Model downloaded. Test the connection next." : "Download failed. Check your connection and model identifier in Ollama." }
                catch { return error.localizedDescription }
            }.value
            downloadingModel = false; diagnostic = result
        }
    }
    func startBundledServer(chooseFile: Bool = false) {
        guard !serverBusy else { return }
        let file = chooseFile ? managedRuntime.selectFile() : nil
        if chooseFile && file == nil { return }
        serverBusy = true; diagnostic = "Starting the bundled local model server…"
        let model = configuration.model
        Task {
            defer { serverBusy = false; serverRunning = managedRuntime.running }
            do {
                try await managedRuntime.start(model: model, file: file)
                configuration.runtime = .openAICompatible
                configuration.endpoint = ManagedRuntime.endpoint
                diagnostic = "Bundled server is running locally. Test the connection next."
            } catch { diagnostic = error.localizedDescription }
        }
    }
    func stopBundledServer() {
        managedRuntime.stop(); serverRunning = false
        UserDefaults.standard.set(false, forKey: "useBundledRuntime")
    }
    func shutdown() { cancel(); managedRuntime.stop() }
    func setLogin(_ enabled: Bool) {
        do { if enabled { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() }; launchAtLogin = enabled }
        catch { diagnostic = error.localizedDescription; launchAtLogin = SMAppService.mainApp.status == .enabled }
    }
    func showSettings() {
        if settingsWindow == nil {
            let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 650, height: 730), styleMask: [.titled, .closable, .miniaturizable, .resizable], backing: .buffered, defer: false)
            window.title = "FnPilot Settings"; window.minSize = NSSize(width: 600, height: 610); window.isReleasedWhenClosed = false
            window.contentViewController = NSHostingController(rootView: SettingsView(controller: self))
            window.center(); settingsWindow = window
        }
        refreshPermissions(); settingsWindow?.makeKeyAndOrderFront(nil); NSApp.activate(ignoringOtherApps: true)
    }
    private func showOverlay() {
        if overlay == nil {
            let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 280, height: 64), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
            panel.isFloatingPanel = true; panel.hidesOnDeactivate = false; panel.level = .floating; panel.backgroundColor = .clear; panel.isOpaque = false; panel.hasShadow = true; panel.ignoresMouseEvents = true
            panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
            panel.contentView = NSHostingView(rootView: StatusOverlay(controller: self)); overlay = panel
        }
        if let screen = NSScreen.main { overlay?.setFrameOrigin(NSPoint(x: screen.visibleFrame.midX - 140, y: screen.visibleFrame.minY + 24)) }
        overlay?.orderFrontRegardless()
    }
}
