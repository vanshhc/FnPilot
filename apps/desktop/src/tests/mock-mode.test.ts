// @vitest-environment jsdom
import { describe, it, expect, vi } from "vitest";
import { mockBackend } from "../ipc";
import { mount } from "../settings";
import { acceptSnapshot, initialState } from "../state";

describe("model-free settings", () => {
  it("saves screenshot-agent opt-in and a vision model without starting desktop actions", async () => {
    const root = document.createElement("main"); const backend = mockBackend();
    const dispose = await mount(root, backend);
    const opt = root.querySelector<HTMLInputElement>("#allow-computer-use")!;
    expect(opt.checked).toBe(false);
    opt.checked = true;
    root.querySelector<HTMLInputElement>("#computer-use-model")!.value = "local-vision-model";
    const form = opt.closest("form")!;
    form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await vi.waitFor(async () => expect((await backend.settings()).allowComputerUse).toBe(true));
    expect((await backend.settings()).computerUseModel).toBe("local-vision-model");
    expect((await backend.snapshot()).generation).toBe(0);
    expect(root.textContent).toContain("Screenshots of the target display");
    dispose();
  });
  it("explains pending microphone approval and prevents duplicate requests", async () => {
    const root = document.createElement("main"); const backend = mockBackend();
    let finish!: (value: unknown) => void;
    backend.requestMicrophone = vi.fn(() => new Promise(resolve => { finish = resolve; }));
    const dispose = await mount(root, backend);
    const request = [...root.querySelectorAll("button")].find(button => button.textContent === "Request microphone access")!;
    request.click();
    expect(request.disabled).toBe(true);
    expect(root.textContent).toContain("microphone prompt");
    request.click(); expect(backend.requestMicrophone).toHaveBeenCalledTimes(1);
    finish(true);
    await vi.waitFor(() => expect(request.disabled).toBe(false));
    expect(root.textContent).toContain("Microphone access enabled"); dispose();
  });
  it("renders untrusted text literally and never starts native actions", async () => {
    const root = document.createElement("main"); const backend = mockBackend(); const dispose = await mount(root, backend);
    await backend.submit('<img src=x onerror="alert(1)">');
    expect(root.querySelector("img")).toBeNull(); expect(root.textContent).toContain("<img");
    expect((root.querySelector("#allow-model-actions") as HTMLInputElement).checked).toBe(false);
    expect(await backend.diagnose()).toMatchObject({ desktopActions: false, microphone: false }); dispose();
  });
  it("ignores stale session events and unknown protocol versions", () => {
    const current = { ...initialState, generation: 5, message: "new" };
    expect(acceptSnapshot(current, { ...initialState, generation: 4 })).toBe(current);
    expect(acceptSnapshot(current, { ...initialState, generation: 6, version: 2 })).toBe(current);
    const latest = { ...current, revision: 3 };
    expect(acceptSnapshot(latest, { ...current, revision: 2 })).toBe(latest);
  });
  it("reopening settings does not submit or create a session", async () => {
    const backend = mockBackend(); const root = document.createElement("main");
    const dispose = await mount(root, backend); dispose(); const close = await mount(root, backend);
    expect((await backend.snapshot()).generation).toBe(0); close();
  });
  it("exposes explicit local setup without acquiring models or enabling actions", async () => {
    const backend = mockBackend(); const root = document.createElement("main");
    const dispose = await mount(root, backend);
    expect(root.querySelector<HTMLButtonElement>("button[type=submit]")?.disabled).toBe(false);
    const install = [...root.querySelectorAll("button")].find(button => button.textContent === "Verify and install speech model");
    expect(install?.disabled).toBe(true);
    expect(await backend.startRuntime()).toEqual({ running: false });
    await expect(backend.installSpeech("/tmp/model.bin", "/tmp/model.json")).rejects.toThrow("native app only");
    expect((await backend.settings()).allowModelActions).toBe(false);
    expect((await backend.snapshot()).generation).toBe(0); dispose();
  });
});
