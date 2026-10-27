import Foundation

/// Resolves explicitly named controls without asking a language model to guess a target.
public struct DirectUIParser {
    public init() {}
    public func parse(_ input: String, context: CommandContext) -> Interpretation? {
        guard let request = request(input) else { return nil }
        if ["it", "that", "this", "thebutton", "button", "field", "thefield", "textfield"].contains(normalize(request.name)) { return Interpretation(status: "clarify", message: request.value == nil ? "Which control should I press? Say its label." : "Which text field should I fill? Say its label.") }
        if request.value != nil, input.range(of: #"\bfield\b"#, options: [.regularExpression, .caseInsensitive]) == nil,
           !context.controls.contains(where: { normalize($0.label) == normalize(request.name) && ["AXTextField", "AXTextArea", "AXComboBox"].contains($0.role) }) { return nil }
        return resolve(request.name, context: context, value: request.value)
    }
    public func answer(original: String, reply: String, context: CommandContext) -> Interpretation? {
        guard let request = request(original) else { return nil }
        return resolve(reply.trimmingCharacters(in: .whitespacesAndNewlines), context: context, value: request.value)
    }
    private func request(_ input: String) -> (name: String, value: String?)? {
        let text = input.trimmingCharacters(in: .whitespacesAndNewlines)
        if let g = capture(#"^(?:please\s+)?(?:click|press|activate)\s+(?:the\s+)?(.+?)(?:\s+button)?[.!]?$"#, text) { return (g[0], nil) }
        if let g = capture(#"^(?:please\s+)?(?:set|change)\s+(?:the\s+)?(.+?)(?:\s+(?:text\s+)?field)?\s+to\s+(.+)$"#, text) { return (g[0], g[1]) }
        if let g = capture(#"^(?:please\s+)?put\s+(.+?)\s+in(?:to)?\s+(?:the\s+)?(.+?)(?:\s+(?:text\s+)?field)?$"#, text) { return (g[1], g[0]) }
        return nil
    }
    private func resolve(_ name: String, context: CommandContext, value: String?) -> Interpretation {
        let matches = context.controls.filter { normalize($0.label) == normalize(name) && (value == nil || ["AXTextField", "AXTextArea", "AXComboBox"].contains($0.role)) }
        guard matches.count == 1, let selected = matches.first else {
            return Interpretation(status: matches.isEmpty ? "unsupported" : "clarify", message: matches.isEmpty ? "No accessible control named ‘\(name)’ is available in the target app." : "Several controls are named ‘\(name)’. Which one do you mean?")
        }
        return Interpretation(actions: [Action(value == nil ? "press_control" : "set_control_value", text: value, controlID: selected.id)])
    }
    private func normalize(_ value: String) -> String { value.lowercased().filter { $0.isLetter || $0.isNumber } }
    private func capture(_ pattern: String, _ input: String) -> [String]? {
        guard let re = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]), let match = re.firstMatch(in: input, range: NSRange(input.startIndex..., in: input)) else { return nil }
        return (1..<match.numberOfRanges).compactMap { Range(match.range(at: $0), in: input).map { String(input[$0]) } }
    }
}
