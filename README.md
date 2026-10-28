# FnPilot

A native, local voice controller for your Mac. Hold **Fn**, speak an English instruction, then release Fn to act. **Escape** cancels recording or stops subsequent actions.

Common commands use a direct parser. Other instructions use your selected local model. Native tool calls, JSON schema, JSON objects, and prompted JSON are supported through Ollama or an OpenAI-compatible local server. Model output always passes the same action validator.

## Current status

This is a working preview. Neither tested Liquid model met the required 95% command accuracy. Model-generated actions are therefore **off by default**. The app still runs explicit commands and exact accessible control labels directly. You can opt into experimental model actions in Settings after choosing a model you trust. See `VALIDATION.md` for measured results and remaining GUI checks.

## Open the app

Build the app using the instructions below, then open `../FnPilot.app`. Use the waveform icon in the menu bar to open Settings.

1. Enable Microphone, Accessibility, and Input Monitoring for FnPilot in System Settings if they are not already enabled. Click **Recheck permissions** afterward; Input Monitoring may require restarting the app.
2. Set **Keyboard → Press 🌐 key to → Do Nothing**. Disable Wispr Flow's Fn shortcut to reserve Fn for FnPilot. Fn combinations with other keys keep their normal behavior.
3. Load the English speech model with **Download / load**. The model is downloaded once and reused locally. Audio stays in memory and is discarded after use.
4. Configure a local command model. See `VALIDATION.md` for qualification results; a successful connection test alone is not an accuracy qualification.

### Local runtimes

- **Ollama:** `http://127.0.0.1:11434`. Set any compatible downloaded model identifier. Cloud models and non-loopback endpoints are rejected.
- **Bundled llama.cpp:** click **Start bundled server** to reuse the selected model's GGUF weights downloaded by Ollama. Alternatively choose a GGUF file. It serves only `127.0.0.1:18114`. Stop it before selecting another model. This does not change your Ollama installation.
- **LM Studio / another OpenAI-compatible server:** supply its loopback base URL, usually ending in `/v1`, and model identifier. Use a server that performs inference locally; a local proxy must not route requests to a cloud provider.

The installed Ollama 0.13.5 cannot load the Liquid GGUFs (`missing tensor 'output_norm'`). Use the bundled runtime for those models. Its source revision and licenses are recorded in `THIRD_PARTY_NOTICES.txt`.

Examples of model downloads:

```sh
ollama pull hf.co/LiquidAI/LFM2.5-350M-GGUF:Q4_K_M
ollama pull hf.co/LiquidAI/LFM2.5-1.2B-Instruct-GGUF:Q4_K_M
```

No model is downloaded or substituted automatically during an instruction. No cloud fallback exists.

## Instructions

- “Open Calculator” or “Switch to Chrome.”
- “Open TextEdit and type Buy milk and eggs.” Text after **type/write** stays literal; it is not parsed as another command. Typing never automatically presses Return.
- “Minimise the window,” “Maximise the window,” “Restore the window,” “Close Chrome,” or “Quit Chrome.” Close and quit are distinct.
- “Full screen the window” or “Full screen the video.” These target different controls.
- “Play,” “Pause,” “Volume up,” “Mute,” “Set volume to 35 percent,” or “Brightness down.” Default volume/brightness changes are 10 percentage points.
- “Click Save” or “Change the Search field to cats.” Explicit control labels are resolved directly. Duplicate labels require clarification; absent controls are reported.

Unnamed targets refer to the app active when Fn was pressed. The overlay does not take keyboard focus. The optional instruction box uses the last app you activated outside FnPilot.

The status popup is a small pill with no transcript. Successful commands dismiss after 0.85 seconds; unverified actions after 1.5 seconds. Errors stay for 3 seconds and clarification questions for 5 seconds in a compact two-line message. The full status remains available in Settings.

Native save dialogs remain available. The tool does not discard unsaved work automatically. A failed step stops a sequence. A check mark indicates an observed result; “check app” means the action was sent but its effect could not be confirmed.

App control depends on Accessibility. There is no screenshot recognition or arbitrary script execution. Password fields, inaccessible controls, external-monitor brightness, and output devices without system-volume controls are unsupported.

## Headless CLI

The CLI prints commands and **never executes GUI or system actions**. It does not inspect your active app. Explicit context can be supplied as a JSON file.

```sh
swift run fnpilot-cli parse 'open Notes and type Buy milk.'
swift run fnpilot-cli interpret 'make the sound quieter' \
  --runtime ollama --model MODEL_ID --format toolCalls
swift run fnpilot-cli validate command.json
swift run fnpilot-cli health --runtime openAICompatible \
  --endpoint http://127.0.0.1:18114/v1 --model MODEL_ID --format toolCalls
```

Use `--model-only` to bypass the direct parser, and `--context context.json` to supply an explicit fixture. Context has `appName`, `bundleID`, `apps`, and `controls`; see the benchmark corpus for examples. Exit codes: 0 actions, 2 clarification, 3 unsupported, 1 validation/runtime failure, 64 usage.

The desktop executable also supports `--diagnostics`, `--prepare-speech`, and `--transcribe AUDIO_FILE`. These modes do not open windows or execute actions.

### Permission troubleshooting

If voice control works only while FnPilot is focused, check **Fn key / Input Monitoring** in the app's Settings. Background listening requires macOS to allow the current app build to monitor input. Accessibility is also required for controlling other apps.

