# Migration baseline — 2026-10-28

The deterministic oracle was captured from Swift revision `b73a603495ccd4c0200b7ad648036a16a803f3c5` with the speech-benchmark work listed in `fixtures/contracts/manifest.json` present. Those edits were preserved. During implementation, the separate speech refactor was committed as `b4b64e41f11e24fd4c3aa934530664f2f12ed57e`; the migration did not remove or replace it.

Fresh pre-port `swift test`: 46 core tests and 5 app tests passed. The original oracle compiled only the retired `Sources/FnPilotCore` plus a temporary driver; it cannot execute desktop actions, access a microphone or contact a model server. `parser.json` contains the unchanged 100-command corpus plus 12 regression inputs, recording direct parsing, exact-label routing, sequence parts and capability lists. Hashes and provenance are in `manifest.json`.

Original corpus SHA-256: `9e6f21d9bab75aba242e8fb9f59d91bfc242e250a2fc05d9123ad5c461c69909`. Rust matches the captured deterministic outputs. Numeric JSON values such as `37` and `37.0` compare as typed numbers. One deliberate validation hardening rejects unused action fields even when explicitly null; the frozen corpus is unchanged.

Local build host: Mac16,12, Apple M4, 16 GiB RAM, macOS 27.0.1 build 26A434. This records build provenance, not a declaration that this is the SC2–SC5 reference setup. No Windows hardware reference, redistributable audio corpus, three paired warm runs, speech accuracy, total app memory or release-to-first-action latency has been measured by this migration run. Historical `VALIDATION.md` results remain historical.

The standalone Swift sources and oracle generator have been retired. Preserve the frozen oracle and its provenance; retrieve the original implementation from Git history if a historical recapture is needed. Run `python3 scripts/validate-baseline.py fixtures/contracts/manifest.json Resources/commands100.json` to validate the report. The validator rejects a mismatched corpus hash and any claimed measurement lacking hardware and all three paired runs.
