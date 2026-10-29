import type { Backend } from "./ipc";
import { acceptSnapshot, initialState, type Settings, type Snapshot } from "./state";
import { strings } from "./strings";

function element<K extends keyof HTMLElementTagNameMap>(tag: K, text?: string): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag); if (text !== undefined) node.textContent = text; return node;
}
export async function mount(root: HTMLElement, backend: Backend): Promise<() => void> {
  root.replaceChildren();
  let current: Snapshot = initialState;
  let settings: Settings = await backend.settings();
  const heading = element("header"); heading.append(element("h1", strings.title), element("p", strings.description));
  const mode = element("p", backend.mock ? strings.preview : strings.native); mode.className = "notice";
  const status = element("div"); status.setAttribute("role", "status"); status.setAttribute("aria-live", "polite"); status.className = "status";
  const phase = element("strong"); const message = element("p"); status.append(phase, message);
  const render = (next: Snapshot) => { current = acceptSnapshot(current, next); phase.textContent = current.phase; message.textContent = current.message; pause.textContent = current.phase === "paused" ? "Resume" : "Pause"; };
  const form = element("form"); const label = element("label", strings.instruction); label.htmlFor = "instruction";
  const input = element("textarea"); input.id = "instruction"; input.placeholder = "Open TextEdit and type Hello, world!"; input.maxLength = 16384; input.required = true;
  const controls = element("div"); controls.className = "actions";
  const submit = element("button", backend.mock ? "Preview instruction" : strings.submit); submit.type = "submit";
  const cancel = element("button", strings.cancel); cancel.type = "button";
  const pause = element("button", "Pause"); pause.type = "button";
  controls.append(submit, cancel, pause); form.append(label, input, controls);
  const reportError = (error: unknown) => { message.textContent = String(error); };
  form.addEventListener("submit", event => { event.preventDefault(); submit.disabled = true; void backend.submit(input.value).catch(reportError).finally(() => { submit.disabled = false; }); });
  cancel.addEventListener("click", () => { void backend.cancel().catch(reportError); });
  pause.addEventListener("click", () => { void backend.pause(current.phase !== "paused").catch(reportError); });
  const section = element("section"); section.append(element("h2", "Local inference"), element("p", strings.modelWarning));
  const fields = element("form");
  const field = (name: string, value: string, id: string) => { const label = element("label", name); label.htmlFor = id; const control = element("input"); control.id = id; control.value = value; fields.append(label, control); return control; };
  const runtimeLabel = element("label", "Runtime"); runtimeLabel.htmlFor = "runtime";
  const runtime = element("select"); runtime.id = "runtime";
  for (const value of ["ollama", "openAICompatible"]) { const option = element("option", value === "ollama" ? "Ollama" : "OpenAI-compatible local server"); option.value = value; runtime.append(option); }
  runtime.value = settings.configuration.runtime; fields.append(runtimeLabel, runtime);
  const endpoint = field("Loopback endpoint", settings.configuration.endpoint, "endpoint");
  const model = field("Downloaded model ID", settings.configuration.model, "model");
  const formatLabel = element("label", "Response format"); formatLabel.htmlFor = "format";
  const format = element("select"); format.id = "format";
  for (const value of ["jsonSchema", "jsonObject", "promptJSON", "toolCalls"]) { const option = element("option", value); option.value = value; format.append(option); }
  format.value = settings.configuration.outputMode; fields.append(formatLabel, format);
  const gguf = field("Local GGUF file for bundled helper", settings.ggufPath ?? "", "gguf");
  const speechLabel = element("label", "Speech recognizer"); speechLabel.htmlFor = "speech-backend";
  const recognizer = element("select"); recognizer.id = "speech-backend";
  for (const [value, name] of [["portable", "Portable whisper.cpp"], ["whisperKit", "WhisperKit small.en (macOS)"]]) { const option = element("option", name); option.value = value; recognizer.append(option); }
  recognizer.value = settings.speechBackend; fields.append(speechLabel, recognizer);
  const speech = field("Portable speech model path (verified manifest required)", settings.speechModelPath ?? "", "speech");
  const legacySpeech = field("Existing WhisperKit small.en model directory", settings.legacySpeechCache ?? "", "whisperkit");
  const tokenizer = field("Existing tokenizer cache (contains models/openai/whisper-small.en)", settings.tokenizerCache ?? "", "tokenizer");
  const keyLabel = element("label", "Hold-to-talk key"); keyLabel.htmlFor = "talk-key";
  const talkKey = element("select"); talkKey.id = "talk-key";
  for (const [value, name] of [["fn", "Fn (macOS)"], ["rightCtrl", "Right Ctrl (Windows)"], ["rightAlt", "Right Alt (Windows)"], ["f8", "F8 (Windows)"], ["f9", "F9 (Windows)"]]) { const option = element("option", name); option.value = value; talkKey.append(option); }
  talkKey.value = settings.talkKey; fields.append(keyLabel, talkKey, element("p", "Choose a key unused by your apps. Other keys cancel recording; Right Alt can conflict with AltGr. macOS requires Fn."));
  const optLabel = element("label"); optLabel.className = "checkbox";
  const opt = element("input"); opt.type = "checkbox"; opt.checked = settings.allowModelActions; opt.id = "allow-model-actions";
  optLabel.append(opt, document.createTextNode("Enable experimental model actions")); fields.append(optLabel, element("p", "Use natural requests with the active app and focused field as context. Focused non-password text (up to 2,048 characters) is shared with your local model. Invoke the talk key to request an action; typing pauses never trigger sending. In supported browsers, you can request new or close tab, next or previous tab, back, forward, reload, and go to a web address."));
  const computerLabel = element("label"); computerLabel.className = "checkbox";
  const computer = element("input"); computer.type = "checkbox"; computer.id = "allow-computer-use"; computer.checked = settings.allowComputerUse;
  computerLabel.append(computer, document.createTextNode("Enable screenshot computer use")); fields.append(computerLabel, element("p", "For in-app requests such as ‘new document’, the agent looks at the screen, moves the cursor, clicks, and checks the result. Screenshots of the target display, including other visible content, go only to your local vision model and are not saved by FnPilot. Requires Screen Recording and Accessibility on macOS. Escape stops further actions. Basic app and system commands keep their direct controls."));
  const visionModel = field("Locally installed vision model ID", settings.computerUseModel, "computer-use-model");
  const save = element("button", "Save settings"); save.type = "submit"; fields.append(save);
  fields.addEventListener("submit", event => {
    event.preventDefault();
    const next: Settings = { ...settings, allowModelActions: opt.checked, allowComputerUse: computer.checked, computerUseModel: visionModel.value.trim(), speechModelPath: speech.value.trim() || null, speechBackend: recognizer.value as Settings["speechBackend"], legacySpeechCache: legacySpeech.value.trim() || null, tokenizerCache: tokenizer.value.trim() || null, talkKey: talkKey.value as Settings["talkKey"], ggufPath: gguf.value.trim() || null, configuration: { ...settings.configuration, endpoint: endpoint.value, model: model.value, runtime: runtime.value as Settings["configuration"]["runtime"], outputMode: format.value as Settings["configuration"]["outputMode"] } };
    save.disabled = true; void backend.save(next).then(() => { settings = next; message.textContent = "Settings saved"; }).catch(reportError).finally(() => { save.disabled = false; });
  });
  section.append(fields);
  const syncFields = (next: Settings) => {
    computer.checked = next.allowComputerUse; visionModel.value = next.computerUseModel;
    settings = next; opt.checked = next.allowModelActions; endpoint.value = next.configuration.endpoint; model.value = next.configuration.model; runtime.value = next.configuration.runtime; format.value = next.configuration.outputMode;
    gguf.value = next.ggufPath ?? ""; speech.value = next.speechModelPath ?? ""; recognizer.value = next.speechBackend; legacySpeech.value = next.legacySpeechCache ?? ""; tokenizer.value = next.tokenizerCache ?? ""; talkKey.value = next.talkKey;
  };
  const helper = element("div"); helper.className = "actions";
  const startHelper = element("button", "Start bundled helper"); startHelper.type = "button"; startHelper.disabled = backend.mock;
  const stopHelper = element("button", "Stop owned helper"); stopHelper.type = "button"; stopHelper.disabled = backend.mock;
  const health = element("button", "Check local server"); health.type = "button";
  startHelper.addEventListener("click", () => { startHelper.disabled = true; void backend.startRuntime().then(result => { if (result.settings) syncFields(result.settings); message.textContent = result.running ? "Owned helper is ready" : "Helper is stopped"; }).catch(reportError).finally(() => { startHelper.disabled = backend.mock; }); });
  stopHelper.addEventListener("click", () => { void backend.stopRuntime().then(() => { message.textContent = "Owned helper stopped"; }).catch(reportError); });
  health.addEventListener("click", () => { void backend.checkRuntime().then(value => { message.textContent = JSON.stringify(value); }).catch(reportError); });
  const unload = element("button", "Unload all models"); unload.type = "button";
  unload.addEventListener("click", () => { void backend.unloadModels().then(() => { message.textContent = "Models unloaded"; }).catch(reportError); });
  helper.append(startHelper, stopHelper, health, unload); section.append(helper);
  const installForm = element("form"); installForm.append(element("h3", "Install verified portable speech weights"), element("p", "Choose files you have already downloaded and checked against trusted upstream metadata. This copies and verifies them locally."));
  const sourceLabel = element("label", "Downloaded GGML model file"); sourceLabel.htmlFor = "model-source"; const source = element("input"); source.id = "model-source"; source.required = true;
  const manifestLabel = element("label", "Provenance manifest file"); manifestLabel.htmlFor = "model-manifest"; const manifest = element("input"); manifest.id = "model-manifest"; manifest.required = true;
  const install = element("button", "Verify and install speech model"); install.type = "submit"; install.disabled = backend.mock;
  const cancelInstall = element("button", "Cancel model setup"); cancelInstall.type = "button";
  cancelInstall.addEventListener("click", () => { void backend.cancelSetup().catch(reportError); });
  installForm.addEventListener("submit", event => { event.preventDefault(); install.disabled = true; void backend.installSpeech(source.value, manifest.value).then(result => { syncFields(result.settings); message.textContent = "Verified speech model installed and selected"; }).catch(reportError).finally(() => { install.disabled = backend.mock; }); });
  installForm.append(sourceLabel, source, manifestLabel, manifest, install, cancelInstall); section.append(installForm);
  const diagnostics = element("section"); diagnostics.append(element("h2", "Setup & diagnostics"));
  const diagnose = element("button", "Check permissions and setup"); diagnose.type = "button"; const output = element("pre");
  diagnose.addEventListener("click", () => { void backend.diagnose().then(value => { output.textContent = JSON.stringify(value, null, 2); }).catch(reportError); });
  const microphone = element("button", "Request microphone access"); microphone.type = "button";
  microphone.addEventListener("click", () => {
    microphone.disabled = true;
    output.textContent = "Approve the system microphone prompt. If it is behind another window, bring FnPilot Settings to the front.";
    void backend.requestMicrophone().then(value => {
      output.textContent = value === true ? "Microphone access enabled" : value === false ? "Microphone access was not granted. Enable FnPilot in system microphone settings, then check permissions again." : JSON.stringify(value, null, 2);
    }).catch(error => { output.textContent = String(error); }).finally(() => { microphone.disabled = false; });
  });
  const accessibility = element("button", "Set up accessibility and talk key"); accessibility.type = "button";
  accessibility.addEventListener("click", () => { void backend.requestAccessibility().then(value => { output.textContent = JSON.stringify(value, null, 2); }).catch(reportError); });
  diagnostics.append(diagnose, microphone, accessibility, output);
  const screenRecording = element("button", "Request Screen Recording access (macOS)"); screenRecording.type = "button";
  screenRecording.addEventListener("click", () => { screenRecording.disabled = true; void backend.requestScreenRecording().then(value => { output.textContent = value === true ? "Screen Recording access enabled" : "Enable FnPilot in macOS System Settings → Privacy & Security → Screen Recording, then restart FnPilot if requested."; }).catch(reportError).finally(() => { screenRecording.disabled = false; }); });
  diagnostics.insertBefore(screenRecording, output);
  const previewImport = element("button", "Preview legacy Mac preferences"); previewImport.type = "button";
  const importLegacy = element("button", "Import reviewed preferences"); importLegacy.type = "button"; importLegacy.disabled = true;
  previewImport.addEventListener("click", () => { void backend.previewImport().then(value => { output.textContent = JSON.stringify(value, null, 2); importLegacy.disabled = value.settings.importVersion === settings.importVersion; }).catch(reportError); });
  importLegacy.addEventListener("click", () => { importLegacy.disabled = true; void backend.importLegacy().then(value => {
    syncFields(value.settings);
    output.textContent = JSON.stringify({ imported: true, warnings: value.warnings }, null, 2);
  }).catch(reportError); });
  const login = element("button", settings.launchAtLoginIntent ? "Disable launch at login" : "Enable launch at login"); login.type = "button";
  login.addEventListener("click", () => { const enabled = !settings.launchAtLoginIntent; login.disabled = true; void backend.autostart(enabled).then(value => { settings.launchAtLoginIntent = enabled; login.textContent = enabled ? "Disable launch at login" : "Enable launch at login"; output.textContent = JSON.stringify(value, null, 2); }).catch(reportError).finally(() => { login.disabled = false; }); });
  const migration = element("div"); migration.className = "actions"; migration.append(previewImport, importLegacy, login); diagnostics.append(migration);
  root.append(heading, mode, status, form, section, diagnostics, element("footer", strings.privacy));
  // Subscribe before the initial read so a completion during setup cannot be missed.
  const unlisten = await backend.subscribe(render); render(await backend.snapshot());
  return unlisten;
}
