// Exercise the shipped renderer and its real Rust commands, without plugins or
// mocked IPC. Run only on a disposable desktop/CI account: this starts a trial
// and writes synthetic notes to that account's Bard database and clipboard.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { cp, mkdir, readFile, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";

if (process.env.BARD_DISPOSABLE_TEST_ACCOUNT !== "1") {
  throw new Error("Use a disposable test account and set BARD_DISPOSABLE_TEST_ACCOUNT=1");
}
const application = resolve(process.argv[2] || "");
assert(process.argv[2], "Pass the installed Bard executable");
const output = resolve("native-review");
await mkdir(output, { recursive: true });
const windows = process.platform === "win32";
// Microsoft recommends attaching for apps with several WebView2 instances.
// Launch mode looks for DevToolsActivePort in a driver-created profile, while
// Bard's main and overlay webviews share the application's persistent profile.
const app = windows ? spawn(application, [], { stdio: "inherit", env: {
  ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=9222",
} }) : null;
const driver = spawn(windows ? "msedgedriver.exe" : "tauri-driver", windows ? ["--port=4444"] : [], { stdio: "inherit" });
driver.on("error", (error) => { console.error(error); process.exitCode = 1; });
let session;
const evidence = { application, platform: process.platform, checks: [] };
const check = (name) => { evidence.checks.push(name); console.log(`PASS ${name}`); };
async function request(path, body, method = body === undefined ? "GET" : "POST") {
  const response = await fetch(`http://127.0.0.1:4444${path}`, {
    method, headers: { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(90_000),
  });
  const result = await response.json();
  if (!response.ok || result.value?.error) throw new Error(JSON.stringify(result));
  return result.value;
}
const command = (path, body, method) => request(`/session/${session}${path}`, body, method);
const script = (script, args = []) => command("/execute/sync", { script, args });
async function invoke(name, args = {}, allowError = false) {
  const result = await command("/execute/async", {
    script: "const done=arguments[arguments.length-1]; window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]).then(value=>done({ok:true,value}),error=>done({ok:false,error:String(error)}));",
    args: [name, args],
  });
  if (!allowError && !result.ok) throw new Error(`${name}: ${result.error}`);
  return allowError ? result : result.value;
}
async function selectWindow(label) {
  const until = Date.now() + 30_000;
  while (Date.now() < until) {
    for (const handle of await command("/window/handles")) {
      await command("/window", { handle });
      if (await script("return window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label") === label) return;
    }
    await new Promise((done) => setTimeout(done, 200));
  }
  throw new Error(`No ${label} webview`);
}
async function screenshot(name) {
  await script("return document.fonts.ready");
  const png = await command("/screenshot");
  await writeFile(`${output}/${name}.png`, Buffer.from(png, "base64"));
}
try {
  if (app) {
    app.on("exit", (code, signal) => { evidence.appExit = { code, signal }; });
    const deadline = Date.now() + 60_000;
    while (true) {
      try {
        const response = await fetch("http://127.0.0.1:9222/json/version", { signal: AbortSignal.timeout(1000) });
        if (response.ok) break;
      } catch { /* WebView2 may still be starting. */ }
      if (Date.now() > deadline || evidence.appExit) throw new Error(`WebView2 did not start: ${JSON.stringify(evidence.appExit ?? {})}`);
      await new Promise((done) => setTimeout(done, 200));
    }
  }
  for (let attempt = 0; ; attempt++) {
    try { await request("/status"); break; }
    catch (error) { if (attempt === 50) throw error; await new Promise((done) => setTimeout(done, 200)); }
  }
  const result = await request("/session", {
    capabilities: { alwaysMatch: windows
      ? { browserName: "webview2", "ms:edgeChromium": true, "ms:edgeOptions": { debuggerAddress: "127.0.0.1:9222" } }
      : { browserName: "wry", "tauri:options": { application } } },
  });
  session = result.sessionId;
  await command("/timeouts", { script: 30_000, implicit: 10_000 });
  await selectWindow("main");
  const state = await invoke("get_state");
  assert.equal(state.appVersion, JSON.parse(await readFile("package.json", "utf8")).version);
  assert.equal(state.platform, process.platform === "win32" ? "windows" : "linux");
  assert.equal(state.storageNotice, undefined);
  check("packaged version, platform and persistent storage initialize");
  await invoke("start_trial");
  await invoke("set_onboarding_completed", { done: true });
  await invoke("show_panel");
  await invoke("add_item", { content: "Bard native smoke — synthetic note" });
  let next = await invoke("get_state");
  const note = next.items.find((item) => item.content === "Bard native smoke — synthetic note");
  assert(note, "note must reach the real database");
  const blank = await invoke("update_item", { id: note.id, content: "  " }, true);
  assert.equal(blank.ok, false);
  next = await invoke("get_state");
  assert.equal(next.items.find((item) => item.id === note.id).content, note.content);
  await invoke("update_item", { id: note.id, content: "Bard native smoke — edited note" });
  next = await invoke("get_state");
  assert.equal(next.items.find((item) => item.id === note.id).content, "Bard native smoke — edited note");
  check("real note create/edit and blank edit rejection preserve data");
  await invoke("copy_item", { id: note.id });
  check("native clipboard write succeeds");
  await invoke("set_theme", { theme: "dark" });
  assert.equal((await invoke("get_state")).theme, "dark");
  await screenshot("shelf-dark");
  await invoke("open_settings");
  await selectWindow("settings");
  assert(await script("return Boolean(document.querySelector('[role=tablist]'))"), "settings navigation renders");
  await screenshot("settings-dark");
  await invoke("set_theme", { theme: "light" });
  assert.equal((await invoke("get_state")).theme, "light");
  await screenshot("settings-light");
  check("native lazy settings window and theme persistence work");
  await selectWindow("main");
  await invoke("open_draw");
  await selectWindow("draw");
  assert(await script("return Boolean(document.querySelector('canvas,svg'))"), "drawing surface renders");
  await screenshot("draw");
  check("native lazy drawing window renders");
} catch (error) {
  evidence.failure = String(error);
  process.exitCode = 1;
  console.error(error);
} finally {
  await writeFile(`${output}/result.json`, JSON.stringify(evidence, null, 2));
  const logs = windows
    ? join(process.env.LOCALAPPDATA, "com.miisodev.bard", "logs")
    : join(process.env.XDG_STATE_HOME || join(process.env.HOME, ".local/state"), "com.miisodev.bard", "logs");
  await cp(logs, `${output}/app-logs`, { recursive: true }).catch(() => {});
  if (session) await command("", undefined, "DELETE").catch(console.error);
  driver.kill();
  app?.kill();
}
