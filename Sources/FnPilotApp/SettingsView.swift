import SwiftUI
import FnPilotCore

struct SettingsView: View {
    @ObservedObject var controller: AppController
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: "waveform.circle.fill").font(.system(size: 42)).foregroundStyle(.teal)
                VStack(alignment: .leading, spacing: 3) {
                    Text("FnPilot").font(.largeTitle.bold())
                    Text("Hold Fn. Speak. Release to act.").foregroundStyle(.secondary)
                }
                Spacer()
                Text("LOCAL").font(.caption.bold()).padding(.horizontal, 9).padding(.vertical, 5).background(.teal.opacity(0.12), in: Capsule())
            }.padding(24)
            Divider()
            Form {
                Section("Setup") {
                    permission("Microphone", granted: controller.microphoneGranted) { controller.requestMicrophone() }
                    permission("Accessibility", granted: controller.accessibilityGranted) { controller.requestAccessibility() }
                    permission("Fn key / Input Monitoring", granted: controller.hotkeyReady) { controller.enableHotkey() }
                    HStack {
                        Label(controller.speechReady ? "English speech model loaded" : "English speech model needed", systemImage: controller.speechReady ? "checkmark.circle.fill" : "arrow.down.circle")
                        Spacer()
                        if controller.speechBusy { ProgressView().controlSize(.small) }
                        else { Button(controller.speechReady ? "Repair / reload" : "Download / load") { controller.prepareSpeech(download: true) } }
                    }
                    Text("Set Keyboard → Press 🌐 key to → Do Nothing. Disable Wispr Flow’s Fn shortcut so only FnPilot listens. Recordings stay in memory; commands execute when you release Fn.").font(.caption).foregroundStyle(.secondary)
                    HStack { Button("Keyboard settings") { controller.openKeyboardSettings() }; Button("Recheck permissions") { controller.refreshPermissions() } }
                }
                Section("Command model — experimental") {
                    Toggle("Enable model-generated actions", isOn: $controller.allowModelActions)
                    Text("The tested small models did not meet the 95% command accuracy target. Fallback is off by default. Explicit commands and exact control labels work without a command model. Enabling fallback lets the selected model act immediately.").font(.caption).foregroundStyle(.secondary)
                    Picker("Runtime", selection: $controller.configuration.runtime) {
                        Text("Ollama").tag(Runtime.ollama)
                        Text("OpenAI-compatible local server").tag(Runtime.openAICompatible)
                    }
                    TextField("Local server", text: $controller.configuration.endpoint)
                    TextField("Model identifier", text: $controller.configuration.model)
                    Picker("Output format", selection: $controller.configuration.outputMode) {
                        Text("Native tools").tag(OutputMode.toolCalls)
                        Text("JSON schema").tag(OutputMode.jsonSchema)
                        Text("JSON object").tag(OutputMode.jsonObject)
                        Text("Prompted JSON").tag(OutputMode.promptJSON)
                    }
                    if controller.configuration.runtime == .ollama { Toggle("Disable thinking if the selected model supports it", isOn: $controller.configuration.disableThinking) }
                    HStack {
                        Button("Test connection") { controller.testConnection() }
                        Button(controller.downloadingModel ? "Downloading…" : "Download in Ollama") { controller.downloadCommandModel() }.disabled(controller.downloadingModel || controller.configuration.runtime != .ollama)
                    }
                    if ManagedRuntime.binary != nil {
                        HStack {
                            Button(controller.serverBusy ? "Starting…" : "Start bundled server") { controller.startBundledServer() }.disabled(controller.serverBusy || controller.serverRunning)
                            Button("Choose GGUF…") { controller.startBundledServer(chooseFile: true) }.disabled(controller.serverBusy || controller.serverRunning)
                            Button("Stop server") { controller.stopBundledServer() }.disabled(!controller.serverRunning)
                        }
                        Text("Ollama 0.13.5 cannot load the Liquid models. The bundled llama.cpp runtime can reuse their downloaded GGUF weights without changing your Ollama installation.").font(.caption).foregroundStyle(.secondary)
                    }
                    Text("Use http://127.0.0.1:11434 for Ollama. Other runtimes usually need an address ending in /v1. Only local endpoints are allowed; there is no automatic cloud or larger-model fallback.").font(.caption).foregroundStyle(.secondary)
                    if !controller.diagnostic.isEmpty { Text(controller.diagnostic).font(.caption).textSelection(.enabled) }
                }
                Section("Try an instruction") {
                    TextField("For example: open Calculator", text: $controller.testInstruction).onSubmit { controller.runTypedInstruction() }
                    HStack { Button("Run instruction") { controller.runTypedInstruction() }.disabled(controller.working); Text("Acts on the last app you used outside FnPilot.").font(.caption).foregroundStyle(.secondary) }
                }
                Section("Preferences") {
                    Toggle("Launch at login", isOn: Binding(get: { controller.launchAtLogin }, set: { controller.setLogin($0) }))
                    Toggle("Pause voice control", isOn: Binding(get: { controller.paused }, set: { _ in controller.togglePause() }))
                    Text("Escape cancels. Fn with another key keeps its normal shortcut. Instructions can operate accessible controls; unsupported or ambiguous targets stop with an explanation.").font(.caption).foregroundStyle(.secondary)
                }
            }.formStyle(.grouped)
            Divider()
            HStack {
                Circle().fill(controller.paused ? .orange : .teal).frame(width: 7, height: 7)
                Text(controller.status).font(.caption).lineLimit(2)
                Spacer()
            }.padding(14)
        }.frame(minWidth: 600, minHeight: 610)
    }
    private func permission(_ title: String, granted: Bool, action: @escaping () -> Void) -> some View {
        HStack {
            Label(title, systemImage: granted ? "checkmark.circle.fill" : "circle").foregroundStyle(granted ? Color.primary : Color.secondary)
            Spacer()
            Button(granted ? "Check" : "Enable", action: action)
        }
    }
}
