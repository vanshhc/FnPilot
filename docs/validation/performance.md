# Performance qualification

Status: **release qualification pending**. A real synthetic offline WhisperKit transcription and packaged helper lifecycle smoke passed on the development Mac. These are functional checks, not paired accuracy/latency or idle-budget results. No efficiency or accuracy improvement is claimed.

Before paired runs, freeze hardware/OS/power mode, corpus/audio hashes, model/backend hashes, source/engine revisions, thread/GPU/decoding policy and repetition IDs 1–3. Distinguish first load from warm transcription, routing, dispatch and effect verification. No private user voice belongs in committed receipts.

SC2/SC3 require the same Mac, comparable model sizes/policies and a fresh Swift baseline. Keep WhisperKit available until the portable backend meets both quality and latency budgets. SC5 requires an explicitly declared CPU-only Windows reference and the frozen 1–5 second explicit-command subset.

`fnpilot-bench --parser-only` measures fixture routing only. Its speech/execution/verification fields are null and nativeAcceptance is false. Model-mode reports are still nonexecuting. `scripts/benchmark-app.py` measures explicitly attributed process trees; require five minutes, settings closed, unloaded models, hardware metadata and explicit attribution completeness before it can report an SC4 pass. Include attributed WebViews and document shared OS-service blind spots. Missing measurements are not zeroes or passes.

The portable speech benchmark and retained native WhisperKit comparison commands are documented in [model setup](../models.md). Both remain nonexecuting. A prepared WAV corpus records the original corpus hash and source audio hashes separately from converted audio hashes.

A five-minute development process-tree sample is recorded in `macos-idle-sample.json`: 109.73 MiB peak and 0.0282% mean one-core CPU. Its attribution is incomplete and it predates the single-instance integration, so it does not qualify SC4 or the final release.
