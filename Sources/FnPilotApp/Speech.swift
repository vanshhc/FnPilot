import AVFoundation
import WhisperKit
import FnPilotCore

protocol SpeechRecognizer {
    func prepare(download: Bool) async throws
    func transcribe(_ samples: [Float]) async throws -> String
}

actor WhisperSpeechRecognizer: SpeechRecognizer {
    private var pipe: WhisperKit?
    static var directory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("FnPilot/Models", isDirectory: true)
    }
    func prepare(download: Bool) async throws {
        if pipe != nil { return }
        let base = Self.directory
        try FileManager.default.createDirectory(at: base, withIntermediateDirectories: true)
        let saved = UserDefaults.standard.string(forKey: "speechModelFolder")
        let folder: URL
        if let saved, FileManager.default.fileExists(atPath: saved) { folder = URL(fileURLWithPath: saved) }
        else if download {
            folder = try await WhisperKit.download(variant: "openai_whisper-small.en", downloadBase: base)
            UserDefaults.standard.set(folder.path, forKey: "speechModelFolder")
        } else { throw PilotError("Download the English speech model in FnPilot Settings first.") }
        let tokenizer = base.appendingPathComponent("models/openai/whisper-small.en")
        if !download {
            for file in ["tokenizer.json", "tokenizer_config.json", "config.json"] {
                guard FileManager.default.fileExists(atPath: tokenizer.appendingPathComponent(file).path) else { throw PilotError("The local tokenizer is incomplete. Use Download / repair speech model in Settings.") }
            }
        }
        let config = WhisperKitConfig(modelFolder: folder.path, tokenizerFolder: base, verbose: false, prewarm: false, load: true, download: false)
        pipe = try await WhisperKit(config)
    }
    func transcribe(_ samples: [Float]) async throws -> String {
        guard let pipe else { throw PilotError("Speech model is not loaded yet.") }
        guard samples.count >= 1600 else { throw PilotError("Hold Fn while speaking, then release it.") }
        let rms = sqrt(samples.reduce(Float(0)) { $0 + $1 * $1 } / Float(samples.count))
        guard rms > 0.002 else { throw PilotError("No speech was detected.") }
        let options = DecodingOptions(language: "en", temperature: 0, temperatureFallbackCount: 0, skipSpecialTokens: true, withoutTimestamps: true, suppressBlank: true, concurrentWorkerCount: 1)
        let results: [TranscriptionResult] = try await pipe.transcribe(audioArray: samples, decodeOptions: options)
        let text = results.map(\.text).joined(separator: " ").trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { throw PilotError("No speech was recognized.") }
        return text
    }
    func transcribeFile(_ path: String) async throws -> String {
        let samples = try AudioProcessor.loadAudioAsFloatArray(fromPath: path)
        return try await transcribe(samples)
    }
}

final class MicrophoneRecorder {
    private let engine = AVAudioEngine()
    private let lock = NSLock()
    private var samples: [Float] = []
    private var conversionFailed = false
    private var overflow = false
    private var active = false
    private var recordingToken: UUID?
    private var converter: AVAudioConverter?
    private let maxSamples = 16_000 * 60
    func start() throws {
        guard !active else { return }
        guard AVCaptureDevice.authorizationStatus(for: .audio) == .authorized else { throw PilotError("Grant microphone access in FnPilot Settings first.") }
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0,
              let output = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16000, channels: 1, interleaved: false),
              let converter = AVAudioConverter(from: format, to: output) else { throw PilotError("No usable microphone input format.") }
        self.converter = converter
        let token = UUID()
        lock.lock(); samples = []; conversionFailed = false; overflow = false; recordingToken = token; lock.unlock()
        input.installTap(onBus: 0, bufferSize: 2048, format: format) { [weak self] buffer, _ in
            guard let self else { return }
            let capacity = AVAudioFrameCount(Double(buffer.frameLength) * 16000 / format.sampleRate) + 32
            guard let converted = AVAudioPCMBuffer(pcmFormat: output, frameCapacity: capacity) else { return }
            var supplied = false, error: NSError?
            let status = converter.convert(to: converted, error: &error) { _, inputStatus in
                if supplied { inputStatus.pointee = .noDataNow; return nil }
                supplied = true; inputStatus.pointee = .haveData; return buffer
            }
            self.lock.lock(); defer { self.lock.unlock() }
            guard self.recordingToken == token else { return }
            if status == .error || error != nil { self.conversionFailed = true; return }
            guard let data = converted.floatChannelData?[0] else { return }
            if self.samples.count + Int(converted.frameLength) > self.maxSamples { self.overflow = true; return }
            self.samples.append(contentsOf: UnsafeBufferPointer(start: data, count: Int(converted.frameLength)))
        }
        active = true
        engine.prepare()
        do { try engine.start() } catch { cancel(); throw error }
    }
    func stop() throws -> [Float] {
        lock.lock(); recordingToken = nil; lock.unlock()
        if active { engine.stop(); engine.inputNode.removeTap(onBus: 0); active = false }
        converter = nil
        lock.lock(); defer { samples.removeAll(keepingCapacity: false); lock.unlock() }
        if conversionFailed { throw PilotError("Microphone conversion failed. Check the selected input device.") }
        if overflow { throw PilotError("Recording exceeded 60 seconds. Give a shorter instruction.") }
        return samples
    }
    func cancel() { _ = try? stop() }
}
