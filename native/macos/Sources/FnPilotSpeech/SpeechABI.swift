import Foundation

@_silgen_name("fnpilot_speech_is_current")
func isCurrent(_ epoch: UInt64) -> Bool

struct SpeechError: LocalizedError {
    let message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}

// Speech-only: actions, targets, preferences and native integration belong to Rust.
private struct SpeechRequest: Decodable {
    let operation: String
    let epoch: UInt64
    let folder: String?
    let tokenizerBase: String?
    let samples: [Float]?
}

@_cdecl("fnpilot_speech_request")
public func speechRequest(_ json: UnsafePointer<CChar>, _ id: UInt64, _ callback: @escaping @convention(c) (UInt64, UnsafePointer<CChar>?) -> Void) {
    let data = Data(String(cString: json).utf8)
    Task {
        let response: [String: Any]
        do {
            let request = try JSONDecoder().decode(SpeechRequest.self, from: data)
            guard isCurrent(request.epoch) else { throw SpeechError("Speech request was cancelled.") }
            switch request.operation {
            case "transcribeWhisperKit":
                guard let samples = request.samples, let folder = request.folder, let tokenizer = request.tokenizerBase,
                      (1600...960000).contains(samples.count), samples.allSatisfy({ $0.isFinite && abs($0) <= 1 }) else { throw SpeechError("Invalid speech input.") }
                let text = try await WhisperSpeech.shared.transcribe(samples, folder: folder, tokenizerBase: tokenizer, epoch: request.epoch)
                response = ["ok": text]
            case "unloadSpeech":
                await WhisperSpeech.shared.unload()
                response = ["ok": true]
            default: throw SpeechError("Unknown speech operation.")
            }
        } catch { response = ["error": error.localizedDescription] }
        let encoded = (try? JSONSerialization.data(withJSONObject: response)) ?? Data("{\"error\":\"Speech result encoding failed.\"}".utf8)
        String(decoding: encoded, as: UTF8.self).withCString { callback(id, $0) }
    }
}
