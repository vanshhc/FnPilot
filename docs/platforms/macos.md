# macOS preview

Build target: macOS 14+, Apple Silicon. The executor, app discovery, Accessibility scans, microphone capture, Fn listener, permissions, login registration and native overlay are implemented directly in Rust. The standalone Swift app and generic Swift execution bridge are removed. Only the retained offline WhisperKit decoder uses a speech-only Swift ABI. Physical Fn events remain listen-only; Fn chords cancel recording. Input Monitoring, Microphone and Accessibility permissions belong to the Tauri app process. The preview identifier is distinct from `com.fnpilot.mac`.

Implemented native paths include app discovery/activation, literal typing, close/quit distinctions, window state, bounded accessible control scans, app-scoped media controls, Core Audio and the existing private DisplayServices brightness path. A Rust-owned native nonactivating panel displays status without requesting key focus.

Compilation is not live parity evidence. Physical Fn, lock/sleep, revoked permissions, protected text, foreground races, exact labels, video/window full screen, brightness availability and nonactivation must pass on actual hardware. Private DisplayServices availability is an explicit platform limitation. External monitor brightness is unsupported.

Do not install over the legacy app, reset permission databases, or infer permission continuity from a bundle identifier. Same-signer and changed-signer upgrades need separate acceptance receipts.

Feature implementation belongs in `crates/fnpilot-platform/src/macos` or the shared Rust crates. Audio conversion lives in `fnpilot-speech::resample`; capture is shared by both native adapters. AX handles stay on a serialized worker, event taps stay on a dedicated run loop, and AppKit UI work stays on the main queue. Native metadata/ABI tests run without grants; live actions remain separate hardware acceptance checks.
