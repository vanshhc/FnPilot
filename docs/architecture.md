# Architecture

`fnpilot-core` defines strict action data, deterministic parsing and exact-label routing, compatibility fixture contexts, generation-aware session state, aggregate action budgets and serial execution policy. `fnpilot-runtime` implements local Ollama/OpenAI-compatible protocols. `fnpilot-speech` validates transient 16 kHz mono audio, installs explicitly selected verified artifacts, and optionally links the pinned whisper.cpp wrapper. `fnpilot-platform` owns live process/window/control identities and native actions.

The TypeScript settings request named Rust operations. They never submit trusted action arrays. The Tauri application manifest registers command ACLs; only the locally bundled `settings` WebView has the application capability. There is no executing overlay WebView, remote navigation grant, shell plugin, executing CLI, or remote API. Untrusted strings use `textContent`.

The Rust backend uses one worker with a bounded queue for interpretation/execution while cancellation invalidates the generation immediately. Native observations are refreshed before every action. Controls require complete scans and matching signatures. Native results distinguish observed success from dispatch without verification. A cancellation cannot undo an action already dispatched to the OS.

macOS integration is implemented directly in `fnpilot-platform/src/macos` using the system C and Objective-C APIs from Rust. A bounded serialized worker owns Accessibility handles, process/window identities and control maps. The physical Fn event tap runs on its own CFRunLoop, microphone capture runs on a separate CPAL worker shared with Windows, and only AppKit UI/activation work is dispatched to the main queue. Cancellation epochs are checked before native mutations and between Unicode chunks; stale requests cannot cancel a newer session. Lock, sleep, permission loss and Escape invalidate the epoch. Audio is mixed and resampled to 16 kHz mono in Rust with anti-alias filtering.

Swift is limited to two files under `native/macos/Sources/FnPilotSpeech`: the existing offline WhisperKit decoder and its speech-only C ABI. Only transcription inputs and decoder unload cross that ABI; app context and actions do not. Callback IDs reference owned Rust channels, so late decoder callbacks cannot access freed request memory. This retained backend preserves speech behavior while portable whisper.cpp quality qualification remains outstanding. New app and platform features belong in Rust. The `native_boundary` test rejects new production Swift feature files and forbidden app responsibilities; the Swift ABI test rejects feature operations.

Windows owns UI Automation on a COM worker, microphone capture on the shared CPAL/WASAPI worker, and an ordinary-key listener. Provider timeouts and generation checks stop subsequent dispatch. Windows provider-hang behavior still requires native acceptance.

The CLI links core/runtime only. Benchmarks have no dependency on the native platform crate. Fixture `bundleID` values are legacy compatibility data; Windows live identities use process paths, creation timestamps, HWNDs and UIA runtime IDs.

The standalone Swift application, command core and CLI have been retired. A small native WhisperKit speech comparison executable remains for qualification. Frozen parser contracts and historical validation receipts remain available; macOS builds only the retained Swift speech binding.

Settings is created on demand and its WebView is destroyed on close; the tray continues running. Speech and the owned helper load only through explicit model selection/use and can be unloaded from Settings. Native macOS mutations check the Rust cancellation epoch before dispatch and between Unicode chunks. Sequence targets are carried across parser parts; unexpected foreground changes stop the remaining actions.

A single-instance host lock prevents duplicate app processes/listeners. Reopen requests show Settings and ignore all supplied arguments; they never route commands.

macOS scans prefer `AXVisibleChildren` and descend only into selected menus. Closed menu hierarchies do not consume the visible control budget; truncation, malformed native arrays and provider failures still mark scans incomplete. Window state verification polls within the native request deadline instead of assuming an animation finishes after a fixed sleep. Saved restore frames are retained until restoration succeeds.
