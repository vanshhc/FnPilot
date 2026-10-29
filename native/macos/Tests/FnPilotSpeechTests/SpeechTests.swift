import XCTest
import AVFoundation
@testable import FnPilotSpeech
@_cdecl("fnpilot_speech_is_current")
func testCurrentEpoch(_ epoch: UInt64) -> Bool { epoch != UInt64.max }
private let replyLock = NSLock()
private var replies: [UInt64: String] = [:]
private func receiveSpeechReply(_ id: UInt64, _ pointer: UnsafePointer<CChar>?) {
    guard let pointer else { return }
    let result = String(cString: pointer)
    replyLock.lock(); defer { replyLock.unlock() }
    replies[id] = result
}
private func takeSpeechReply(_ id: UInt64) -> String? {
    replyLock.lock(); defer { replyLock.unlock() }
    return replies.removeValue(forKey: id)
}
final class SpeechTests: XCTestCase {
    func testBindingRejectsFeatureOperationsAndInvalidAudioWithoutLoadingModels() async throws {
        for (index, operation) in ["execute", "observe", "startCapture", "overlay", "autostart", "transcribeWhisperKit"].enumerated() {
            let id = UInt64(index + 100)
            let input = "{\"operation\":\"\(operation)\",\"epoch\":1,\"action\":{\"kind\":\"mute\"}}"
            input.withCString { speechRequest($0, id, receiveSpeechReply) }
            var reply: String?
            for _ in 0..<200 {
                reply = takeSpeechReply(id)
                if reply != nil { break }
                try await Task.sleep(nanoseconds: 10_000_000)
            }
            let data = try XCTUnwrap(reply).data(using: .utf8)!
            let value = try JSONSerialization.jsonObject(with: data) as! [String: String]
            XCTAssertEqual(value["error"], operation == "transcribeWhisperKit" ? "Invalid speech input." : "Unknown speech operation.")
            XCTAssertNil(value["ok"])
        }
    }

    func testSelectedOfflineSpeechWhenExplicitlyProvisioned() async throws {
        let env=ProcessInfo.processInfo.environment
        guard let folder=env["FNPILOT_TEST_SPEECH_FOLDER"],let tokenizer=env["FNPILOT_TEST_TOKENIZER"],let audio=env["FNPILOT_TEST_AUDIO"] else {throw XCTSkip("Explicit offline speech smoke assets not provisioned")}
        let file=try AVAudioFile(forReading:URL(fileURLWithPath:audio),commonFormat:.pcmFormatFloat32,interleaved:false)
        XCTAssertEqual(file.processingFormat.sampleRate,16000)
        XCTAssertEqual(file.processingFormat.channelCount,1)
        let buffer=AVAudioPCMBuffer(pcmFormat:file.processingFormat,frameCapacity:AVAudioFrameCount(file.length))!
        try file.read(into:buffer)
        let samples=Array(UnsafeBufferPointer(start:buffer.floatChannelData![0],count:Int(buffer.frameLength)))
        let text=try await WhisperSpeech.shared.transcribe(samples,folder:folder,tokenizerBase:tokenizer,epoch:1)
        XCTAssertTrue(text.lowercased().contains("calculator"))
        await WhisperSpeech.shared.unload()
    }
    func testWhisperKitShortAudioBoundary() {
        XCTAssertEqual(WhisperSpeech.prepare([Float](repeating: 0.1, count: 1600)).count,16001)
        XCTAssertEqual(WhisperSpeech.prepare([Float](repeating: 0.1, count: 16000)).count,16001)
        XCTAssertEqual(WhisperSpeech.prepare([Float](repeating: 0.1, count: 17000)).count,17000)
    }
}
