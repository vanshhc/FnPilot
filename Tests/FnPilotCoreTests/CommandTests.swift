import XCTest
@testable import FnPilotCore

final class CommandTests: XCTestCase {
    let parser = DirectParser()
    func testLiteralDictationPreservesCaseAndCommandWords() {
        XCTAssertEqual(parser.parse("type Open Chrome then delete nothing!")?.actions, [Action("type_text", text: "Open Chrome then delete nothing!")])
    }
    func testPoliteDictationIsStillLiteral() {
        XCTAssertEqual(parser.parse("Please type Open Chrome then close")?.actions, [Action("type_text", text: "Open Chrome then close")])
        XCTAssertEqual(CommandSequence.parts("Please type Open Chrome then close"), ["Please type Open Chrome then close"])
    }
    func testVolumeStepAndPercentage() {
        XCTAssertEqual(parser.parse("volume up")?.actions, [Action("volume_up")])
        XCTAssertEqual(parser.parse("set volume to 37 percent")?.actions, [Action("set_volume", value: 37)])
    }
    func testCompoundAppAndDictation() {
        XCTAssertEqual(parser.parse("Open Notes and type Buy milk.")?.actions, [Action("open_app", app: "Notes"), Action("type_text", text: "Buy milk.")])
    }
    func testVideoAndWindowFullscreenAreDistinct() {
        XCTAssertEqual(parser.parse("full screen the video")?.actions.first?.kind, "fullscreen_video")
        XCTAssertEqual(parser.parse("full screen the window")?.actions.first?.kind, "fullscreen_window")
    }
    func testCloseIsNotQuit() {
        XCTAssertEqual(parser.parse("close Chrome")?.actions, [Action("close_window", app: "Chrome")])
        XCTAssertEqual(parser.parse("quit Chrome")?.actions, [Action("quit_app", app: "Chrome")])
    }
    func testAmbiguousAndUnsupportedGoToInterpreter() {
        XCTAssertEqual(parser.parse("open the app")?.status, "clarify")
        XCTAssertNil(parser.parse("do that thing"))
        XCTAssertNil(parser.parse("delete all files"))
        XCTAssertNil(parser.parse("open Notes and send a message"))
    }
}
