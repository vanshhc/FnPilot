import XCTest
@testable import FnPilotCore

final class HotkeyTests: XCTestCase {
    func testHoldAndReleaseStartsAndFinishesOnce() {
        var key = HotkeyState()
        XCTAssertEqual(key.flags(fn: true, otherModifier: false), .start)
        XCTAssertNil(key.flags(fn: true, otherModifier: false))
        XCTAssertEqual(key.flags(fn: false, otherModifier: false), .finish)
        XCTAssertNil(key.flags(fn: false, otherModifier: false))
    }
    func testFnChordCancelsWithoutExecutingOnRelease() {
        var key = HotkeyState()
        XCTAssertEqual(key.flags(fn: true, otherModifier: false), .start)
        XCTAssertEqual(key.keyDown(escape: false), .cancel)
        XCTAssertNil(key.flags(fn: false, otherModifier: false))
    }
    func testOtherModifiersPreserveNormalShortcut() {
        var key = HotkeyState()
        XCTAssertNil(key.flags(fn: true, otherModifier: true))
        XCTAssertNil(key.flags(fn: false, otherModifier: true))
        XCTAssertEqual(key.flags(fn: true, otherModifier: false), .start)
        XCTAssertEqual(key.flags(fn: true, otherModifier: true), .cancel)
        XCTAssertNil(key.flags(fn: false, otherModifier: false))
    }
    func testEscapeAndSleepResetCannotFinishOldRecording() {
        var key = HotkeyState()
        _ = key.flags(fn: true, otherModifier: false)
        XCTAssertEqual(key.keyDown(escape: true), .cancel)
        XCTAssertNil(key.flags(fn: false, otherModifier: false))
        _ = key.flags(fn: true, otherModifier: false)
        key.reset()
        XCTAssertNil(key.flags(fn: false, otherModifier: false))
        XCTAssertEqual(key.flags(fn: true, otherModifier: false), .start)
    }
    func testSequenceKeepsDictationLiteral() {
        XCTAssertEqual(CommandSequence.parts("open Notes and type Milk and open Chrome then pause"), ["open Notes", "type Milk and open Chrome then pause"])
        XCTAssertEqual(CommandSequence.parts("type Open Notes then click Save"), ["type Open Notes then click Save"])
        XCTAssertEqual(CommandSequence.parts("open Notes then click New Note then type Hello"), ["open Notes", "click New Note", "type Hello"])
    }
}
