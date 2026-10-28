# Validation

FnPilot is a **working preview, not a qualified general-purpose voice agent**. Both required small models missed the 95% command-accuracy threshold. Model-generated actions are off by default. The direct router still handles explicit app/system commands and exact accessible control labels.

## Automated checks

- **34 tests pass.** They cover literal dictation, compound commands, close/quit and video/window distinctions, percentages, invalid/unknown fields, action budgets, clarification, local-only endpoints, both runtime response formats, native tools, unrelated tool rejection, Fn/chord/Escape/sleep state, context bounds, direct UI resolution, and token-limited response rejection.
- The truncated-plan regression was observed failing before the rejection guard was added, then passed.
- Five CLI smoke checks pass, including direct parsing, literal text, clarification/unsupported exit codes, and rejection of an unknown `--execute` option. The CLI has no executor and never drives the desktop.
- Release app and CLI build successfully. Initial builds were ad-hoc signed; the current app uses the pinned Apple Development identity and passes strict code-signature verification.
- The relocated, packaged llama.cpp helper loads local weights and returns a healthy local endpoint. Its test process was stopped afterward.

## Small-model comparison

The same frozen 100-case text-command corpus was used for both models. Both used Q4_K_M weights, the same capability filter, native tool calls, a 4,096-token context, and a 256-token output budget. The runtime was source-built llama.cpp b11669 / commit 620d222, using Metal and two CPU threads.

| Model | Model only | Direct router + model | Warm median | Warm p95 | Sampled server peak RSS |
|---|---:|---:|---:|---:|---:|
| LFM2.5-350M | 26/100 | 84/100 | 0.316 s | 1.911 s | 575 MiB |
| LFM2.5-1.2B-Instruct | 54/100 | 89/100 | 1.197 s | 5.115 s | 994 MiB |

The direct router correctly resolved 78 cases without a command model. The small models improved that to 84 and 89 respectively; this is still insufficient, especially for immediately executed desktop actions. Valid JSON/tool calls can still describe the wrong action.

The full inputs, expected actions, actual responses, timings, errors, fixture hash, and memory caveats are in `Validation/350m-100-commands.json` and `Validation/1.2b-100-commands.json`. These are development-corpus results, not a held-out accuracy guarantee or speech-recognition accuracy measurement. The recorded profile predates two final hardening changes: explicit token-limit rejection and narrowing a field-entry matcher that also matched one window phrase. The changes are covered by regression tests. No model is claimed to be qualified by those changes.

Timing was measured on this M4 MacBook Air, 16 GB RAM, macOS 27.0.1, while the Mac was also in use. The first request took 0.256 s / 0.772 s after the server was ready. Server startup took 9.173 s / 0.946 s; cache/compilation state differed, so those startup values are not a controlled model comparison. RSS was sampled every 0.5 seconds and excludes separate GPU allocations and the Swift app. These are approximate observations, not response-time promises.

Ollama 0.13.5 downloaded both Liquid models but could not load them (`missing tensor 'output_norm'`); the independent bundled runtime resolves that compatibility issue without upgrading Ollama. An exploratory Llama 3.2 1B run also failed to establish a qualified alternative. It is not selected automatically.

## Offline audio pipeline

A generated English test clip was transcribed and converted into an action plan in **1.156 seconds**, with all networking denied to the entire CLI pipeline by `sandbox-exec`. The transcript was `open calculator then volume up`. The resulting plan contained `open_app` and `volume_up`. **No actions executed.** See `Validation/offline-audio-pipeline.json`.

This measures file audio -> local transcription -> direct interpretation, including process/model startup. It excludes microphone capture and desktop execution. An earlier isolated transcription check also passed with networking denied; its reported peak footprint was about 135 MiB, excluding separate Core ML services.

## Desktop observations and remaining checks

Before the request to stop UI testing, observed live results included:

- Opening Calculator through FnPilot, then minimising and restoring it.
- Opening TextEdit and typing `Open Chrome then quit nothing!` literally, without interpreting the embedded words as actions.
- Raising and restoring output volume and built-in brightness, with read-back verification.
- The settings window rendered correctly, and macOS permission indicators eventually reported granted.
- Chrome exposed native Play and Enter full screen controls for a silent local test video.

Remaining GUI checks were deliberately stopped when requested. Real spoken microphone -> Fn release -> execution, physical Fn combinations, sleep/wake recovery, Chrome play/pause/full-screen execution, and a final GUI pass on the release build still need confirmation. The first live maximisation attempt reported a constrained frame; the resize order and asynchronous verification were corrected afterward, but that correction has not been re-tested on the live desktop. The core state machine is covered by tests; that does not substitute for hardware testing.

