import Foundation

public struct HotkeyState {
    public enum Signal: Equatable { case start, finish, cancel }
    private var fnDown = false
    private var recording = false
    public init() {}
    public mutating func flags(fn: Bool, otherModifier: Bool) -> Signal? {
        if fn && !fnDown {
            fnDown = true
            if !otherModifier { recording = true; return .start }
        } else if !fn && fnDown {
            fnDown = false
            if recording { recording = false; return .finish }
        }
        if fn && otherModifier && recording { recording = false; return .cancel }
        return nil
    }
    public mutating func keyDown(escape: Bool) -> Signal? {
        if recording { recording = false; return .cancel }
        return escape ? .cancel : nil
    }
    public mutating func reset() { fnDown = false; recording = false }
}

public enum CommandSequence {
    public static func parts(_ input: String) -> [String] {
        let clean = input.trimmingCharacters(in: .whitespacesAndNewlines)
        if clean.range(of: #"^(?:please\s+)?(?:type|write|enter text)\s"#, options: [.regularExpression, .caseInsensitive]) != nil { return [clean] }
        if clean.range(of: #"^(?:please\s+)?(?:(?:set|change)\s+(?!(?:the\s+)?(?:volume|brightness)\b).+\s+to\s+|put\s+)"#, options: [.regularExpression, .caseInsensitive]) != nil { return [clean] }
        let pattern = #"\s+(?:and then|then|and)\s+(?=(?:open|launch|start|switch|focus|quit|close|minimi[sz]e|maximi[sz]e|restore|type|write|click|press|scroll|mute|unmute|volume|brightness|set|play|pause|exit|full\s?screen)\b)"#
        guard let range = clean.range(of: pattern, options: [.regularExpression, .caseInsensitive]) else { return [clean] }
        return [String(clean[..<range.lowerBound])] + parts(String(clean[range.upperBound...]))
    }
}
