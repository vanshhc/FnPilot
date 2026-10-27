import Foundation
import FnPilotCore

enum UsageError: Error { case message(String) }

let usage = """
FnPilot CLI — interprets commands without controlling your Mac.

  fnpilot-cli parse "volume up"
  fnpilot-cli interpret "make the sound quieter" [options]
  fnpilot-cli validate command.json
  fnpilot-cli health [options]

Options:
  --runtime ollama|openAICompatible
  --endpoint http://127.0.0.1:11434 (Ollama) or http://127.0.0.1:18114/v1
  --model MODEL_ID
  --context context.json     Explicit fixture; never reads your active app
  --format toolCalls|jsonSchema|jsonObject|promptJSON
  --model-only              Bypass the direct parser

Outputs JSON only. Exit codes: 0 actions, 2 clarification, 3 unsupported,
1 validation/runtime error, 64 usage. No command executes GUI or system actions.
"""

func output(_ result: Interpretation) throws {
    let encoder = JSONEncoder(); encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    print(String(decoding: try encoder.encode(result), as: UTF8.self))
}
func run() async throws -> Int32 {
    let args = Array(CommandLine.arguments.dropFirst())
    guard let verb = args.first, ["parse", "interpret", "validate", "health"].contains(verb) else { print(usage); return args.isEmpty || args.first == "--help" ? 0 : 64 }
    let valueOptions: Set<String> = ["--runtime", "--endpoint", "--model", "--context", "--format"]
    var cursor = verb == "health" ? 1 : 2
    var seen = Set<String>()
    while cursor < args.count {
        let option = args[cursor]
        guard seen.insert(option).inserted else { throw UsageError.message("Duplicate option: \(option)") }
        if option == "--model-only" { cursor += 1; continue }
        guard valueOptions.contains(option), cursor + 1 < args.count, !args[cursor + 1].hasPrefix("--") else { throw UsageError.message("Unknown option or missing value: \(option)") }
        cursor += 2
    }
    func option(_ key: String) throws -> String? {
        guard let i = args.firstIndex(of: key) else { return nil }
        guard i + 1 < args.count, !args[i + 1].hasPrefix("--") else { throw UsageError.message("Missing value for \(key)") }
        return args[i + 1]
    }
    if verb == "validate" {
        guard args.count == 2 else { print(usage); return 64 }
        let result = try ActionRegistry.decode(Data(contentsOf: URL(fileURLWithPath: args[1])))
        try output(result); return result.status == "actions" ? 0 : result.status == "clarify" ? 2 : 3
    }
    guard verb == "health" || args.count >= 2 && !args[1].hasPrefix("--") else { print(usage); return 64 }
    let text = verb == "health" ? "volume up" : args[1]
    var context = CommandContext()
    if let file = try option("--context") { context = try JSONDecoder().decode(CommandContext.self, from: Data(contentsOf: URL(fileURLWithPath: file))) }
    let result: Interpretation
    if verb == "parse" {
        result = DirectParser().parse(text) ?? Interpretation(status: "unsupported", message: "The direct parser does not recognize this instruction. Try interpret with a local model.")
    } else if verb != "health", !args.contains("--model-only"), let direct = DirectParser().parse(text) { result = direct }
    else if verb != "health", !args.contains("--model-only"), let direct = DirectUIParser().parse(text, context: context) { result = direct }
    else {
        let runtimeName = try option("--runtime") ?? "ollama"
        guard let runtime = Runtime(rawValue: runtimeName) else { throw UsageError.message("Unknown runtime: \(runtimeName)") }
        let formatName = try option("--format") ?? "jsonSchema"
        guard let format = OutputMode(rawValue: formatName) else { throw UsageError.message("Unknown output format: \(formatName)") }
        let endpoint = try option("--endpoint") ?? (runtime == .ollama ? "http://127.0.0.1:11434" : "http://127.0.0.1:18114/v1")
        let model = try option("--model") ?? ModelConfiguration().model
        result = try await LocalModelInterpreter(configuration: ModelConfiguration(runtime: runtime, endpoint: endpoint, model: model, outputMode: format)).interpret(text, context: context)
    }
    try ActionRegistry.validate(result)
    try output(result)
    return result.status == "actions" ? 0 : result.status == "clarify" ? 2 : 3
}
do { exit(try await run()) }
catch UsageError.message(let message) { fputs("\(message)\n", stderr); exit(64) }
catch { fputs("\(error.localizedDescription)\n", stderr); exit(1) }
