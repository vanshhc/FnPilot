import Foundation
import AppKit
import FnPilotCore

@MainActor
final class ManagedRuntime: ObservableObject {
    @Published private(set) var running = false
    @Published private(set) var starting = false
    private var process: Process?
    static let port = 18114
    static let endpoint = "http://127.0.0.1:18114/v1"
    static var binary: URL? {
        let url = Bundle.main.resourceURL?.appendingPathComponent("Runtime/llama-server")
        return url.flatMap { FileManager.default.isExecutableFile(atPath: $0.path) ? $0 : nil }
    }
    func selectFile() -> URL? {
        let panel = NSOpenPanel(); panel.title = "Choose a local GGUF model"; panel.canChooseDirectories = false; panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK else { return nil }
        return panel.url
    }
    func start(model: String, file selected: URL? = nil) async throws {
        guard !starting else { return }
        if running { throw PilotError("Stop the bundled server before changing its model.") }
        guard let binary = Self.binary else { throw PilotError("The bundled runtime is missing. Rebuild the app with the runtime resources, or use Ollama / another local server.") }
        starting = true; defer { starting = false }
        let file: URL
        if let selected { file = selected }
        else if let saved = UserDefaults.standard.string(forKey: "gguf:\(model)"), FileManager.default.fileExists(atPath: saved) { file = URL(fileURLWithPath: saved) }
        else { file = try await ollamaFile(model: model) }
        let handle = try FileHandle(forReadingFrom: file)
        let magic = try handle.read(upToCount: 4); try handle.close()
        guard magic == Data("GGUF".utf8) else { throw PilotError("The selected file is not a GGUF model.") }
        let health = URL(string: "http://127.0.0.1:\(Self.port)/health")!
        let session = URLSession(configuration: .ephemeral)
        defer { session.invalidateAndCancel() }
        if let (_, response) = try? await session.data(from: health), (response as? HTTPURLResponse)?.statusCode == 200 { throw PilotError("Port \(Self.port) is already serving another process. Choose its endpoint manually or stop it before starting FnPilot's server.") }
        let process = Process()
        process.executableURL = binary
        process.arguments = ["--model", file.path, "--alias", model, "--host", "127.0.0.1", "--port", String(Self.port), "--ctx-size", "4096", "--parallel", "1", "--n-gpu-layers", "99", "--jinja", "--no-webui", "--log-disable", "--threads", "2", "--threads-batch", "2"]
        process.qualityOfService = .utility
        process.standardOutput = FileHandle.nullDevice; process.standardError = FileHandle.nullDevice
        try process.run(); self.process = process
        do {
            for _ in 0..<120 {
                try Task.checkCancellation()
                guard process.isRunning else { throw PilotError("The bundled runtime could not load this model. Check GGUF compatibility or choose another runtime.") }
                var request = URLRequest(url: health); request.timeoutInterval = 1
                if let (_, response) = try? await session.data(for: request), (response as? HTTPURLResponse)?.statusCode == 200 {
                    running = true
                    UserDefaults.standard.set(file.path, forKey: "gguf:\(model)")
                    UserDefaults.standard.set(true, forKey: "useBundledRuntime")
                    return
                }
                try await Task.sleep(nanoseconds: 250_000_000)
            }
            throw PilotError("The local model did not finish loading. Try starting the server again.")
        } catch { stop(); throw error }
    }
    func stop() {
        if let process, process.isRunning { process.terminate() }
        process = nil; running = false
    }
    private func ollamaFile(model: String) async throws -> URL {
        var request = URLRequest(url: URL(string: "http://127.0.0.1:11434/api/show")!)
        request.httpMethod = "POST"; request.setValue("application/json", forHTTPHeaderField: "Content-Type"); request.timeoutInterval = 5
        request.httpBody = try JSONSerialization.data(withJSONObject: ["model": model])
        let (data, response) = try await URLSession.shared.data(for: request)
        guard (response as? HTTPURLResponse)?.statusCode == 200, let object = try JSONSerialization.jsonObject(with: data) as? [String: Any], object["remote_host"] == nil, object["remote_model"] == nil, let modelfile = object["modelfile"] as? String,
              let line = modelfile.split(separator: "\n").first(where: { $0.hasPrefix("FROM /") }) else { throw PilotError("Could not find downloaded weights in Ollama. Download the model first or choose its GGUF file.") }
        let path = String(line.dropFirst(5)).trimmingCharacters(in: CharacterSet(charactersIn: "\""))
        guard FileManager.default.fileExists(atPath: path) else { throw PilotError("The local model file is missing.") }
        return URL(fileURLWithPath: path)
    }
}