Older builds used ad-hoc signing, which changed FnPilot's identity on each rebuild. Current builds reuse the Apple Development identity pinned in `signing.json`. Routine updates made with that identity and the same bundle identifier are designed to retain permissions. Keep the installed app in the same location.

The transition from an old ad-hoc build to the Apple-signed build needs one final permission renewal. Remove FnPilot from **Privacy & Security → Accessibility** and **Input Monitoring**, add the current `FnPilot.app` again, enable it, and restart FnPilot. Use **Enable** beside Microphone in FnPilot Settings if its approval still belongs to the old build. This migration has been completed on the development Mac, and all three permissions were retained through a subsequent signed update. A deliberately changed signing identity or an OS permission reset can still require renewed approval.

Use the running app's **Recheck permissions** button to check its own permission state. Running `FnPilot --diagnostics` from a terminal or another app can inherit that host's macOS permission attribution; its output alone does not establish that the independently launched menu-bar app is permitted.

## Build and verify

Requires Xcode / Swift and an Apple Silicon Mac. The app source targets macOS 14+. The development helper runtime was tested on macOS 27; build it locally for your macOS version.

Configure signing once before packaging the app:

```sh
cp signing.example.json signing.json
security find-identity -v -p codesigning
```

Edit `signing.json`: set `identity_sha1` to your Apple Development or Developer ID Application certificate's SHA-1 fingerprint, and `team_id` to the certificate's Apple team identifier (its subject OU). The team identifier may differ from the identifier in the certificate's display name. Keep `bundle_id` unchanged for routine updates. Your certificate and private key must already be available in Keychain. The local configuration is ignored by Git; private keys are never stored in the project.

The public repository contains source, not compiled runtime binaries or model weights. For the optional bundled llama.cpp server, run `./scripts/build-runtime.sh` first; it requires CMake and builds the pinned official source. You can also use an external local Ollama or OpenAI-compatible server without that bundled runtime.

```sh
swift test
./scripts/build-app.sh
```

`build-app.sh` signs `../FnPilot.app`, its CLI, and its optional bundled runtime with the Apple signing identity pinned in `signing.json`. The script stops if the identity is missing or expired, the team is unexpected, or the final signature is invalid; it never silently falls back to ad-hoc signing. This is a local development build, not a notarized distribution.

Quit FnPilot before rebuilding into its installed location, then reopen it afterward. The build does not launch or quit apps. When moving this project to another developer's Mac, configure that developer's signing identity deliberately; that is an identity migration, not a routine update.

Check signing continuity with scratch copies, without launching apps or executing commands:

```sh
python3 scripts/verify-signing.py ../FnPilot.app Validation/signing-continuity.json --work-dir /tmp
```

The check changes the signed bundle version, confirms a different code hash with the same designated requirement, validates the versions against each other's requirement, and rejects an ad-hoc replacement and a missing identity. This is a signing check; it does not itself grant or test macOS permissions.

The frozen 100-command corpus is `Resources/commands100.json`. It covers app targeting, dictation, compound requests, windows, media, volume, brightness, accessible controls, ambiguity, and unsupported instructions. `fnpilot-bench` never executes actions:

```sh
swift run fnpilot-bench MODEL_ID Resources/commands100.json report.json \
  openAICompatible http://127.0.0.1:18114/v1 100 toolCalls
```

Results report model-only and parser-first accuracy separately. Parser-first runs use the desktop's sequence splitting, routing, and action checks with a simulated executor. Fixtures describe the initial app only: switching to another app clears its controls rather than reusing stale targets. These scores do not verify desktop execution or controls discovered after launch. See `VALIDATION.md` for hardware, measurements, methodology, and remaining verification limits. Previously saved benchmark results predate this shared routing.

### Speech model benchmark

The M4 comparison retains **Whisper small.en**: the full large-v3-turbo Core ML model improved public read-speech accuracy but took about 4.8 times as long on short commands. See `VALIDATION.md` and `Validation/speech-benchmark-summary.json` for the results and limits. Brief recordings are now padded past WhisperKit 0.9.4's one-second decode boundary; speech is validated before padding.

Prepare the public and generated corpus, then install Turbo without changing the app's selected model. Install small.en through Settings first.

```sh
python3 scripts/benchmark-speech.py prepare .build/speech-benchmark/corpus
python3 scripts/download-whisper-turbo.py
swift build -c release --product fnpilot-speech-bench --jobs 2
sandbox-exec -p '(version 1)(allow default)(deny network*)' \
  .build/release/fnpilot-speech-bench \
  --manifest .build/speech-benchmark/corpus/manifest.json \
  --model openai_whisper-small.en \
  --output .build/speech-benchmark/small.json \
  --repeats 3 --pad-short-audio
```

Repeat with `--model openai_whisper-large-v3_turbo` and a separate output file, then score both reports with `python3 scripts/benchmark-speech.py summarize SMALL_REPORT TURBO_REPORT`. Use identical padding settings for both models. Omitting `--pad-short-audio` reproduces the original, unpadded pipeline used for the archived model comparison. The standalone benchmark uses the archived comparison's decoder options and is excluded from the packaged app. It checks audio hashes, warms each model three times, measures sequential decoding, and never executes actions. Loading, first-decode time, and thermal state are recorded separately. Transcription timing excludes file loading, resampling, input validation/padding, and command interpretation/execution.

Corpus audio stays under the ignored `.build` directory. The checked-in manifest records its hashes; macOS voice updates can change regenerated synthetic audio. These clips do not measure your voice or microphone capture accuracy.
