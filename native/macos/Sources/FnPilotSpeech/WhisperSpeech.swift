import Foundation
import WhisperKit

/// Only native speech lives here. Rust owns validation, routing and session authority.
public actor WhisperSpeech {
    static let shared = WhisperSpeech()
    private var recognizer: WhisperKit?
    private var selection: String?
    private var decoding = false

    func unload() { recognizer = nil; selection = nil }

    func transcribe(_ samples: [Float], folder: String, tokenizerBase: String, epoch: UInt64) async throws -> String {
        guard !decoding else {throw SpeechError("The speech backend is still stopping its previous operation. Retry shortly.")}
        decoding = true
        defer {decoding = false}
        guard isCurrent(epoch) else { throw SpeechError("Speech was cancelled.") }
        let key = folder + "\n" + tokenizerBase
        if selection != key {
            unload()
            let base = URL(fileURLWithPath: tokenizerBase, isDirectory: true)
            let tokenizer = base.appendingPathComponent("models/openai/whisper-small.en")
            for name in ["tokenizer.json", "tokenizer_config.json", "config.json"] {
                guard FileManager.default.fileExists(atPath: tokenizer.appendingPathComponent(name).path) else {
                    throw SpeechError("Select the existing WhisperKit tokenizer cache containing models/openai/whisper-small.en. No download is performed.")
                }
            }
            let loaded = try await WhisperKit(WhisperKitConfig(modelFolder: folder, tokenizerFolder: base, verbose: false, prewarm: false, load: true, download: false))
            guard loaded.modelVariant == .smallEn else { throw SpeechError("The retained Mac backend requires the original small.en model. Select portable speech for other models.") }
            guard isCurrent(epoch) else { throw SpeechError("Speech loading was cancelled.") }
            recognizer = loaded
            selection = key
        }
        guard let recognizer else { throw SpeechError("Speech model is not loaded.") }
        let audio = Self.prepare(samples)
        let options = DecodingOptions(language: "en", temperature: 0, temperatureFallbackCount: 0, skipSpecialTokens: true, withoutTimestamps: true, suppressBlank: true, concurrentWorkerCount: 1)
        let results: [TranscriptionResult] = try await recognizer.transcribe(audioArray: audio, decodeOptions: options, callback: { _ in isCurrent(epoch) ? nil : false })
        guard isCurrent(epoch) else { throw SpeechError("Speech was cancelled.") }
        let text = results.map(\.text).joined(separator: " ").trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { throw SpeechError("No speech was recognized.") }
        return text
    }

    // WhisperKit 0.9.4 skips its decode loop at or below one second. Validation is in Rust.
    public static func prepare(_ samples: [Float]) -> [Float] {
        samples.count <= 16_000 ? samples + [Float](repeating: 0, count: 16_001 - samples.count) : samples
    }
}
