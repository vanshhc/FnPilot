# Releasing

Unsigned contributor bundles and official releases are separate. `native-build.yml` produces labeled unsigned previews on explicit dispatch. No installer is automatically published. The official workflow rebuilds reviewed tags in protected native jobs and produces signed candidate artifacts. It fails closed until native, provenance, signer, upgrade and performance gates have evidence and the protected signing environment is provisioned. It has not been executed.

Required official pipeline: rebuild the reviewed immutable tag in protected native jobs; build and inventory the CLI and pinned llama-server plus all dependent libraries; compile the selected speech backend; bundle licenses/SBOM and source revision; then sign native code inside-out. Never promote executables from untrusted PR jobs. Release credentials belong only in the protected environment.

For macOS, relocate Swift and helper libraries into the app, remove developer-toolchain rpaths, apply Developer ID Application with hardened runtime/audio entitlement, notarize and staple, then verify using `scripts/release/verify-macos.py APP --team-id TEAM`. Create and verify the DMG after the app is complete. Preserve same-signer update evidence and document permission renewal if signing requirements change.

For Windows, build x64 baseline helpers with every required DLL, use a per-user NSIS installer and provision WebView2, sign app/helpers/installer with the configured Authenticode identity, then run `scripts/release/verify-windows.ps1`. Test installation, running-app upgrade, spaces/Unicode paths, autostart cleanup and uninstall. Preserve settings/models unless explicitly removed.

An artifact manifest lists schema version, exact source revision, engine revisions, dependency inventory, every file's relative path/role/size/SHA-256, official status and acceptance receipts. Run `verify-artifacts.py` against a staging directory containing exactly the listed files. Required roles are app, CLI, runtime, notices and SBOM. The manifest itself lives outside that directory. Native signature verification remains mandatory; manifest fields alone cannot prove signatures.

Manual versioned downloads are the initial update mechanism. Signing identities, native machines, model qualification and installer/upgrade receipts are outstanding. Do not label a successful local build as an official or accepted release.


Configure the protected `official-release` environment with required reviewers. `ACCEPTANCE_RECEIPTS_JSON` must name the exact tagged `source_revision`, a `reviewer`, `reviewed_at`, and `receipts` for `macos`, `windows`, `performance` and `upgrade`; each receipt needs `status: passed`, a reviewable `reference` and its lowercase SHA-256. `scripts/release/check-gates.py` rejects missing or mismatched evidence. These are maintainer approval records, never generated pass claims.

macOS variables: `APPLE_SIGNING_IDENTITY` (Developer ID Application), `APPLE_TEAM_ID`, `APPLE_API_KEY_ID`, `APPLE_API_ISSUER`. Secrets: `APPLE_CERTIFICATE_BASE64` (P12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_API_KEY_BASE64` (P8). The native job imports an ephemeral keychain, signs embedded code and the app, notarizes/staples the app and DMG, verifies the expected signer, then removes credentials. Windows variables: `WINDOWS_CERTIFICATE_THUMBPRINT`, `WINDOWS_TIMESTAMP_URL` (HTTPS). Secrets: `WINDOWS_PFX_BASE64`, `WINDOWS_PFX_PASSWORD`. The job verifies the imported signer, signs embedded EXE/DLLs, bundles/signs the host and per-user installer with offline WebView2 setup, and removes the ephemeral certificate/key.

Official candidates use `com.fnpilot.desktop` / FnPilot; local development retains `com.fnpilot.preview` / FnPilot Preview. The workflow uploads candidate artifacts and an exact manifest; it does not publish a GitHub release or change repository visibility. Installing over a previous app remains an explicit cutover action after acceptance.
