import AppKit
import AVFoundation
import ApplicationServices
import FnPilotCore

@MainActor
final class PilotDelegate: NSObject, NSApplicationDelegate, NSMenuDelegate {
    var controller: AppController!
    var item: NSStatusItem!
    var stateItem = NSMenuItem(title: "", action: nil, keyEquivalent: "")
    var pauseItem = NSMenuItem(title: "Pause", action: #selector(togglePause), keyEquivalent: "")
    func applicationDidFinishLaunching(_ notification: Notification) {
        controller = AppController()
        let bar = NSMenu()
        let appMenu = NSMenu()
        appMenu.addItem(NSMenuItem(title: "About FnPilot", action: #selector(NSApplication.orderFrontStandardAboutPanel(_:)), keyEquivalent: ""))
        let preferences = NSMenuItem(title: "Settings…", action: #selector(settings), keyEquivalent: ","); preferences.target = self
        appMenu.addItem(preferences); appMenu.addItem(.separator())
        appMenu.addItem(NSMenuItem(title: "Quit FnPilot", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        let appEntry = NSMenuItem(); appEntry.submenu = appMenu; bar.addItem(appEntry)
        let edit = NSMenu(title: "Edit")
        for (title, selector, key) in [("Undo", "undo:", "z"), ("Cut", "cut:", "x"), ("Copy", "copy:", "c"), ("Paste", "paste:", "v"), ("Select All", "selectAll:", "a")] {
            edit.addItem(NSMenuItem(title: title, action: NSSelectorFromString(selector), keyEquivalent: key))
        }
        let editEntry = NSMenuItem(title: "Edit", action: nil, keyEquivalent: ""); editEntry.submenu = edit; bar.addItem(editEntry)
        NSApp.mainMenu = bar
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        item.button?.image = NSImage(systemSymbolName: "waveform", accessibilityDescription: "FnPilot")
        let menu = NSMenu(); menu.delegate = self
        menu.addItem(stateItem); menu.addItem(.separator())
        let settings = NSMenuItem(title: "Settings…", action: #selector(settings), keyEquivalent: ","); settings.target = self; menu.addItem(settings)
        pauseItem.target = self; menu.addItem(pauseItem)
        let cancel = NSMenuItem(title: "Cancel current instruction", action: #selector(cancel), keyEquivalent: ""); cancel.target = self; menu.addItem(cancel)
        menu.addItem(.separator()); menu.addItem(NSMenuItem(title: "Quit FnPilot", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q"))
        item.menu = menu
        controller.menuChanged = { [weak self] in self?.updateMenu() }
        updateMenu()
        if !UserDefaults.standard.bool(forKey: "hasOpened") { UserDefaults.standard.set(true, forKey: "hasOpened"); controller.showSettings() }
    }
    func menuWillOpen(_ menu: NSMenu) { controller.refreshPermissions(); updateMenu() }
    func updateMenu() { stateItem.title = controller.paused ? "Paused" : controller.hotkeyReady ? "Hold Fn to speak" : "Fn key needs setup"; pauseItem.title = controller.paused ? "Resume" : "Pause" }
    @objc func settings() { controller.showSettings() }
    @objc func togglePause() { controller.togglePause(); updateMenu() }
    @objc func cancel() { controller.cancel() }
    func applicationWillTerminate(_ notification: Notification) { controller.shutdown() }
}

if CommandLine.arguments.contains("--speech-benchmark") {
    Task {
        do { try await SpeechBenchmark.run(arguments: CommandLine.arguments); exit(0) }
        catch { fputs("\(error.localizedDescription)\n", stderr); exit(1) }
    }
    RunLoop.main.run()
} else if CommandLine.arguments.contains("--help") {
    print("FnPilot headless modes: --diagnostics | --prepare-speech | --transcribe AUDIO_FILE. Use fnpilot-cli for command parsing. These modes do not open the GUI.")
    print("Speech benchmark: --speech-benchmark MANIFEST --speech-model VARIANT --benchmark-output REPORT --benchmark-repeats 3. Requires local model files; never executes actions.")
} else if CommandLine.arguments.contains("--diagnostics") {
    print("Accessibility: \(AXIsProcessTrusted())")
    print("Input Monitoring: \(CGPreflightListenEventAccess())")
    print("Microphone: \(AVCaptureDevice.authorizationStatus(for: .audio).rawValue) (3=authorized)")
    print("Speech model folder: \(UserDefaults.standard.string(forKey: "speechModelFolder") ?? "not installed")")
} else if CommandLine.arguments.contains("--prepare-speech") || CommandLine.arguments.contains("--transcribe") {
    Task {
        do {
            let speech = WhisperSpeechRecognizer()
            try await speech.prepare(download: CommandLine.arguments.contains("--prepare-speech"))
            if let position = CommandLine.arguments.firstIndex(of: "--transcribe"), position + 1 < CommandLine.arguments.count {
                print(try await speech.transcribeFile(CommandLine.arguments[position + 1]))
            } else { print("English speech model ready") }
            exit(0)
        } catch { fputs("\(error.localizedDescription)\n", stderr); exit(1) }
    }
    RunLoop.main.run()
} else {
    MainActor.assumeIsolated {
        let app = NSApplication.shared
        let delegate = PilotDelegate()
        app.delegate = delegate
        app.setActivationPolicy(.accessory)
        app.run()
    }
}