At initial delivery, no mouse/keyboard/window testing resumed after that request. The FnPilot test instance was stopped. The generated TextEdit test document is saved under this chat's scratch folder; existing user documents were not edited.

### Background-listening investigation after delivery

The user subsequently reported that Fn only worked while FnPilot was focused and authorized changing focus for investigation. The app was restarted without rebuilding it. A read-only `CGGetEventTapList` check found no event tap owned by the restarted FnPilot process; Secure Input was off. macOS TCC logs explicitly reported that the current app's code signature did not match the saved Accessibility approval for an older build. A standalone `--diagnostics` process reported granted permissions, but the same logs showed it was attributed to the Codex host. That result is not proof that the independently launched app has access. The running app's indicator remained disabled after rechecking.

After the user renewed the Accessibility and Input Monitoring entries and restarted the app, `CGGetEventTapList` reported an enabled session-level, listen-only event tap owned by FnPilot, with the expected `flagsChanged` and `keyDown` mask (`0x1400`). FnPilot was not the active app during the check. This confirms registration of the background listener; it does not by itself prove physical-key or microphone behavior. No permission database was edited or reset. The automation API cannot synthesize the physical Fn key, so a real Fn hold/release remains a manual verification step. No change to the command executor or app binary was made during this investigation.

The user then confirmed that holding the physical Fn key while another app was focused displayed FnPilot's Listening overlay. Background activation is now manually verified. That check deliberately used Escape to cancel; it did not test transcription or action execution.

### Compact popup update

The normal popup was reduced from 480 × 130 points to a 200 × 38 point pill without a transcript. Errors and clarification questions use a 280 × 64 point message. Dismissal changed from 12 seconds to 0.85 seconds for success, 1.5 seconds for unverified actions, 3 seconds for errors/setup notices, and 5 seconds for clarification. Cancellation still hides the popup immediately; dismissal never stops an active recording or execution.

The updated release build and strict signature verification passed. Listening and error layouts were visually checked in an isolated native preview using the actual overlay source, without recording audio or executing commands. The app was rebuilt for this change, so the previous physical background-listening result applies to the preceding binary. After restarting the updated app, macOS logs confirmed rejection of its changed signature against the saved Accessibility entry, and no background event tap was installed. Permission renewal for the updated app is required before repeating the physical-key check.

### Stable signing for future updates

The build now uses the existing Apple Development certificate pinned by fingerprint, Apple team, and bundle identifier in `signing.json`. The private key remains in Keychain. The app and its nested runtime code are signed with that identity; unavailable identities fail before the build instead of falling back to ad-hoc signing. The final Apple team, designated requirement, and full bundle signature are checked.

`scripts/verify-signing.py` passed on two versions with different signed bundle versions and CodeDirectory hashes: both had identical Apple-backed designated requirements and validated against each other's requirement. An ad-hoc replacement failed that requirement, and a nonexistent configured identity was rejected. No GUI actions or permission changes were performed by the check. The machine-specific signing receipt (`Validation/signing-continuity.json`), signing configuration, process-listener snapshot, and packaged-binary checksums remain local and are excluded from the public repository. The check can be rerun with your own signing identity.

The user completed migration of Accessibility and Input Monitoring, then approved Microphone for the Apple-signed app. The running app showed all three enabled. A further signed update changed the bundle version from 2 to 3 and changed its CodeDirectory hash. After independently relaunching that update, the app's own TCC preflight requests still returned authorized (`authValue=2`) for all three permissions, and its session-level event tap was enabled while the app was in the background. No permission settings were changed during that update, and no audio or commands were executed. See `Validation/permission-retention.json`. This verifies retention through the tested update on this Mac; it does not promise retention after deliberate signer changes or OS permission resets.

## Review coverage

A sequential in-thread correctness/reliability/security review was performed under the supplied AGENTS tool mapping. No independent reviewer or external code-review provider was used. Fixes included invalid action-field rejection, unrelated-tool rejection, cancellation generation checks, stale-control/PID/signature validation, Unicode typing focus checks, per-recording buffer tokens, explicit partial-plan rejection, bounded context/actions, window resize sequencing, and a case-insensitive product-name collision between the app and CLI.

Known limits remain explicit: Accessibility availability varies by app; exact control labels are needed for the direct UI router; output devices may not expose system volume; brightness uses an availability-checked private DisplayServices API; external-monitor brightness and screenshots are unsupported. Native save dialogs are preserved. No model is qualified by this release, and there is no automatic cloud or larger-model fallback.

## Speech model comparison — 2026-10-28

The local comparison on an **M4 MacBook Air, 16 GB RAM**, retains **Whisper small.en**. The full `openai_whisper-large-v3_turbo` Core ML conversion did not meet the requirement to improve recognition while preserving speed.

