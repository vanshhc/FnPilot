# Contributing to FnPilot

Start with [development instructions](docs/development.md). Core tests and the mock settings UI need no microphone, weights, running model server, signing identity, or paid account.

Use a feature branch. Keep changes small enough to review, add behavioral regression coverage, and run the documented checks. Preserve the immutable `Resources/commands100.json` corpus; add new cases under `fixtures/` instead. Never drive a real desktop from a default unit test or benchmark.

Useful entry points:

- `crates/fnpilot-core`: deterministic parsing, strict validation, cancellation, and aggregate budgets.
- `crates/fnpilot-runtime`: local provider protocols and owned helpers.
- `crates/fnpilot-platform`: native permissions, targeting, capture, hotkeys and operations, all in Rust.
- `native/macos`: speech-only WhisperKit binding and nonexecuting speech qualification tool. App/platform features must not be added here.
- `apps/desktop/src`: TypeScript settings using a mock backend in the browser.
- `docs/validation`: explicit acceptance evidence and outstanding gates.

Contributions to FnPilot-owned code use the MIT license. Submit only code and assets you have permission to contribute, retain upstream notices, and describe the origin of dependencies, generated assets, models, and datasets. Do not add captured user speech, transcripts, accessibility contents, signing material, or private diagnostic exports.

Report vulnerabilities privately through [GitHub security reporting](https://github.com/vanshhc/FnPilot/security/advisories/new). See [SECURITY.md](SECURITY.md) and the [code of conduct](CODE_OF_CONDUCT.md).
