import XCTest
@testable import FnPilotCore

final class SpeechAudioTests: XCTestCase {
    func testBriefSpeechIsPaddedPastWhisperKitsOneSecondDecodeBoundary() throws {
        let speech = [Float](repeating: 0.02, count: 8000)
        let prepared = try SpeechAudio.prepareForWhisper(speech)
        XCTAssertGreaterThan(prepared.count, 16000)
        XCTAssertEqual(Array(prepared.prefix(speech.count)), speech)
        XCTAssertTrue(prepared.dropFirst(speech.count).allSatisfy { $0 == 0 })
    }

    func testExactlyOneSecondIsAlsoPadded() throws {
        let speech = [Float](repeating: 0.02, count: 16000)
        XCTAssertGreaterThan(try SpeechAudio.prepareForWhisper(speech).count, 16000)
    }

    func testLongerSpeechIsUnchanged() throws {
        let speech = [Float](repeating: 0.02, count: 16001)
        XCTAssertEqual(try SpeechAudio.prepareForWhisper(speech), speech)
    }

    func testSilenceAndAccidentalTapsAreRejectedBeforePadding() {
        XCTAssertThrowsError(try SpeechAudio.prepareForWhisper([Float](repeating: 0, count: 8000)))
        XCTAssertThrowsError(try SpeechAudio.prepareForWhisper([Float](repeating: 0.02, count: 1599)))
    }
}
