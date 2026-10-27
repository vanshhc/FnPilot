import XCTest
@testable import FnPilotCore

final class ValidationTests: XCTestCase {
    func decode(_ text: String) throws -> Interpretation { try ActionRegistry.decode(Data(text.utf8)) }
    func testRegisteredActionDecodes() throws {
        XCTAssertEqual(try decode(#"{"status":"actions","actions":[{"kind":"type_text","text":"Hello!"}]}"#).actions, [Action("type_text", text: "Hello!")])
    }
    func testUnknownActionsAndExtraFieldsAreRejected() {
        for text in [
            #"{"status":"actions","actions":[{"kind":"run_shell","text":"anything"}]}"#,
            #"{"status":"actions","actions":[{"kind":"volume_up","shell":"anything"}]}"#,
            #"{"status":"actions","actions":[{"kind":"volume_up"}],"execute":true}"#,
            #"{"status":"actions","actions":[]}"#,
            #"```json {"status":"actions","actions":[{"kind":"volume_up"}]} ```"#
        ] { XCTAssertThrowsError(try decode(text), text) }
    }
    func testArgumentsAreStrictlyValidated() {
        for action in [Action("set_volume", value: 101), Action("set_brightness", value: -1), Action("type_text"), Action("volume_up", text: "unexpected"), Action("press_control"), Action("scroll", direction: "elsewhere"), Action("open_app"), Action("pause", value: 1), Action("mute", app: "Notes")] {
            XCTAssertThrowsError(try ActionRegistry.validate(Interpretation(actions: [action])), action.kind)
        }
        XCTAssertThrowsError(try decode(#"{"status":"actions","actions":[{"kind":"set_volume","value":true}]}"#))
    }
    func testClarificationCannotExecuteActions() {
        XCTAssertThrowsError(try ActionRegistry.validate(Interpretation(status: "clarify", actions: [Action("volume_up")], message: "Which?")))
        XCTAssertThrowsError(try ActionRegistry.validate(Interpretation(status: "unsupported", message: "")))
        XCTAssertNoThrow(try ActionRegistry.validate(Interpretation(status: "clarify", message: "Which Save button?")))
    }
    func testLimitsAndBoundaries() {
        XCTAssertNoThrow(try ActionRegistry.validate(Interpretation(actions: [Action("set_volume", value: 0), Action("set_brightness", value: 100)])))
        XCTAssertThrowsError(try ActionRegistry.validate(Interpretation(actions: Array(repeating: Action("volume_up"), count: 21))))
    }
    func testLocalEndpointsOnly() throws {
        for address in ["https://api.example.com/v1", "http://192.168.1.2:8080/v1", "http://127.0.0.1.evil.example/v1", "http://user:pass@localhost:80", "http://localhost:80/?remote=yes"] {
            XCTAssertThrowsError(try ModelConfiguration(endpoint: address).validatedURL(path: "api/chat"), address)
        }
        XCTAssertEqual(try ModelConfiguration(endpoint: "http://127.0.0.1:11434").validatedURL(path: "api/chat").path, "/api/chat")
        XCTAssertThrowsError(try ModelConfiguration(model: "some-model:cloud").validatedURL(path: "api/chat"))
    }
}

final class ContextTests: XCTestCase {
    func testCompactionKeepsRelevantControlsAndAmbiguity() {
        var controls = (0..<80).map { UIControl(id: "c\($0)", role: "AXButton", label: "Unrelated item \($0)") }
        controls += [UIControl(id: "save1", role: "AXButton", label: "Save"), UIControl(id: "save2", role: "AXButton", label: "Save")]
        let context = CommandContext(controls: controls).compacted(for: "Please click Save")
        XCTAssertEqual(context.controls.count, 24)
        XCTAssertEqual(Array(context.controls.prefix(2)).map(\.id), ["save1", "save2"])
    }
}
