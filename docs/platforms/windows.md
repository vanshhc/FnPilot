# Windows preview

Intended target: Windows 11 x64, ordinary user. The adapter cross-compiles; native Windows acceptance is pending.

Current native paths: Right Ctrl hold/release with repeats suppressed and chords/Escape cancellation; WASAPI capture; UI Automation exact labels, Invoke and writable Value patterns; Unicode input and app-scoped UIA scrolling; close/minimize/maximize/placement restoration; Core Audio with readback. Native process identity includes creation time. Elevated or inaccessible targets are refused without an elevation retry.

App discovery reads the installed AppsFolder catalog and visible ordinary-user windows. It resolves explicit desktop or packaged launch identities, rejects name collisions, activates packaged apps through ApplicationActivationManager, and verifies an accessible foreground window. Process creation timestamps prevent PID reuse from redirecting a launch. The hold key is configurable as Right Ctrl, Right Alt, F8 or F9. F8/F9 are checked against registered global shortcuts on save; app-specific shortcuts can still conflict, and Right Alt may act as AltGr. Choose a key unused by your target apps. Chords cancel recording.

Windows brightness, media, and window/video full-screen actions are explicitly unsupported; maximize, F11 and global media keys are not substitutes.

The overlay uses `WS_EX_NOACTIVATE` and `SW_SHOWNOACTIVATE`. Hardware acceptance must confirm focus, keyboard layout/IME behavior, Windows lock/sleep interruption, secure desktop handling, high-DPI placement and provider hangs. Right Ctrl is an ordinary key; hardware Fn is not promised. Native tests must use disposable Notepad documents and leave save prompts intact.

WebView2 must be installed. The optional NSIS config is per-user. Its bootstrapper needs network if WebView2 is absent; official release candidates select Tauri’s offline WebView2 installer. Native installation and upgrade still require qualification. The uninstall hook removes only the matching installation’s launch-at-login entry and preserves user data.

The MSVC target selects the static CRT for the Rust host. Cargo also passes CMake policy CMP0091 and `MultiThreaded` to the native CPU speech build, keeping whisper.cpp on the same runtime; the helper uses `MultiThreaded` CRT. Qualification must still inspect actual imports/DLLs on a clean Windows machine.
