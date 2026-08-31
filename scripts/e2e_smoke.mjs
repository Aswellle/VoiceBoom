/**
 * Tauri E2E smoke test — drives the real desktop app via tauri-driver.
 *
 * Prerequisites (must be satisfied before running):
 *   1. Build the app: `bun run tauri:build`  (produces src-tauri/target/release/voiceboom.exe)
 *   2. msedgedriver — already downloaded to D:/msedgedriver/msedgedriver.exe for this
 *      project. Override with --native-driver if yours lives elsewhere.
 *
 * What this smoke test covers (WebDriver-reachable):
 *   - app launches and the floating window webview is attached
 *   - engine label, manual start/stop button, and settings button are present
 *   - clicking the manual button toggles to the stop state
 *
 * What is intentionally NOT covered (needs real mic / OS message loop / WebView2):
 *   - global push-to-talk hotkey
 *   - actual audio capture + ASR transcription
 *   - window dragging on the desktop
 *
 * Run:
 *   node scripts/e2e_smoke.mjs
 *   node scripts/e2e_smoke.mjs --native-driver D:/msedgedriver/msedgedriver.exe
 *   node scripts/e2e_smoke.mjs --port 4444 --native-port 4445
 */
import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import net from "node:net";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { Builder, By, until } from "selenium-webdriver";

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = resolve(__dirname, "..");

// --- CLI args --------------------------------------------------------------
const args = process.argv.slice(2);
const arg = (name, fallback) => {
  const i = args.indexOf(name);
  return i !== -1 && args[i + 1] ? args[i + 1] : fallback;
};
const PORT = parseInt(arg("--port", "4444"), 10);
const NATIVE_PORT = parseInt(arg("--native-port", "4445"), 10);
const NATIVE_DRIVER = arg("--native-driver", "D:/msedgedriver/msedgedriver.exe");

// --- Paths -----------------------------------------------------------------
const tauriDriver = resolve("D:/cargo/bin/tauri-driver.exe");
const appExe = resolve(
  projectRoot,
  "src-tauri/target/release/voiceboom.exe"
);

let exitCode = 0;
const pass = (m) => console.log(`  [PASS] ${m}`);
const fail = (m) => {
  console.error(`  [FAIL] ${m}`);
  exitCode = 1;
};

function waitForPort(port, timeoutMs) {
  const start = Date.now();
  return new Promise((resolvePromise) => {
    const tryOnce = () => {
      const sock = net.connect(port, "127.0.0.1");
      sock.once("connect", () => {
        sock.destroy();
        resolvePromise(true);
      });
      sock.once("error", () => {
        sock.destroy();
        if (Date.now() - start > timeoutMs) resolvePromise(false);
        else setTimeout(tryOnce, 300);
      });
    };
    tryOnce();
  });
}

async function startTauriDriver() {
  const cmdArgs = [
    "--port",
    String(PORT),
    "--native-port",
    String(NATIVE_PORT),
    "--native-driver",
    NATIVE_DRIVER,
  ];
  console.log(`\n[tauri-driver] ${tauriDriver} ${cmdArgs.join(" ")}`);
  const proc = spawn(tauriDriver, cmdArgs, {
    cwd: projectRoot,
    stdio: ["ignore", "pipe", "inherit"],
  });
  const ready = await waitForPort(PORT, 30000);
  if (!ready) {
    proc.kill();
    throw new Error(`tauri-driver did not come up on port ${PORT} within 30s`);
  }
  console.log(`[tauri-driver] ready on http://127.0.0.1:${PORT}`);
  return proc;
}

async function main() {
  console.log("=== VoiceBoom E2E smoke test ===");

  if (!existsSync(tauriDriver)) {
    console.error(`\n[ABORT] tauri-driver not found at ${tauriDriver}`);
    process.exit(2);
  }
  if (!existsSync(NATIVE_DRIVER)) {
    console.error(`\n[ABORT] msedgedriver not found at ${NATIVE_DRIVER}`);
    process.exit(2);
  }
  if (!existsSync(appExe)) {
    console.error(`\n[ABORT] built app not found at ${appExe}\nBuild it first:  bun run tauri:build`);
    process.exit(2);
  }

  const server = await startTauriDriver();

  const driver = await new Builder()
    .usingServer(`http://127.0.0.1:${PORT}`)
    .withCapabilities({
      browserName: "webview2",
      "tauri:options": {
        application: appExe,
      },
    })
    .build();

  try {
    await driver.wait(until.elementLocated(By.css("body")), 20000);
    await driver.sleep(1500); // allow React mount + initial effects
    console.log("  app launched, webview attached");

    // Engine label present.
    try {
      const el = await driver.wait(
        until.elementLocated(By.css("[title*='语音识别引擎']")),
        8000
      );
      const txt = await el.getText();
      pass(`engine label visible: "${txt}"`);
    } catch {
      fail("engine label [title*='语音识别引擎'] not found");
    }

    // Manual start button present.
    const startBtns = await driver.findElements(
      By.css("button[title='点击开始录音']")
    );
    if (startBtns.length > 0) pass("manual start button present");
    else fail("manual start button not found");

    // Settings button present.
    const settingsBtns = await driver.findElements(
      By.css("[aria-label='打开设置']")
    );
    if (settingsBtns.length > 0) pass("settings button present");
    else fail("settings button not found");

    // Toggle start/stop via the manual button.
    if (startBtns.length > 0) {
      await startBtns[0].click();
      await driver.sleep(600);
      const stopBtns = await driver.findElements(
        By.css("button[title='点击停止录音']")
      );
      if (stopBtns.length > 0) pass("button toggled to 停止 after click");
      else fail("button did not toggle to 停止 after click");
    }

    console.log(`\nResult: ${exitCode === 0 ? "ALL PASS" : "FAILURES PRESENT"}`);
  } catch (err) {
    fail(`unexpected error: ${err?.message || err}`);
    console.error(err);
  } finally {
    try {
      await driver.quit();
    } catch {
      /* ignore */
    }
    server.kill("SIGTERM");
    await new Promise((r) => setTimeout(r, 1500));
  }

  process.exit(exitCode);
}

main().catch((e) => {
  console.error("fatal:", e);
  process.exit(1);
});
