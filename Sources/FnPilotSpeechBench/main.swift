import Foundation
import CryptoKit
import WhisperKit
import FnPilotCore

enum SpeechBenchmark {
    struct Clip: Decodable {
        let id: String
        let path: String
        let reference: String
        let category: String
        let sha256: String
    }

    struct Measurement: Encodable {
        let id: String
        let category: String
        let reference: String
        let transcript: String
        let repetition: Int
        let audioSeconds: Double
        let transcriptionSeconds: Double
        let error: String?
    }

    struct Report: Encodable {
        let model: String
        let manifestSHA256: String
        let decodingOptions: DecodingOptions
        let osVersion: String
        let physicalMemoryBytes: UInt64
        let thermalStateAtStart: Int
        let thermalStateAtEnd: Int
        let padsShortAudio: Bool
        let modelLoadSeconds: Double
        let firstDecodeSeconds: Double
        let warmupDecodes: Int
        let repetitions: Int
        let measurements: [Measurement]
    }

    static func run(arguments: [String]) async throws {
        func value(_ flag: String) throws -> String {
            guard let index = arguments.firstIndex(of: flag), index + 1 < arguments.count,
                  !arguments[index + 1].hasPrefix("--") else {
                throw PilotError("Missing value for \(flag).")
            }
            return arguments[index + 1]
        }
        let manifestURL = URL(fileURLWithPath: try value("--manifest"))
        let model = try value("--model")
        guard ["openai_whisper-small.en", "openai_whisper-large-v3_turbo"].contains(model) else {
            throw PilotError("Benchmark model must be openai_whisper-small.en or openai_whisper-large-v3_turbo.")
        }
        let outputURL = URL(fileURLWithPath: try value("--output"))
        let repetitions = Int(try value("--repeats")) ?? 0
        guard (1...10).contains(repetitions) else { throw PilotError("Benchmark repeats must be between 1 and 10.") }
        let manifestData = try Data(contentsOf: manifestURL)
        let manifestSHA256 = SHA256.hash(data: manifestData).map { String(format: "%02x", $0) }.joined()
        let clips = try JSONDecoder().decode([Clip].self, from: manifestData)
        guard !clips.isEmpty, Set(clips.map(\.id)).count == clips.count else {
            throw PilotError("Benchmark manifest needs clips with unique IDs.")
        }
        // File decoding and resampling are outside the transcription timing.
        let padsShortAudio = arguments.contains("--pad-short-audio")
        let rawAudio = try clips.map { clip in
            let file = manifestURL.deletingLastPathComponent().appendingPathComponent(clip.path)
            let digest = SHA256.hash(data: try Data(contentsOf: file)).map { String(format: "%02x", $0) }.joined()
            guard digest == clip.sha256 else { throw PilotError("Audio checksum does not match for \(clip.id).") }
            return try AudioProcessor.loadAudioAsFloatArray(fromPath: file.path)
        }
        let audio = try rawAudio.map { padsShortAudio ? try SpeechAudio.prepareForWhisper($0) : $0 }
        guard audio.allSatisfy({ !$0.isEmpty }) else { throw PilotError("Benchmark contains empty audio.") }
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("FnPilot/Models", isDirectory: true)
        let folder = base.appendingPathComponent("models/argmaxinc/whisperkit-coreml/\(model)")
        let tokenizer = model == "openai_whisper-small.en" ? "whisper-small.en" : "whisper-large-v3"
        for file in ["tokenizer.json", "tokenizer_config.json", "config.json"] {
            guard FileManager.default.fileExists(atPath: base.appendingPathComponent("models/openai/\(tokenizer)/\(file)").path) else {
                throw PilotError("Install the local \(tokenizer) tokenizer before benchmarking.")
            }
        }
        let loadStart = ProcessInfo.processInfo.systemUptime
        let pipe = try await WhisperKit(WhisperKitConfig(modelFolder: folder.path, tokenizerFolder: base, verbose: false, prewarm: false, load: true, download: false))
        let loadSeconds = ProcessInfo.processInfo.systemUptime - loadStart
        let options = DecodingOptions(language: "en", temperature: 0, temperatureFallbackCount: 0, skipSpecialTokens: true, withoutTimestamps: true, suppressBlank: true, concurrentWorkerCount: 1)
        let initialThermalState = ProcessInfo.processInfo.thermalState.rawValue
        let firstStart = ProcessInfo.processInfo.systemUptime
        _ = try await pipe.transcribe(audioArray: audio[0], decodeOptions: options)
        let firstSeconds = ProcessInfo.processInfo.systemUptime - firstStart
        // Two additional warm-ups avoid conflating Core ML specialization with steady state.
        for _ in 0..<2 { _ = try await pipe.transcribe(audioArray: audio[0], decodeOptions: options) }
        var measurements: [Measurement] = []
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        for repetition in 0..<repetitions {
            for offset in clips.indices {
                try Task.checkCancellation()
                let index = (offset + repetition * 17) % clips.count
                let clip = clips[index]
                let start = ProcessInfo.processInfo.systemUptime
                var text = ""
                var failure: String?
                do {
                    let results: [TranscriptionResult] = try await pipe.transcribe(audioArray: audio[index], decodeOptions: options)
                    text = results.map(\.text).joined(separator: " ").trimmingCharacters(in: .whitespacesAndNewlines)
                    if text.isEmpty { failure = "No speech was recognized." }
                } catch { failure = error.localizedDescription }
                let elapsed = ProcessInfo.processInfo.systemUptime - start
                measurements.append(Measurement(id: clip.id, category: clip.category, reference: clip.reference, transcript: text, repetition: repetition, audioSeconds: Double(rawAudio[index].count) / 16_000, transcriptionSeconds: elapsed, error: failure))
                let report = Report(model: model, manifestSHA256: manifestSHA256, decodingOptions: options,
                    osVersion: ProcessInfo.processInfo.operatingSystemVersionString,
                    physicalMemoryBytes: ProcessInfo.processInfo.physicalMemory,
                    thermalStateAtStart: initialThermalState,
                    thermalStateAtEnd: ProcessInfo.processInfo.thermalState.rawValue,
                    padsShortAudio: padsShortAudio,
                    modelLoadSeconds: loadSeconds, firstDecodeSeconds: firstSeconds, warmupDecodes: 3,
                    repetitions: repetitions, measurements: measurements)
                try encoder.encode(report).write(to: outputURL, options: .atomic)
                print("\(model) pass \(repetition + 1)/\(repetitions) clip \(offset + 1)/\(clips.count): \(String(format: "%.3f", elapsed))s")
                fflush(stdout)
            }
        }
    }
}

if CommandLine.arguments.contains("--help") {
    print("Usage: fnpilot-speech-bench --manifest MANIFEST --model VARIANT --output REPORT --repeats 3 [--pad-short-audio]")
    print("Requires local models; never executes desktop actions. Models: openai_whisper-small.en, openai_whisper-large-v3_turbo.")
} else {
    Task {
        do { try await SpeechBenchmark.run(arguments: CommandLine.arguments); exit(0) }
        catch { fputs("\(error.localizedDescription)\n", stderr); exit(1) }
    }
    RunLoop.main.run()
}
