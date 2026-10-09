// Manual harness: drives smtc.ps1 exactly the way main.cjs does.
//   node scripts/test-bridge.mjs             -> observe state for 10s
//   node scripts/test-bridge.mjs "seek 30"   -> observe, then send a command
import { spawn } from "child_process";
import readline from "readline";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const cmd = process.argv[2];

const ps = spawn(
  "powershell.exe",
  ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", path.join(__dirname, "..", "electron", "smtc.ps1")],
  { windowsHide: true },
);

readline.createInterface({ input: ps.stdout }).on("line", (line) => {
  let msg;
  try {
    msg = JSON.parse(line);
  } catch {
    console.log("UNPARSEABLE:", line.slice(0, 200));
    return;
  }
  if (msg.type === "state") {
    console.log(
      `STATE ${msg.title ?? "(none)"} — ${msg.artist ?? ""} | ${msg.status} ` +
        `${msg.position?.toFixed(1)}/${msg.duration?.toFixed(1)} vol=${msg.volume} ` +
        `app=${msg.appId} caps=${JSON.stringify(msg.caps)}`,
    );
  } else {
    console.log("ERROR:", msg.message);
  }
});

ps.stderr.on("data", (d) => console.log("STDERR:", d.toString().trim().slice(0, 400)));
ps.on("exit", (c) => console.log("ps exited", c));

if (cmd) {
  setTimeout(() => {
    console.log(`>>> sending: ${cmd}`);
    ps.stdin.write(cmd + "\n");
  }, 4000);
}

setTimeout(() => {
  ps.stdin.end();
  setTimeout(() => process.exit(0), 500);
}, 10000);
