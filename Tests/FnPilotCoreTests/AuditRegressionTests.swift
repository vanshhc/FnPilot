import XCTest
@testable import FnPilotCore

final class AuditRegressionTests: XCTestCase {
    func testCompoundsPreserveClarificationOnEitherSide() {
        for input in ["open the app and increase volume", "volume up and close the app"] {
            let result = DirectParser().parse(input)
            XCTAssertEqual(result?.status, "clarify")
            XCTAssertEqual(result?.actions, [])
        }
    }

    func testEveryDictationVerbKeepsPunctuationAndCommandWords() {
        for verb in ["type", "write", "enter text"] {
            let input = "open Notes and \(verb) Buy milk! Then open Chrome!"
            let expected = Action("type_text", text: "Buy milk! Then open Chrome!")
            XCTAssertEqual(DirectParser().parse(input)?.actions.last, expected)
            let parts = CommandSequence.parts(input)
            XCTAssertEqual(parts.count, 2)
            XCTAssertEqual(DirectParser().parse(parts[1])?.actions, [expected])
        }
    }

    func testIncompleteScanCannotResolveNamedControlOrReply() {
        let context = CommandContext(controls: [UIControl(id: "save", role: "AXButton", label: "Save")], controlsComplete: false)
        XCTAssertEqual(DirectUIParser().parse("click Save", context: context)?.status, "unsupported")
        XCTAssertEqual(DirectUIParser().answer(original: "click it", reply: "Save", context: context)?.status, "unsupported")
    }

    @MainActor
    func testSequenceDiscoversControlsAfterOpeningTarget() async throws {
        let runner = CommandRunner()
        var context = CommandContext(appName: "Browser")
        var executed: [Action] = []
        for part in CommandSequence.parts("open Notes then click New Note then type Hello!") {
            let result = try await runner.interpret(part, context: context) { _, _ in XCTFail("Unexpected fallback"); return Interpretation() }
            try await runner.execute(result, context: { context }) { action in
                executed.append(action)
                if action.kind == "open_app" { context = CommandContext(appName: "Notes", controls: [UIControl(id: "new", role: "AXButton", label: "New Note")]) }
            }
        }
        XCTAssertEqual(executed, [Action("open_app", app: "Notes"), Action("press_control", controlID: "new"), Action("type_text", text: "Hello!")])
    }

    @MainActor
    func testRunnerStopsOnClarificationAndRejectsStaleOrIncompleteControls() async throws {
        let runner = CommandRunner()
        var calls = 0
        let result = try await runner.interpret("open the app and increase volume", context: CommandContext()) { _, _ in XCTFail(); return Interpretation() }
        try await runner.execute(result, context: { CommandContext() }) { _ in calls += 1 }
        XCTAssertEqual(calls, 0)
        for context in [CommandContext(), CommandContext(controls: [UIControl(id: "save", role: "AXButton", label: "Save")], controlsComplete: false)] {
            do {
                try await runner.execute(Interpretation(actions: [Action("press_control", controlID: "save")]), context: { context }) { _ in calls += 1 }
                XCTFail("Unsafe control accepted")
            } catch {}
        }
        XCTAssertEqual(calls, 0)
    }

    @MainActor
    func testRunnerStopsAfterFailureAndEnforcesSequenceBudget() async throws {
        let runner = CommandRunner()
        var calls = 0
        do {
            try await runner.execute(Interpretation(actions: [Action("mute"), Action("unmute")]), context: { CommandContext() }) { _ in calls += 1; throw PilotError("Rejected") }
            XCTFail()
        } catch {}
        XCTAssertEqual(calls, 1)
        let budget = CommandRunner()
        for _ in 0..<20 { try await budget.execute(Interpretation(actions: [Action("mute")]), context: { CommandContext() }) { _ in } }
        do { try await budget.execute(Interpretation(actions: [Action("mute")]), context: { CommandContext() }) { _ in XCTFail() }; XCTFail() } catch {}
    }
}

extension AuditRegressionTests {
    @MainActor
    func testRunnerStopsWhenCancelledBetweenActions() async throws {
        var calls = 0
        var resume: CheckedContinuation<Void, Never>?
        let task = Task { @MainActor in
            try await CommandRunner().execute(Interpretation(actions: [Action("mute"), Action("unmute")]), context: { CommandContext() }) { _ in
                calls += 1
                await withCheckedContinuation { resume = $0 }
            }
        }
        while resume == nil { await Task.yield() }
        task.cancel(); resume?.resume()
        do { try await task.value; XCTFail("Cancellation ignored") } catch { XCTAssertTrue(error is CancellationError) }
        XCTAssertEqual(calls, 1)
    }

    @MainActor
    func testRunnerRejectsControlWhoseLabelChangedDuringInterpretation() async {
        let before = CommandContext(controls: [UIControl(id: "button", role: "AXButton", label: "Save")])
        let after = CommandContext(controls: [UIControl(id: "button", role: "AXButton", label: "Delete")])
        do {
            try await CommandRunner().execute(Interpretation(actions: [Action("press_control", controlID: "button")]), observed: before, context: { after }) { _ in XCTFail("Changed control pressed") }
            XCTFail("Changed control accepted")
        } catch {}
    }
}
