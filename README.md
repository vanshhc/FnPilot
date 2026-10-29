# FnPilot

A local voice controller built with Rust, Tauri and TypeScript, with native adapters for macOS and Windows. Hold the platform hotkey, speak an instruction, then release to act. Escape cancels recording or stops subsequent actions. FnPilot-owned code is [MIT licensed](LICENSE).

The superseded standalone Swift application has been removed. macOS permissions, audio capture, hotkeys, accessibility, app actions and native overlay now run in Rust. Swift remains only as the offline WhisperKit speech decoder binding in `native/macos`. New features belong in Rust/Tauri. The frozen migration fixtures remain regression inputs for the Rust engine.

Native acceptance, performance and official release qualification have separate requirements; see [migration status](docs/migration-status.md) and [validation receipts](docs/validation/local-checks.md).

## Build and verify

Install the Rust version pinned in `rust-toolchain.toml`, Node from `.node-version`, and npm 11.19.0. Native macOS builds require Xcode command line tools and Apple Silicon. Windows builds require Visual Studio C++ build tools, a Windows SDK and WebView2. See [development.md](docs/development.md) for prerequisites, speech builds and platform limits.

```sh
npm ci
npm run check
npm test
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 -m unittest discover -s scripts/tests
swift test --package-path native/macos
npm run desktop:build
```

Run the speech binding tests on macOS only. `desktop:build` creates a development bundle; official signing and distribution requirements are in [signing.md](docs/signing.md) and [releasing.md](docs/releasing.md).

For the browser settings UI with a fake backend and no desktop actions:

```sh
npm run ui:dev
```

For the native application:

```sh
npm run desktop:dev
```

Native builds include speech decoding and require CMake, a C/C++ compiler and explicitly provisioned model files. macOS retains WhisperKit small.en by default; Windows uses portable whisper.cpp. See [models.md](docs/models.md). Configure microphone, accessibility and hotkey permissions for the native host as described in the [macOS](docs/platforms/macos.md) or [Windows](docs/platforms/windows.md) instructions.

## Headless CLI and benchmarks

The CLI interprets commands without executing desktop or system actions. It does not inspect your active app; explicit context can be supplied as a JSON fixture.

```sh
cargo run -p fnpilot-cli -- parse 'open Notes and type Buy milk.'
cargo run -p fnpilot-cli -- interpret 'make the sound quieter' --runtime ollama --model MODEL_ID --format toolCalls
cargo run -p fnpilot-cli -- validate command.json
cargo run -p fnpilot-cli -- health --runtime openAICompatible --endpoint http://127.0.0.1:18114/v1 --model MODEL_ID --format toolCalls
cargo run -p fnpilot-bench -- --parser-only Resources/commands100.json report.json
```

Use `--model-only` to bypass direct parsing during interpretation, and `--context context.json` for explicit context. Exit codes: 0 actions, 2 clarification, 3 unsupported, 1 validation/runtime failure, 64 usage.

The immutable 100-command corpus is `Resources/commands100.json`; frozen parser contracts are in `fixtures/contracts`. Benchmarks never execute actions. Historical Swift measurements remain in `VALIDATION.md` and `Validation/`; they do not qualify the current Rust application.

## Local runtimes

For in-app actions such as “new document,” enable **screenshot computer use** and select a locally installed vision model in Settings. The agent reads screenshots, moves the real cursor, clicks and checks the updated screen. See [computer use setup and limits](docs/computer-use.md).

Configure Ollama or an OpenAI-compatible local server in Settings. Endpoints must be loopback addresses. Model output passes strict action validation; model-generated actions are off by default. No cloud fallback exists.

The optional llama.cpp helper is built from pinned source using `scripts/build-runtime.sh` on macOS or `scripts/build-runtime.ps1` on Windows. Development bundles include the pinned helper and nonexecuting CLI/benchmark. Select a GGUF file in Settings, save it, then click Start bundled helper; Stop affects only the owned process. Model weights and compiled runtime binaries are not distributed in the repository.

See [architecture](docs/architecture.md), [contributing](CONTRIBUTING.md), and [troubleshooting](docs/troubleshooting.md) for more detail.
