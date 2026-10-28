import XCTest
import AppKit
import ApplicationServices
import FnPilotCore
@testable import FnPilotApp

final class DesktopAuditTests: XCTestCase {
    @MainActor
    func testFocusedWindowWinsAfterMinimizingAnotherWindow() throws {
        let executor = MacExecutor()
        let app = NSRunningApplication.current
        let a = AXUIElementCreateApplication(11111), b = AXUIElementCreateApplication(22222)
        var focused = a
        var minimized = false
        executor.readAttribute = { node, key in
            if key == kAXWindowsAttribute { return [a, b] as CFArray }
            if key == kAXFocusedWindowAttribute { return focused }
            if key == kAXMinimizedAttribute { return (CFEqual(node, a) && minimized) ? kCFBooleanTrue : kCFBooleanFalse }
            return nil
        }
        _ = executor.window(for: app, restoring: true) // Remember A as the restore target.
        minimized = true; focused = b
        XCTAssertTrue(CFEqual(try XCTUnwrap(executor.window(for: app)), b))
        XCTAssertTrue(CFEqual(try XCTUnwrap(executor.window(for: app, restoring: true)), a))
    }

    @MainActor
    func testScanReportsEachLimitAndDoesNotTreatCyclesAsTruncation() {
        for limit in ["depth", "nodes", "controls", "cycle"] {
            let executor = MacExecutor()
            let root = AXUIElementCreateApplication(11111)
            executor.readAttribute = { _, key in key == kAXChildrenAttribute ? [root] as CFArray : nil }
            var visited = Set<CFHashCode>()
            var controls: [FnPilotCore.UIControl] = []
            if limit == "nodes" { visited = Set((1...700).map { CFHashCode($0) }) }
            if limit == "controls" { controls = (0..<90).map { FnPilotCore.UIControl(id: "\($0)", role: "AXButton", label: "Save") } }
            var complete = true
            executor.scan(root, depth: limit == "depth" ? 14 : 0, visited: &visited, controls: &controls, complete: &complete)
            XCTAssertEqual(complete, limit == "cycle", limit)
        }
    }

    @MainActor
    func testDuplicateBeyondScanLimitCannotBecomeUniqueSaveButton() {
        let executor = MacExecutor()
        let root = AXUIElementCreateApplication(10000)
        let buttons = (0..<91).map { AXUIElementCreateApplication(pid_t(20000 + $0)) }
        executor.readAttribute = { node, key in
            if CFEqual(node, root) { return key == kAXChildrenAttribute ? buttons as CFArray : nil }
            if key == kAXRoleAttribute { return "AXButton" as CFString }
            if key == kAXTitleAttribute { return (CFEqual(node, buttons[0]) || CFEqual(node, buttons[90]) ? "Save" : "Other") as CFString }
            return nil
        }
        var visited = Set<CFHashCode>(), controls: [UIControl] = []
        var complete = true
        executor.scan(root, depth: 0, visited: &visited, controls: &controls, complete: &complete)
        XCTAssertEqual(controls.filter { $0.label == "Save" }.count, 1)
        XCTAssertFalse(complete)
        let result = DirectUIParser().parse("click Save", context: CommandContext(controls: controls, controlsComplete: complete))
        XCTAssertEqual(result?.status, "unsupported")
        XCTAssertEqual(result?.actions, [])
    }

    @MainActor
    func testStopDuringMetadataOrPortWaitPreventsLaunch() async throws {
        for stage in ["metadata", "port"] {
            let runtime = ManagedRuntime()
            let file = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            try Data("GGUF".utf8).write(to: file)
            defer { try? FileManager.default.removeItem(at: file) }
            var waiting: CheckedContinuation<Void, Never>?
            let task = Task { @MainActor in
                try await runtime.start(model: UUID().uuidString, file: stage == "metadata" ? nil : file,
                                        binary: URL(fileURLWithPath: "/does-not-exist"),
                                        resolveFile: { _ in await withCheckedContinuation { waiting = $0 }; return file },
                                        portAvailable: { await withCheckedContinuation { waiting = $0 }; return true })
            }
            while waiting == nil { await Task.yield() }
            runtime.stop(); waiting?.resume()
            do { try await task.value; XCTFail("Stopped startup completed") }
            catch { XCTAssertTrue(error is CancellationError, "\(error)") }
            XCTAssertFalse(runtime.running); XCTAssertFalse(runtime.starting)
        }
    }

    @MainActor
    func testUnexpectedProcessExitClearsStateAndReportsFailure() async throws {
        let runtime = ManagedRuntime()
        let exited = expectation(description: "Exit observed")
        runtime.onStateChange = { message in
            if let message {
                XCTAssertFalse(runtime.running)
                XCTAssertTrue(message.contains("status 7"))
                exited.fulfill()
            }
        }
        let input = Pipe()
        let process = Process(); process.executableURL = URL(fileURLWithPath: "/bin/sh"); process.arguments = ["-c", "read line; exit 7"]
        process.standardInput = input
        try runtime.launch(process)
        runtime.markReady()
        XCTAssertTrue(runtime.running)
        try input.fileHandleForWriting.write(contentsOf: Data("exit\n".utf8))
        await fulfillment(of: [exited], timeout: 3)
        XCTAssertFalse(runtime.running)
    }
}
