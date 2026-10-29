# macOS native acceptance

Status: **selected live paths pass; full native acceptance pending**. Direct Rust native compilation, speech-only ABI, local packaging and live desktop checks were exercised on Apple M4/macOS 27.0.1. See [the E2E receipt](macos-e2e.md) for physical voice, literal typing, app/window/control actions, cancellation and helper lifecycle results. Minimum macOS 14 and the complete stress matrix remain unqualified.

Record the exact app revision, signer/designated requirement, OS/hardware, permission attribution, selected model hashes and disposable test app/document for each receipt. Exercise AE1–AE3/AE5/AE7, physical Fn/chords/Escape, lock/sleep, device changes, permission revocation, target changes, save dialogs, duplicate/incomplete controls and nonactivating overlay focus. Verify observed effect separately from dispatch.

Previously installed Swift app bundles, preferences and caches remain available; the superseded standalone source has been retired, with the native WhisperKit backend retained. No cutover or permission-database modification is authorized by this receipt.
