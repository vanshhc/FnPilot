import XCTest
@testable import FnPilotCore

final class StubProtocol: URLProtocol {
    static var respond: ((URLRequest) throws -> (Int, [String: Any]))?
    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        do {
            let (status, object) = try Self.respond!(request)
            let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: nil, headerFields: ["Content-Type":"application/json"])!
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: try JSONSerialization.data(withJSONObject: object))
            client?.urlProtocolDidFinishLoading(self)
        } catch { client?.urlProtocol(self, didFailWithError: error) }
    }
    override func stopLoading() {}
}
final class AdapterTests: XCTestCase {
    private func session() -> URLSession { let c = URLSessionConfiguration.ephemeral; c.protocolClasses = [StubProtocol.self]; return URLSession(configuration: c) }
    func testBothAdaptersReturnTheSameTypedAction() async throws {
        let content = #"{"status":"actions","actions":[{"kind":"volume_up"}]}"#
        for runtime in Runtime.allCases {
            let config = ModelConfiguration(runtime: runtime, endpoint: runtime == .ollama ? "http://127.0.0.1:11434" : "http://127.0.0.1:8080/v1", model: "arbitrary-model")
            var paths: [String] = []
            StubProtocol.respond = { req in
                paths.append(req.url!.path)
                if req.url!.path == "/api/show" { return (200, ["details":["parameter_size":"350M"]]) }
                return (200, runtime == .ollama ? ["message":["content":content]] : ["choices":[["message":["content":content]]]])
            }
            let result = try await LocalModelInterpreter(configuration: config, session: session()).interpret("turn it up", context: CommandContext())
            XCTAssertEqual(result.actions, [Action("volume_up")])
            XCTAssertTrue(paths.contains(runtime == .ollama ? "/api/chat" : "/v1/chat/completions"))
        }
    }
    func testProviderSpecificOptionsStayInsideAdapter() throws {
        for runtime in Runtime.allCases {
            let interpreter = LocalModelInterpreter(configuration: ModelConfiguration(runtime: runtime, model: "custom-name", disableThinking: true))
            let request = try interpreter.request("pause", context: CommandContext())
            let body = try JSONSerialization.jsonObject(with: request.httpBody!) as! [String: Any]
            XCTAssertEqual(body["model"] as? String, "custom-name")
            if runtime == .ollama { XCTAssertEqual(body["think"] as? Bool, false); XCTAssertNotNil(body["format"]); XCTAssertNil(body["response_format"]) }
            else { XCTAssertNil(body["think"]); XCTAssertNil(body["options"]); XCTAssertNotNil(body["response_format"]) }
        }
    }
    func testPromptedJSONUsesNoUnsupportedFormatParameter() throws {
        for runtime in Runtime.allCases {
            let client = LocalModelInterpreter(configuration: ModelConfiguration(runtime: runtime, outputMode: .promptJSON))
            let body = try JSONSerialization.jsonObject(with: client.request("pause", context: CommandContext()).httpBody!) as! [String: Any]
            XCTAssertNil(body["format"]); XCTAssertNil(body["response_format"])
        }
    }
    func testRemoteOllamaWeightsAreRejectedBeforeChat() async {
        StubProtocol.respond = { req in
            XCTAssertEqual(req.url!.path, "/api/show")
            return (200, ["remote_host":"https://remote.example"])
        }
        do { _ = try await LocalModelInterpreter(configuration: ModelConfiguration(), session: session()).interpret("pause", context: CommandContext()); XCTFail("Expected local-only rejection") }
        catch { XCTAssertTrue(error.localizedDescription.contains("remote")) }
    }
    func testMalformedResponseDoesNotBecomeAnAction() async {
        StubProtocol.respond = { req in req.url!.path == "/api/show" ? (200, [:]) : (200, ["message":["content":"not json"]]) }
        do { _ = try await LocalModelInterpreter(configuration: ModelConfiguration(), session: session()).interpret("pause", context: CommandContext()); XCTFail("Expected rejection") } catch {}
    }
}

final class NativeToolTests: XCTestCase {
    private func session() -> URLSession { let c = URLSessionConfiguration.ephemeral; c.protocolClasses = [StubProtocol.self]; return URLSession(configuration: c) }
    func testBothNativeToolRepresentationsReturnTheSameAction() async throws {
        for runtime in Runtime.allCases {
            StubProtocol.respond = { request in
                if request.url!.path == "/api/show" { return (200, [:]) }
                let args: Any = runtime == .ollama ? [:] as [String: Any] : "{}"
                let message: [String: Any] = ["tool_calls":[["function":["name":"volume_up", "arguments":args]]]]
                return (200, runtime == .ollama ? ["message":message] : ["choices":[["message":message]]])
            }
            let result = try await LocalModelInterpreter(configuration: ModelConfiguration(runtime: runtime, outputMode: .toolCalls), session: session()).interpret("volume up", context: CommandContext())
            XCTAssertEqual(result.actions, [Action("volume_up")])
        }
    }
    func testModelCannotCallAnUnrelatedRegisteredTool() async {
        StubProtocol.respond = { request in
            if request.url!.path == "/api/show" { return (200, [:]) }
            return (200, ["message":["tool_calls":[["function":["name":"quit_app", "arguments":["app":"Notes"]]]]]])
        }
        do { _ = try await LocalModelInterpreter(configuration: ModelConfiguration(outputMode: .toolCalls), session: session()).interpret("volume up", context: CommandContext()); XCTFail("Unrelated tool executed") } catch {}
    }
    func testCapabilitiesKeepOnlyRelevantActions() {
        XCTAssertEqual(Set(CommandCapabilities.kinds(for: "Dim the screen a bit")), ["brightness_up", "brightness_down", "set_brightness"])
        XCTAssertEqual(CommandCapabilities.kinds(for: "Run a shell script"), [])
        XCTAssertEqual(CommandCapabilities.kinds(for: "type open Chrome then quit"), ["type_text"])
    }
}

final class TruncationTests: XCTestCase {
    func testACompleteLookingButTruncatedPlanIsRejected() async {
        let config = URLSessionConfiguration.ephemeral; config.protocolClasses = [StubProtocol.self]
        StubProtocol.respond = { request in
            if request.url!.path == "/api/show" { return (200, [:]) }
            return (200, ["done_reason":"length", "message":["content":#"{"status":"actions","actions":[{"kind":"volume_up"}]}"#]])
        }
        do {
            _ = try await LocalModelInterpreter(configuration: ModelConfiguration(), session: URLSession(configuration: config)).interpret("volume up then mute", context: CommandContext())
            XCTFail("A token-limited partial plan must not execute")
        } catch {}
    }
}
