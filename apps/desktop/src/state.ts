export type Phase = "ready" | "recording" | "transcribing" | "interpreting" | "acting" | "clarifying" | "paused" | "error";
export interface Snapshot { version: number; generation: number; revision: number; phase: Phase; message: string; actionCount: number }
export interface Configuration { runtime: "ollama" | "openAICompatible"; endpoint: string; model: string; outputMode: "jsonSchema" | "jsonObject" | "promptJSON" | "toolCalls"; disableThinking: boolean }
export interface Settings { schemaVersion: number; allowModelActions: boolean; allowComputerUse: boolean; computerUseModel: string; configuration: Configuration; ggufPath: string | null; speechModelPath: string | null; speechBackend: "portable" | "whisperKit"; tokenizerCache: string | null; talkKey: "fn" | "rightCtrl" | "rightAlt" | "f8" | "f9"; legacySpeechCache: string | null; launchAtLoginIntent: boolean; preferBundledRuntime: boolean; importVersion: number }

export const initialState: Snapshot = { version: 1, generation: 0, revision: 0, phase: "ready", message: "Ready", actionCount: 0 };
export const defaultSettings: Settings = { schemaVersion: 1, allowModelActions: false, allowComputerUse: false, computerUseModel: "", configuration: { runtime: "ollama", endpoint: "http://127.0.0.1:11434", model: "hf.co/LiquidAI/LFM2.5-350M-GGUF:Q4_K_M", outputMode: "jsonSchema", disableThinking: false }, ggufPath: null, speechModelPath: null, speechBackend: "portable", tokenizerCache: null, talkKey: "rightCtrl", legacySpeechCache: null, launchAtLoginIntent: false, preferBundledRuntime: false, importVersion: 0 };

export function acceptSnapshot(current: Snapshot, next: Snapshot): Snapshot {
  if (next.version !== 1 || next.generation < current.generation || next.generation === current.generation && next.revision < current.revision) return current;
  return next;
}
