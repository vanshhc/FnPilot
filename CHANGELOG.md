# Changelog

## Unreleased — cross-platform migration preview

- Add the Rust command core, nonexecuting CLI, frozen Swift behavior oracle, and model-free tests.
- Add a Tauri 2 host and framework-free TypeScript settings, including a browser mock mode.
- Implement macOS native integration directly in Rust and add a Windows ordinary-user adapter.
- Add portable speech setup/decoding, local runtime protocols, preference migration helpers, and redacted diagnostics.
- Add contributor checks, MIT licensing, private security reporting, and explicit release acceptance gates.

The superseded Swift app/core/CLI source is removed; only the speech-only WhisperKit binding and a speech comparison executable remain in Swift. All macOS features, capture, hotkeys, native UI and actions now live in Rust. Previously installed Swift releases and their data are preserved. No signed cross-platform release or performance improvement is claimed.

- Enable real speech in native builds, add explicit model installation and helper controls, release Settings WebViews on close, and package the pinned helper, CLI, licenses and SBOM.
- Add Windows installed-app activation, configurable hold keys, cancellation-aware local HTTP inference and protected native signing workflows.

- Remove the generic Swift execution bridge, port all macOS platform features to Rust, share native capture across platforms, add streaming anti-alias resampling and guard the speech-only Swift boundary.
- Carry sequence target identity across parser parts and keep event-tap cancellation independent of device shutdown and Accessibility work.
- Fix macOS app activation through NSWorkspace, visible-menu control scans, animated window verification and restore/maximize geometry; add scan counts/error diagnostics and clearer microphone setup feedback.
- Support explicitly selected Apple Development signing for local previews, preserving the helper inventory and stable permission identity across rebuilds. Ad-hoc signing remains the credential-free default.
