import Foundation

public enum CommandCapabilities {
    public static let descriptions: [String: String] = [
        "open_app":"Open or launch a named app", "focus_app":"Switch to an already running app", "quit_app":"Quit a named app",
        "type_text":"Type literal dictated text, preserving case and punctuation", "close_window":"Close a window without quitting the app",
        "minimize_window":"Minimize a window into the Dock", "restore_window":"Restore a minimized or maximized window", "maximize_window":"Fill the usable display with a window without entering full screen",
        "fullscreen_window":"Enter window full screen", "exit_fullscreen_window":"Leave window full screen", "fullscreen_video":"Make the video player full screen", "exit_fullscreen_video":"Leave video player full screen",
        "play":"Play or resume media", "pause":"Pause media", "toggle_playback":"Toggle play and pause", "volume_up":"Increase sound volume one step", "volume_down":"Decrease sound volume one step", "set_volume":"Set sound volume to an explicit percentage",
        "mute":"Mute sound", "unmute":"Unmute sound", "brightness_up":"Make the display brighter one step", "brightness_down":"Dim the display one step", "set_brightness":"Set display brightness to an explicit percentage",
        "press_control":"Press a currently visible control by its controlID", "set_control_value":"Fill a visible text field by controlID", "scroll":"Scroll the current window"
    ]
    public static func kinds(for input: String) -> [String] {
        let parts = CommandSequence.parts(input)
        if parts.count > 1 { return Array(Set(parts.flatMap { kinds(for: $0) })).sorted() }
        let text = input.lowercased()
        func matches(_ expression: String) -> Bool { text.range(of: expression, options: .regularExpression) != nil }
        if matches(#"^(?:please\s+)?(?:type|write|enter text)\s"#) { return ["type_text"] }
        var choices = Set<String>()
        if matches(#"\b(open|launch|start|switch|focus|quit|bring|active app)\b"#) || matches(#"^exit\s+(?!.*full.?screen)"#) { choices.formUnion(["open_app", "focus_app", "quit_app"]) }
        let video = matches(#"\b(video|play|pause|playback|resume|media)\b"#)
        let full = matches(#"full\s?screen"#)
        if video { choices.formUnion(["play", "pause", "toggle_playback", "fullscreen_video", "exit_fullscreen_video"]) }
        if matches(#"\b(window|dock|close|minimi[sz]e|maximi[sz]e|restore|shrink)\b"#) || full && !video { choices.formUnion(["close_window", "minimize_window", "restore_window", "maximize_window", "fullscreen_window", "exit_fullscreen_window"]) }
        if matches(#"\b(volume|sound|audio|speaker|speakers|louder|loud|quieter|quiet|silence|mute|unmute)\b"#) { choices.formUnion(["volume_up", "volume_down", "set_volume", "mute", "unmute"]) }
        if matches(#"\b(brightness|brighter|dimmer|dim|display|light)\b"#) || matches(#"\bscreen\b"#) && !video && !full { choices.formUnion(["brightness_up", "brightness_down", "set_brightness"]) }
        if matches(#"\b(click|press|activate|hit|button|field|search|save|scroll|put|change)\b"#) { choices.formUnion(["press_control", "set_control_value", "scroll"]) }
        return choices.sorted()
    }
    public static func tools(for input: String) -> [[String: Any]] {
        var tools = kinds(for: input).map { kind -> [String: Any] in
            var properties: [String: Any] = [:]
            var required: [String] = []
            if ["open_app", "focus_app", "quit_app"].contains(kind) { properties["app"] = ["type":"string"]; required = ["app"] }
            else if kind == "type_text" { properties["text"] = ["type":"string"]; required = ["text"] }
            else if ["set_volume", "set_brightness"].contains(kind) { properties["value"] = ["type":"number", "minimum":0, "maximum":100]; required = ["value"] }
            else if kind == "press_control" { properties["controlID"] = ["type":"string"]; required = ["controlID"] }
            else if kind == "set_control_value" { properties["controlID"] = ["type":"string"]; properties["text"] = ["type":"string"]; required = ["controlID", "text"] }
            else if kind == "scroll" { properties["direction"] = ["type":"string", "enum":["up","down","left","right"]]; required = ["direction"] }
            if !["volume_up", "volume_down", "set_volume", "mute", "unmute", "brightness_up", "brightness_down", "set_brightness", "open_app", "focus_app", "quit_app"].contains(kind) { properties["app"] = ["type":"string"] }
            return ["type":"function", "function":["name":kind, "description":descriptions[kind]!, "parameters":["type":"object", "properties":properties, "required":required, "additionalProperties":false]]]
        }
        for status in ["clarify", "unsupported"] {
            tools.append(["type":"function", "function":["name":status, "description":status == "clarify" ? "Ask a short question when the instruction or target is ambiguous" : "Explain that the requested action or control is unavailable", "parameters":["type":"object", "properties":["message":["type":"string"]], "required":["message"], "additionalProperties":false]]])
        }
        return tools
    }
}
