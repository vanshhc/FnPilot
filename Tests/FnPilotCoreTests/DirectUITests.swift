import XCTest
@testable import FnPilotCore

final class DirectUITests: XCTestCase {
    let parser = DirectUIParser()
    let context = CommandContext(controls: [UIControl(id: "b", role: "AXButton", label: "Save"), UIControl(id: "f", role: "AXTextField", label: "Search")])
    func testNamedButtonNeedsNoModel() {
        XCTAssertEqual(parser.parse("Please click the Save button", context: context)?.actions, [Action("press_control", controlID: "b")])
    }
    func testMissingAndDuplicateControlsDoNotExecute() {
        XCTAssertEqual(parser.parse("Click Export", context: context)?.status, "unsupported")
        let duplicate = CommandContext(controls: [UIControl(id: "a", role: "AXButton", label: "Save"), UIControl(id: "b", role: "AXButton", label: "Save")])
        let result = parser.parse("Click Save", context: duplicate)
        XCTAssertEqual(result?.status, "clarify"); XCTAssertEqual(result?.actions, [])
    }
    func testFieldValuesRemainLiteral() {
        XCTAssertEqual(parser.parse("Change the Search field to Open Chrome then quit nothing!", context: context)?.actions, [Action("set_control_value", text: "Open Chrome then quit nothing!", controlID: "f")])
        XCTAssertEqual(parser.parse("Put cats in the Search field", context: context)?.actions, [Action("set_control_value", text: "cats", controlID: "f")])
    }
    func testAClarificationReplyResolvesWithoutAModel() {
        XCTAssertEqual(parser.answer(original: "click it", reply: "Save", context: context)?.actions, [Action("press_control", controlID: "b")])
        XCTAssertEqual(parser.answer(original: "Change the field to Keep my words!", reply: "Search", context: context)?.actions, [Action("set_control_value", text: "Keep my words!", controlID: "f")])
    }
    func testWindowPhrasesAreNotMistakenForFieldEntry() {
        XCTAssertNil(parser.parse("Put this window into full screen mode", context: context))
        XCTAssertEqual(parser.parse("Change the field to hello", context: context)?.status, "clarify")
    }
    func testPronounsRequireClarification() {
        XCTAssertEqual(parser.parse("Click it", context: context)?.status, "clarify")
    }
}
