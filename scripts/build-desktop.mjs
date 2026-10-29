import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)));
const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const stage = spawnSync(process.platform === "win32" ? "python" : "python3", ["scripts/stage-desktop.py"], { cwd: root, stdio: "inherit" });
if (stage.status !== 0) process.exit(stage.status ?? 1);
const build = spawnSync(npm, ["run", "tauri", "--workspace", "@fnpilot/desktop", "--", "build", "--no-sign", "--config", process.platform === "win32" ? "src-tauri/tauri.dev.windows.conf.json" : "src-tauri/tauri.dev.conf.json"], { cwd: root, stdio: "inherit", shell: process.platform === "win32" });
if (build.status !== 0) process.exit(build.status ?? 1);
if (process.platform === "darwin") {
  const args = ["scripts/relocate-swift-runtime.py", "target/release/bundle/macos/FnPilot Preview.app"];
  if (process.env.FNPILOT_DEV_SIGNING_IDENTITY) args.push("--identity", process.env.FNPILOT_DEV_SIGNING_IDENTITY);
  const relocate = spawnSync("python3", args, { cwd: root, stdio: "inherit" });
  process.exit(relocate.status ?? 1);
}
