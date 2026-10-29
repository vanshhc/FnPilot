import { backend } from "./ipc";
import { mount } from "./settings";
import "./style.css";

const root = document.querySelector<HTMLElement>("#app")!;
void mount(root, backend()).then(dispose => { window.addEventListener("pagehide", dispose, { once: true }); }).catch(error => { root.textContent = `Could not open FnPilot Settings: ${String(error)}`; });
