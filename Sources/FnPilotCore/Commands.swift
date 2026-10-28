import Foundation

public struct Action: Codable, Equatable, Sendable {
    public var kind: String
    public var app: String?
    public var text: String?
    public var value: Double?
    public var controlID: String?
    public var direction: String?
    public init(_ kind: String, app: String? = nil, text: String? = nil, value: Double? = nil, controlID: String? = nil, direction: String? = nil) {
        self.kind = kind; self.app = app; self.text = text; self.value = value; self.controlID = controlID; self.direction = direction
    }
}

public struct Interpretation: Codable, Equatable, Sendable {
    public var status: String
    public var actions: [Action]
    public var message: String?
    public init(status: String = "actions", actions: [Action] = [], message: String? = nil) {
        self.status = status; self.actions = actions; self.message = message
    }
}

public struct DirectParser {
    public init() {}
    public func parse(_ transcript: String) -> Interpretation? {
        let original = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !original.isEmpty else { return nil }
        // Dictation is parsed before separators: "then" inside literal text is not a command.
        if let groups = capture(#"^(?:please\s+)?(?:type|write|enter text)\s+(.+)$"#, original) {
            return Interpretation(actions: [Action("type_text", text: groups[0])])
        }
        let clean = original.trimmingCharacters(in: CharacterSet(charactersIn: ".!? ")).replacingOccurrences(of: #"^please\s+"#, with: "", options: [.regularExpression, .caseInsensitive])
        if let separator = original.range(of: #"\s+(?:and then|then|and)\s+"#, options: [.regularExpression, .caseInsensitive]) {
            guard let first = parse(String(original[..<separator.lowerBound])), let rest = parse(String(original[separator.upperBound...])), first.actions.count + rest.actions.count <= 20 else { return nil }
            if first.status != "actions" { return first }
            if rest.status != "actions" { return rest }
            return Interpretation(actions: first.actions + rest.actions)
        }
        let lower = clean.lowercased()
        if let g = capture(#"^(open|launch|start|switch to|focus|quit|exit|close)\s+(.+)$"#, clean), ["app", "the app", "an app", "this app", "that app", "the application", "it"].contains(g[1].lowercased()) {
            return Interpretation(status: "clarify", message: "Which app do you mean? Say its full name.")
        }
        let simple: [String: String] = [
            "volume up":"volume_up", "turn volume up":"volume_up", "turn the volume up":"volume_up", "increase volume":"volume_up", "increase the volume":"volume_up",
            "volume down":"volume_down", "turn volume down":"volume_down", "turn the volume down":"volume_down", "decrease volume":"volume_down", "lower the volume":"volume_down",
            "mute":"mute", "mute audio":"mute", "unmute":"unmute", "unmute audio":"unmute",
            "brightness up":"brightness_up", "increase brightness":"brightness_up", "turn brightness up":"brightness_up", "brightness down":"brightness_down", "decrease brightness":"brightness_down", "lower brightness":"brightness_down",
            "play":"play", "play the video":"play", "resume playback":"play", "pause":"pause", "pause the video":"pause", "pause playback":"pause", "play pause":"toggle_playback", "toggle playback":"toggle_playback",
            "full screen the video":"fullscreen_video", "fullscreen the video":"fullscreen_video", "video full screen":"fullscreen_video", "exit video full screen":"exit_fullscreen_video", "exit video fullscreen":"exit_fullscreen_video",
            "full screen the window":"fullscreen_window", "fullscreen the window":"fullscreen_window", "full screen":"fullscreen_window", "fullscreen":"fullscreen_window", "exit full screen":"exit_fullscreen_window", "exit fullscreen":"exit_fullscreen_window",
            "minimize":"minimize_window", "minimise":"minimize_window", "minimize the window":"minimize_window", "minimise the window":"minimize_window", "minimize window":"minimize_window",
            "maximize":"maximize_window", "maximise":"maximize_window", "maximize the window":"maximize_window", "maximise the window":"maximize_window", "maximize window":"maximize_window",
            "restore":"restore_window", "restore the window":"restore_window", "restore window":"restore_window", "close":"close_window", "close the window":"close_window", "close window":"close_window"
        ]
        if let kind = simple[lower] { return Interpretation(actions: [Action(kind)]) }
        if let g = capture(#"^(?:set\s+)?(?:the\s+)?(volume|brightness)\s+(?:to\s+)?(\d+(?:\.\d+)?)\s*(?:%|percent)?$"#, clean), let number = Double(g[1]), (0...100).contains(number) {
            return Interpretation(actions: [Action(g[0].lowercased() == "volume" ? "set_volume" : "set_brightness", value: number)])
        }
        if let g = capture(#"^scroll\s+(up|down|left|right)$"#, clean) { return Interpretation(actions: [Action("scroll", direction: g[0].lowercased())]) }
        if let g = capture(#"^(open|launch|start|switch to|focus|quit|exit|close|minimize|minimise|maximize|maximise|restore)\s+(.+)$"#, clean), validAppName(g[1]) {
            let verbs = ["open":"open_app", "launch":"open_app", "start":"open_app", "switch to":"focus_app", "focus":"focus_app", "quit":"quit_app", "exit":"quit_app", "close":"close_window", "minimize":"minimize_window", "minimise":"minimize_window", "maximize":"maximize_window", "maximise":"maximize_window", "restore":"restore_window"]
            return Interpretation(actions: [Action(verbs[g[0].lowercased()]!, app: g[1])])
        }
        return nil
    }
    private func validAppName(_ value: String) -> Bool {
        !value.isEmpty && value.count < 120 && !value.contains("/") && value.range(of: #"\b(and|then|that|it|window|video)\b"#, options: [.regularExpression, .caseInsensitive]) == nil
    }
    private func capture(_ pattern: String, _ input: String) -> [String]? {
        guard let re = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]), let match = re.firstMatch(in: input, range: NSRange(input.startIndex..., in: input)) else { return nil }
        return (1..<match.numberOfRanges).compactMap { Range(match.range(at: $0), in: input).map { String(input[$0]) } }
    }
}
