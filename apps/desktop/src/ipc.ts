import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { defaultSettings, initialState, type Settings, type Snapshot } from "./state";

export interface Backend {
  readonly mock: boolean;
  snapshot(): Promise<Snapshot>;
  settings(): Promise<Settings>;
  save(settings: Settings): Promise<void>;
  submit(input: string): Promise<number>;
  cancel(): Promise<void>;
  pause(paused: boolean): Promise<void>;
  diagnose(): Promise<unknown>;
  previewImport(): Promise<{ settings: Settings; warnings: string[] }>;
  importLegacy(): Promise<{ settings: Settings; warnings: string[] }>;
  autostart(enabled: boolean): Promise<unknown>;
  installSpeech(source: string, manifestPath: string): Promise<{ settings: Settings; installed: boolean }>;
  cancelSetup(): Promise<void>;
  startRuntime(): Promise<{ running: boolean; settings?: Settings }>;
  stopRuntime(): Promise<unknown>;
  checkRuntime(): Promise<unknown>;
  unloadModels(): Promise<unknown>;
  requestAccessibility(): Promise<unknown>;
  requestMicrophone(): Promise<unknown>;
  requestScreenRecording(): Promise<unknown>;
  subscribe(callback: (state: Snapshot) => void): Promise<() => void>;
}

export function mockBackend(): Backend {
  let state = structuredClone(initialState);
  let settings = structuredClone(defaultSettings);
  const listeners = new Set<(state: Snapshot) => void>();
  const emit = () => { for (const listener of listeners) listener(structuredClone(state)); };
  return {
    mock: true,
    async snapshot() { return structuredClone(state); },
    async settings() { return structuredClone(settings); },
    async save(next) { settings = structuredClone(next); },
    async submit(input) { state = { ...state, generation: state.generation + 1, phase: "ready", message: `Preview only: ${input}`, actionCount: 0 }; emit(); return state.generation; },
    async cancel() { state = { ...state, generation: state.generation + 1, phase: "ready", message: "Cancelled" }; emit(); },
    async pause(paused) { state = { ...state, generation: state.generation + 1, phase: paused ? "paused" : "ready", message: paused ? "Paused" : "Ready" }; emit(); },
    async diagnose() { return { host: "mock-ui", desktopActions: false, microphone: false, modelDownloads: false }; },
    async previewImport() { return { settings: structuredClone(settings), warnings: ["Mock preview has no legacy store."] }; },
    async importLegacy() { settings.importVersion = 1; return { settings: structuredClone(settings), warnings: [] }; },
    async autostart(enabled) { settings.launchAtLoginIntent = enabled; return { enabled, mock: true }; },
    async installSpeech() { throw new Error("Model installation is available in the native app only."); },
    async cancelSetup() {},
    async startRuntime() { return { running: false }; },
    async stopRuntime() { return { running: false }; },
    async checkRuntime() { return { available: false, mock: true }; },
    async unloadModels() { return { mock: true }; },
    async requestAccessibility() { return { mock: true }; },
    async requestMicrophone() { return { mock: true, granted: false }; },
    async requestScreenRecording() { return { mock: true, granted: false }; },
    async subscribe(callback) { listeners.add(callback); return () => { listeners.delete(callback); }; },
  };
}

export function backend(): Backend {
  if (!isTauri()) return mockBackend();
  return {
    mock: false,
    snapshot: () => invoke("snapshot"), settings: () => invoke("read_settings"),
    save: settings => invoke("write_settings", { settings }),
    submit: input => invoke("submit_instruction", { input }),
    cancel: () => invoke("cancel_session"), pause: paused => invoke("set_paused", { paused }),
    diagnose: () => invoke("diagnose"),
    previewImport: () => invoke("preview_legacy_settings"), importLegacy: () => invoke("import_legacy_settings"), autostart: enabled => invoke("configure_autostart", { enabled }),
    installSpeech: (source, manifestPath) => invoke("install_speech_model", { source, manifestPath }),
    cancelSetup: () => invoke("cancel_setup"),
    startRuntime: () => invoke("start_runtime"), stopRuntime: () => invoke("stop_runtime"), checkRuntime: () => invoke("check_runtime"),
    unloadModels: () => invoke("unload_models"),
    requestAccessibility: () => invoke("request_accessibility"),
    requestMicrophone: () => invoke("request_microphone"),
    requestScreenRecording: () => invoke("request_screen_recording"),
    subscribe: callback => listen<Snapshot>("session-state", event => callback(event.payload)),
  };
}
