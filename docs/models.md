# Local model setup

Models are not included in source or downloaded during commands. Choose and provision weights explicitly, verify their upstream license and checksum, then select the backend. No cloud fallback, automatic substitution or per-command download exists.

Portable speech is enabled by the default `speech` desktop feature using `whisper-rs` 0.16.0 and its locked `whisper-rs-sys`/whisper.cpp source. Default inference is CPU-only English, greedy decoding with temperature zero, no fallback, two threads, and no printed transcription. A selected recognizer is cached between recordings. Build with CMake installed.

For a selected `speech.bin`, create `speech.manifest.json` beside it using independently verified metadata:

```json
{
  "format": "ggml-whisper",
  "upstream_revision": "EXACT_UPSTREAM_REVISION",
  "sha256": "64_HEX_DIGITS_FROM_A_TRUSTED_SOURCE",
  "size_bytes": 0,
  "license": "UPSTREAM_LICENSE_REFERENCE",
  "architecture": "cpu"
}
```

The example is deliberately invalid until completed. Do not treat a checksum computed only from an untrusted download as proof of its provenance. Core ML directories are never accepted as portable weights. Audio must contain finite samples, last 100 ms–60 seconds and pass the existing silence threshold. Portable decoding receives short audio unchanged; the retained native WhisperKit backend pads clips through its one-second boundary. macOS keeps this backend as its default until portable latency and accuracy qualify. Only one recognizer is kept loaded at a time.

Command inference supports Ollama and local OpenAI-compatible servers with four output modes. Endpoints must be HTTP literal loopback or localhost, pinned to loopback; redirects and proxy routing are disabled. Ollama remote metadata and cloud model identifiers are rejected. An independently configured local proxy can still forward upstream; configure its behavior separately.

Helper source provenance lives in `scripts/runtime-manifest.json`. Packaged helper relocation, dependent-library inventories, owned-process tree cleanup and listener ownership must pass before official releases. Never stop independently running Ollama or another user's server.


Settings can verify and copy an already downloaded portable model plus manifest into the application model store. Installation is explicit and cancellable; selected weights are verified again on load. Choose WhisperKit only for an existing small.en Core ML model directory and its tokenizer cache containing `models/openai/whisper-small.en`. Read-only legacy import can recover those selections. The pinned WhisperKit tokenizer patch fails locally when assets are absent or invalid; it never fetches replacements. “Unload all models” stops the owned helper and releases both speech caches.

For nonexecuting speech qualification, convert a provisioned corpus with `scripts/prepare-speech-corpus.py SOURCE_MANIFEST NEW_DIRECTORY`. It verifies original hashes and records conversion provenance without changing the source corpus. Run `fnpilot-bench --speech MODEL MODEL_MANIFEST WAV_CORPUS_MANIFEST REPORT [LOCAL_RUNTIME_CONFIG]` for three warm repetitions, WER, command exact-match, transcription and file-audio-to-plan stages. The retained Mac comparison tool is `swift run --package-path native/macos -c release fnpilot-whisperkit-bench --manifest MANIFEST --model openai_whisper-small.en --output REPORT --repeats 3 --pad-short-audio`. Reports do not execute desktop actions or establish native microphone/dispatch acceptance.
