# Public source readiness

MIT approval is recorded in `LICENSE`. Private vulnerability reporting is enabled. Contributor docs, issue/PR templates and secret-free CI definitions are present.

Before changing visibility or publishing a source snapshot:

- Inspect the exact source archive and all intended public history for credentials, private paths/content, large binaries and unlicensed assets.
- Review the generated Rust/Node inventory plus Swift/C++ dependencies and complete upstream notice collection.
- Keep all model weights, voice clips, generated voices and private benchmark artifacts excluded unless redistribution is approved.
- Run clean macOS and Windows quickstarts and observe the hosted check results.
- Configure branch protections/required checks and confirm the private security reporting path remains enabled.
- Record the approved snapshot revision, audit method, findings/remediation and rights-holder approval.

No history rewrite or repository visibility change is part of this implementation. The audit is not complete merely because `.gitignore` excludes private files.
