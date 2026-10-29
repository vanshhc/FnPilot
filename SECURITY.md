# Security reporting

Use [GitHub private vulnerability reporting](https://github.com/vanshhc/FnPilot/security/advisories/new) for suspected vulnerabilities. Private reporting was enabled and its API status verified on 2026-10-28. Do not open a public issue containing an exploit or private user data.

Include the source revision, OS, affected component, a minimal synthetic reproduction, impact, and expected behavior. Remove transcripts, accessibility text, credentials, model payloads, and sensitive paths. Never attach signing keys or user recordings.

The Rust/Tauri implementation is a migration preview. Native capability, signed installation, and performance acceptance remain separate release gates. Security fixes target the current development revision; there is no supported stable cross-platform binary release yet. Maintainers will coordinate disclosure and fixes through the private report without promising an unstaffed response deadline.

FnPilot does not elevate privileges, automate secure desktops, expose an executing CLI, use cloud fallback, or treat model output as trusted code. Runtime endpoint checks cannot determine whether an independently configured local proxy forwards requests upstream.
