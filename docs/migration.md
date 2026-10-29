# Preferences and rollback

The preview uses `com.fnpilot.preview` and a separate OS application-config directory. `settings.json` is schema-versioned; writes stage a new file, flush it, back up the old file and atomically replace it. An interrupted `.partial` file blocks a later write instead of silently overwriting a recovery candidate. Restore or remove that staging file only after examining it locally.

Import helpers accept only known legacy settings. Missing, false or malformed model-action opt-in remains disabled. A valid explicit prior opt-in can be preserved. An import version makes repeated import idempotent. Core ML cache paths remain references, not selected portable weights. OS launch-at-login is not enabled by copying intent.

The legacy preference reader is read-only. The Settings UI previews known legacy values and imports only after an explicit click. Launch-at-login registration is a separate checked OS operation. The legacy app read its login status from SMAppService rather than persisting intent, so confirm that choice manually in the preview. No existing UserDefaults store, model cache or app bundle is deleted or replaced.

The standalone Swift source and build tools have been retired. To return to a previously installed Swift release, quit FnPilot Preview, keep its settings directory as a backup, and reopen that saved app. Its preferences and caches remain available. Rebuilding the historical app requires its saved Git revision and matching signing configuration. Do not manipulate TCC databases.
