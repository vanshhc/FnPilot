# Development

The Rust/Tauri application is the active implementation. The standalone Swift app has been retired; native macOS integration now runs in Rust; Swift is required only for the retained WhisperKit speech decoder. Target scope is macOS 14+ Apple Silicon and Windows 11 x64; running checks on newer hosted runners does not establish minimum-OS acceptance.

Install the toolchain pinned by `rust-toolchain.toml`, Node from `.node-version`, and npm 11.19.0. macOS native builds require Xcode command line tools and an Apple Silicon host. Windows native builds require Visual Studio C++ build tools, a Windows SDK, and WebView2. Portable speech builds additionally need CMake and a C/C++ compiler. See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

From a clean checkout:

```sh
npm ci
npm run check
npm test
cargo test -p fnpilot-core -p fnpilot-cli -p fnpilot-runtime -p fnpilot-speech --locked
npm run ui:dev
```

Open the printed local browser address. The browser always uses the fake backend and cannot record audio or execute desktop actions. No model or server is required.

Native contributor checks:

```sh
npm run build --workspace @fnpilot/desktop
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 -m unittest discover -s scripts/tests
npm run desktop:build
```

Windows can use `python` instead of `python3`. `desktop:build` creates a development bundle without release signing credentials. On Mac it relocates Swift runtime libraries and applies local ad-hoc signing. Its current default bundle target is the Mac `.app`; the Windows-specific config selects a per-user NSIS installer. Invoke it with `npm run tauri --workspace @fnpilot/desktop -- build --no-sign --config src-tauri/tauri.windows.conf.json`. WebView2's embedded bootstrapper needs internet when WebView2 is absent; offline deployments must provision WebView2 in advance or qualify an offline installer configuration.

`npm run desktop:dev` starts the live host using a separate `com.fnpilot.preview` identity. OS permission prompts apply to that host. Native builds include the real portable decoder without downloading weights. The macOS build also prepares pinned WhisperKit source with a fail-closed offline tokenizer patch. CMake and a compiler are required. Select an existing WhisperKit small.en model/cache or verify and install portable GGML weights from Settings; see [models.md](models.md). `--no-default-features` remains available for an explicitly decoder-free Rust host build.

`desktop:build` builds the pinned llama-server source when its cached provenance does not match, stages the nonexecuting CLI/benchmark and dependency licenses, then bundles everything. This downloads dependency source code, never model weights. To make the helper available in a debug host, run `python3 scripts/stage-desktop.py` first. Standalone Mac speech binding checks from a clean tree start with `python3 scripts/prepare-whisperkit.py`, then `swift test --package-path native/macos`. Preparation records the exact upstream revision and disables tokenizer network fallback.

```sh
cargo run -p fnpilot-cli -- parse 'Open TextEdit and type Open Chrome then quit nothing!'
cargo run -p fnpilot-bench -- --parser-only Resources/commands100.json /tmp/fnpilot-parser-report.json
```

The CLI and benchmark never observe or control the desktop. On Windows use an appropriate local report path. Native microphone, physical-key, accessibility, focus and signed-install acceptance are separate from default tests. Read [platform limits](platforms/windows.md) before testing native actions, and use disposable documents.

The WhisperKit speech binding links Swift into the Rust host. A newer Xcode can require versioned Swift runtime libraries during local development. Official packaging must relocate these libraries into the app before signing; a developer-toolchain rpath is not a distributable dependency.

For native macOS development, an ad-hoc rebuild changes the code requirement and can invalidate existing permission grants even when System Settings shows the switches on. The build defaults to ad-hoc signing and never selects Keychain credentials automatically. To use an explicitly authorized Apple Development certificate, run:

```sh
FNPILOT_DEV_SIGNING_IDENTITY='Apple Development: YOUR CERTIFICATE NAME' npm run desktop:build
```

This signs the local preview and its copied Swift libraries, retains the microphone entitlement, verifies the bundle, and leaves the pinned helper bytes/provenance intact. It does not notarize, publish or install the app. Grant permissions once for the selected development identity, then keep the same certificate and bundle identifier for subsequent builds. When switching from an ad-hoc preview, remove the stale Preview entries in Accessibility/Input Monitoring and add the exact workspace bundle again. Verify permission state from the app itself. The official Developer ID release workflow remains separate.

Quit previously installed FnPilot builds before enabling the new host’s talk-key permissions, so two different application identities do not listen to the same key. Reopening the new app reuses one host and shows Settings; command-line arguments never authorize desktop actions.

New features must use the Rust modules listed in [AGENTS.md](../AGENTS.md). macOS platform features live in `crates/fnpilot-platform/src/macos`, never in the speech binding. Run `cargo test -p fnpilot-platform --test native_boundary` to check the boundary. The default macOS contract tests read permission state and application metadata without prompting or executing actions; speech ABI tests reject attempted feature operations without loading weights.

## Contextual model actions

Enable **experimental model actions** in Settings to interpret whole spoken requests with the configured local model. This path does not select tools or discard accessible controls based on command keywords. The direct parser remains available when model actions are disabled. No typing pause triggers an action; invoke the talk key and speak the request.

The model receives the active app, up to 90 accessible controls, focus state, and up to 2,048 characters of the focused non-password field. Longer drafts are omitted rather than truncated. Labels and field text are untrusted data, not instructions. Natural requests such as “send it” can select a visible Send button; “go there” can submit the focused browser address field. Ambiguous intent should produce a short clarification. Accuracy depends on the configured model and the app's accessibility support; fixture tests do not establish live model accuracy.

`submit_control` takes an observed control ID and submits existing text with Return. It requires a complete scan and one focused, nonempty accessible text field. The executor rechecks native focus and the draft before dispatch, and reports the result as unverified. Changes to the observed focused field or its value stop control actions before dispatch. Submission never rewrites the draft or selects a different field. These checks do not verify message delivery or successful page navigation.

Default tests use synthetic controls and canned model responses. Live WhatsApp/Chrome acceptance requires a prepared desktop and disposable messages; it is not part of default checks. This feature does not install or replace an application bundle.

## Browser action group

With experimental model actions enabled, the active browser exposes a group of actions for new/close tab, next/previous tab, back/forward, reload, and URL navigation. Phrasing is interpreted by the local model. For example, “take me to example.org in a fresh tab” can produce `browser_new_tab` followed by `browser_navigate`; the current tab is used unless a new tab was requested. Ambiguous destinations require clarification.

The initial native identity registry includes Safari on macOS, and Chrome, Chromium, Firefox, Edge and Brave on macOS/Windows. This registry chooses available browser behavior; it does not match spoken phrases. App/window titles cannot enable the group. Other browsers retain generic accessible control actions. Browser tools and JSON schemas are scoped to the active app's observed capabilities, and native dispatch independently rechecks the browser identity and foreground window.

`browser_navigate` takes the web address in `text`. A bare domain uses HTTPS. It rejects search queries, embedded credentials, non-web schemes, control characters and oversized input. Navigation uses the browser's address shortcut, then requires a focused editable field within native browser toolbar ancestry, excluding web document fields. It sets and checks that field before Return, without using the clipboard. If accessibility cannot identify the address field, it stops. Shortcuts and navigation report requests as unverified; they do not claim page load success. New-tab and navigation sequences stay in the same browser window. Closing the last tab can close the window; further actions then stop.

Fixtures cover capability scoping, URL validation, both model protocols and all four output formats, sequence execution, and target changes. Live browser compatibility and shortcut behavior still need a separately prepared desktop test on each supported browser/OS.