| Metric | small.en | large-v3-turbo |
|---|---:|---:|
| Short-command median transcription | 149 ms | 713 ms |
| Short-command 95th percentile | 223 ms | 1,019 ms |
| Command word-error rate | 12.21% | 12.21% |
| Normalized exact command transcript | 68.75% | 72.92% |
| Human read-speech word-error rate | 4.24% | 2.83% |

The selection rule was set before comparison: Turbo must improve recognition, and both median and 95th-percentile command latency must remain within 5% of small.en. Turbo took **4.79 times** as long at the median and **4.57 times** at the 95th percentile. It improved read-speech accuracy and some noisy commands, but regressed some other commands. No cloud service or model fallback was enabled, and the app's selected model remains small.en.

### Method and evidence

- **72 unique clips:** 24 public human read-speech clips from 12 LibriSpeech speakers, plus 12 Mac command phrases spoken by each of macOS Samantha, Daniel, and Rishi, plus a deterministic 12 dB white-noise version of Rishi's 12 commands.
- Dataset: `argmaxinc/whisperkit-test-data`, revision `df439ffa58323ef5d36d3f4caa3dd26708d834bd`. Audio comes from its `librispeech` directory; references and deterministic speaker selection use its `librispeech-10mins/metadata.json`.
- Turbo weights: `argmaxinc/whisperkit-coreml`, revision `0f63a7800b00dd0226abd051b906c246e1907482`, full `openai_whisper-large-v3_turbo` directory. Downloads were size-checked and LFS files SHA-256-checked. Compressed Turbo variants were not tested.
- Release build; WhisperKit **0.9.4**; default Core ML compute configuration; identical English decoding options, temperature zero, no fallback, one worker. Models run sequentially in separate processes. Each model receives three warm-up decodes, then three passes over the same audio, rotating clip order between passes. Repeat measurements are not independent accuracy samples.
- Word-error rate is corpus-level Levenshtein word edits divided by reference word count. Case and punctuation are ignored; apostrophes are removed within contractions. Exact match uses that same normalization. Empty results count as deletions and failures. No reference text or vocabulary hints are supplied to the decoder.
- All inference runs had networking denied by `sandbox-exec`. No microphone recording or desktop actions occurred. Timing covers decoding and text joining, excluding file loading/resampling, input validation/padding, and command interpretation/execution.
- Turbo started at nominal thermal state and ended at fair; the following small.en runs started and ended at fair. Command median times for Turbo's individual passes were 726, 691, and 720 ms. The decision uses the post-download small.en run, not an earlier baseline made while Turbo was downloading.
- Model loading and first decode are separate fields in the raw reports. They include device specialization/cache effects and must not be treated as ordinary command latency. The measured load times were 28.4 seconds for the comparison small.en process and 93.4 seconds for Turbo's first load; first decode took 0.35 and 1.71 seconds respectively.

Full evidence is in `Validation/speech-small-en.json`, `Validation/speech-large-v3-turbo.json`, `Validation/speech-benchmark-summary.json`, and the manifest/provenance JSON files. The corpus audio stays in the ignored `.build/speech-benchmark/corpus` folder. Regenerating synthetic speech after a macOS voice update may produce different hashes. These results do not measure the user's voice, live microphone capture, or end-to-end action accuracy.

### Short-recording fix

Two generated commands shorter than one second returned empty transcripts with **both** models. WhisperKit 0.9.4's `TranscribeTask` uses `while seek < seekClipEnd - 16000`; at 16 kHz, that skips the entire decode loop for recordings at or below one second. FnPilot previously accepted recordings down to 100 ms, then passed them unchanged into that loop.

`SpeechAudio.prepareForWhisper` now validates the original duration and RMS, then adds trailing zeros up to 16,001 samples only when needed. Longer audio is unchanged. Four regression tests cover brief speech, exactly one second, longer speech, and rejected silence/accidental taps. The two padding tests failed before the fix and passed afterward. Both originally empty clips then transcribed correctly through the actual small.en model.

The complete 72-clip, three-pass rerun with the fix is in `Validation/speech-small-en-padded.json`: **zero empty results**, command word-error rate **9.88%**, normalized exact command transcripts **72.92%**, median command transcription **135 ms**, and 95th percentile **198 ms**. This verifies the boundary fix, not a model accuracy upgrade. Timing variation between runs does not establish a speedup; already-supported audio follows the same decoding path.

All **51 Swift tests** pass, including the four speech tests and the previously committed audit tests. The scoring script was checked against known edit-distance cases, missing/null error fields, and incomplete-report rejection. Sequential simplification review found no changes needed in the audio fix; targeted manual correctness review covered only this task's changed files. No independent subagent review was used.
