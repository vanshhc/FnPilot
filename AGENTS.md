# FnPilot development

The active application is Rust/Tauri. Put new features in Rust and the TypeScript settings UI.

- `crates/fnpilot-core`: command schema, parsers, validation, routing, session policy and budgets.
- `crates/fnpilot-runtime`: local model protocols and owned inference helpers.
- `crates/fnpilot-speech`: audio validation, resampling, model installation and portable decoding.
- `crates/fnpilot-platform/src/macos`: macOS apps, Accessibility actions, permissions, Fn listener, system controls and native overlay, implemented directly in Rust.
- `crates/fnpilot-platform/src/windows`: Windows native integration.
- `apps/desktop/src-tauri`: application/session orchestration, settings, migration and Tauri commands.
- `apps/desktop/src`: TypeScript settings UI.

`native/macos/Sources/FnPilotSpeech` is a speech-only WhisperKit binding. Its only operations are transcription and decoder unload. Do not add app features, action models, desktop targeting, capture, permissions, hotkeys, preferences or UI there. The separate speech benchmark is nonexecuting qualification tooling. `cargo test -p fnpilot-platform --test native_boundary` guards this boundary.

Preserve `Resources/commands100.json` and the frozen oracle fixtures. Default tests and benchmarks must never perform desktop actions or download model weights. Test new behavior with deterministic fixtures; real microphone/desktop checks require an explicitly prepared environment. Keep legacy installed applications, preferences and caches intact.

Run `cargo fmt --all -- --check`, `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `npm run check`, and `npm test`. On macOS also run `swift test --package-path native/macos`. See `docs/development.md` for toolchain and staging requirements.

Run tasks sequentially in the main agent; do not delegate to subagents. Independent tool reads/checks may run in parallel. Do not publish, replace an installed app, or use signing credentials without the user's authorization.
