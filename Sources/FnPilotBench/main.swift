import Foundation
import FnPilotCore

struct Example: Codable {
    let id: Int
    let category: String
    let input: String
    let expected: Interpretation
    let context: CommandContext
}
struct Measurement: Codable {
    let id: Int
    let category: String
    let input: String
    let expected: Interpretation
    let actual: Interpretation?
    let correct: Bool
    let hybridCorrect: Bool
    let direct: Bool
    let seconds: Double
    let error: String?
}
struct Report: Codable {
    let model: String
    let runtime: Runtime
    let outputMode: OutputMode
    let created: String
    let modelCorrect: Int
    let hybridCorrect: Int
    let count: Int
    let invalidResponses: Int
    let coldSeconds: Double
    let warmMedianSeconds: Double
    let warmP95Seconds: Double
    let observations: [Measurement]
}

func normalized(_ result: Interpretation, apps: [AppInfo], expected: Interpretation? = nil, activeID: String = "") -> Interpretation {
    var result = result
    result.message = nil
    result.actions = result.actions.map { action in
        var action = action
        if let name = action.app {
            action.app = apps.first { $0.name.caseInsensitiveCompare(name) == .orderedSame || $0.bundleID.caseInsensitiveCompare(name) == .orderedSame }?.bundleID ?? (["chrome":"com.google.Chrome", "notes":"com.apple.Notes", "safari":"com.apple.Safari", "finder":"com.apple.finder", "vs code":"com.microsoft.VSCode", "vscode":"com.microsoft.VSCode"][name.lowercased()] ?? name.lowercased())
        }
        return action
    }
    if let expected, result.actions.count == expected.actions.count {
        for index in result.actions.indices where expected.actions[index].app == nil && result.actions[index].app == activeID && !["open_app", "focus_app", "quit_app"].contains(result.actions[index].kind) {
            result.actions[index].app = nil
        }
    }
    return result
}

func run() async throws {
    let args = CommandLine.arguments
    guard args.count >= 4 else {
        print("Usage: fnpilot-bench MODEL FIXTURES.json REPORT.json [ollama|openAICompatible] [ENDPOINT] [LIMIT]")
        exit(2)
    }
    let runtime = args.count > 4 ? Runtime(rawValue: args[4]) ?? .ollama : .ollama
    let endpoint = args.count > 5 ? args[5] : (runtime == .ollama ? "http://127.0.0.1:11434" : "http://127.0.0.1:11434/v1")
    let mode = args.count > 7 ? OutputMode(rawValue: args[7]) ?? .jsonSchema : .jsonSchema
    let config = ModelConfiguration(runtime: runtime, endpoint: endpoint, model: args[1], outputMode: mode)
    let interpreter = LocalModelInterpreter(configuration: config)
    var examples = try JSONDecoder().decode([Example].self, from: Data(contentsOf: URL(fileURLWithPath: args[2])))
    if args.count > 6, let limit = Int(args[6]) { examples = Array(examples.prefix(limit)) }
    var results: [Measurement] = []
    for example in examples {
        let start = Date()
        var result: Interpretation?, error: String?
        do { result = try await interpreter.interpret(example.input, context: example.context) }
        catch let caught { error = caught.localizedDescription }
        let seconds = Date().timeIntervalSince(start)
        let direct = DirectParser().parse(example.input) ?? DirectUIParser().parse(example.input, context: example.context)
        let wanted = normalized(example.expected, apps: example.context.apps)
        let correct = result.map { normalized($0, apps: example.context.apps, expected: wanted, activeID: example.context.bundleID) == wanted } ?? false
        let hybridCorrect = (direct ?? result).map { normalized($0, apps: example.context.apps, expected: wanted, activeID: example.context.bundleID) == wanted } ?? false
        results.append(Measurement(id: example.id, category: example.category, input: example.input, expected: example.expected, actual: result, correct: correct, hybridCorrect: hybridCorrect, direct: direct != nil, seconds: seconds, error: error))
        print("\(example.id) \(correct ? "PASS" : "FAIL") hybrid=\(hybridCorrect) \(String(format: "%.3f", seconds))s \(example.input)\(error.map { " [\($0)]" } ?? "")")
        fflush(stdout)
        try save(results, config: config, path: args[3])
    }
}
func save(_ results: [Measurement], config: ModelConfiguration, path: String) throws {
    let times = results.dropFirst().map(\.seconds).sorted()
    let report = Report(model: config.model, runtime: config.runtime, outputMode: config.outputMode, created: ISO8601DateFormatter().string(from: Date()), modelCorrect: results.filter(\.correct).count, hybridCorrect: results.filter(\.hybridCorrect).count, count: results.count, invalidResponses: results.filter { $0.error != nil }.count, coldSeconds: results.first?.seconds ?? 0, warmMedianSeconds: times.isEmpty ? 0 : times[times.count / 2], warmP95Seconds: times.isEmpty ? 0 : times[min(times.count - 1, Int(Double(times.count) * 0.95))], observations: results)
    let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try encoder.encode(report).write(to: URL(fileURLWithPath: path), options: .atomic)
}

do { try await run() } catch { fputs("\(error.localizedDescription)\n", stderr); exit(1) }
