# Troubleshooting

Open Settings → Check permissions and setup. Diagnostics identify the desktop host, platform, permission status and selected backend state without exporting transcript, control labels, model requests or home paths. CLI fixture results do not establish GUI permissions.

If the Mac Fn listener is unavailable, enable Input Monitoring for the actual FnPilot Preview app and restart it. Grant Accessibility and Microphone separately. Permission changes apply to the running host; a terminal-launched child's status does not establish the installed app's status.

If Windows refuses a target, check that it is ordinary-user and accessible. FnPilot never retries as administrator, targets another window as a fallback, or automates UAC/login screens. Use the documented exact running app name. Unsupported capabilities stop with a limitation message.

If speech setup fails, verify the model file, adjacent manifest, format, checksum and size. Core ML caches are not GGML weights. A model-free developer build must be rebuilt with `--features speech` before real speech decoding is available.

For a runtime error, check a downloaded local model, loopback endpoint and supported output format. Runtime health never changes model-action opt-in. Occupied helper ports and missing native libraries are setup failures, not permission to adopt an unrelated server.

For a settings recovery error, retain the last valid JSON, `.backup.json` and staging file. Never post these files publicly without reviewing sensitive paths. Use synthetic inputs in issue reports.
