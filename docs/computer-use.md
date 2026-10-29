# Screenshot computer use

FnPilot can operate the focused app by looking at screenshots and moving the real cursor. For example, focus WPS Office and say **“new document.”** The local vision model can click the relevant toolbar control or open a menu visually, inspect the next screenshot, and continue until it sees the requested result. There are no app-specific menu paths or command mappings for this agent.

## Setup

1. Run a local vision-capable model in Ollama or an OpenAI-compatible server. Use an already installed model; FnPilot never downloads weights automatically. The default small text model cannot interpret screenshots.
2. In FnPilot Settings, configure the local runtime endpoint, enter the **Locally installed vision model ID**, enable **screenshot computer use**, and save. This opt-in is independent of experimental text model actions. The vision model uses the selected endpoint and response format.
3. On macOS, grant Accessibility and Screen Recording to the exact FnPilot preview bundle. The Screen Recording setup button requests the permission; macOS may require restarting the app. On Windows, desktop input and capture use the current interactive desktop without elevation.
4. Focus the intended app, hold the talk key, speak an instruction, then release. Typed instructions restore the app selected before opening FnPilot Settings.

The agent supports cursor movement, single/double/right click, scrolling, literal text and a bounded set of keyboard shortcuts. It receives one screenshot, chooses one action, then gets a new screenshot. It may interact with new windows and dialogs in the same application. Escape is reserved for user cancellation. Existing direct commands for apps, windows, browsers and system controls keep their native execution routes; other in-app requests use the screenshot agent when enabled.

## Privacy and limits

Screenshots cover the display containing the target window, including any other visible content on that display. FnPilot sends PNG images only to the configured loopback endpoint. It keeps them in memory for the active operation, releases them when the run finishes, and excludes them from diagnostics. The local server has its own logging policy.

Input is bound to the observed process and window, with screenshot coordinates mapped to native display coordinates. Before input, FnPilot checks the foreground target and compares the current screen with the captured click region or keyboard scene. It stops if these checks fail, the session is cancelled, the app changes, or 20 steps have been used. Password input and clicks outside the target app are rejected. Other visible applications can appear in screenshots but cannot receive cursor input from this agent.

Model accuracy and app rendering determine what can be completed. Dynamic content may cause the scene check to stop an action; retry after the screen settles. The agent reports completion based on the vision model's reading of the final screenshot. This is not independent verification of an application's saved state. If clarification is needed, repeat the complete instruction with the missing detail.

Default tests use deterministic screens and local protocol fixtures. Native cursor/capture acceptance in WPS Office and other applications requires a prepared environment with permissions, a provisioned vision model and disposable documents. It is not performed by the default test suite.
