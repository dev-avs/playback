import { useEffect, useRef, useState } from "react";
import "./Changelog.css";
import "./Setup.css";
import { AppMark } from "./Changelog.tsx";
import { KEYS, usePersisted } from "./settings";
import type { SetupState } from "./player";

// keep in sync with STAGE_PAD in electron/main.cjs
const STAGE_PAD = 22;
const CLOSE_MS = 200;
// Spicetify's own install command, from spicetify.app
const INSTALL_SPICETIFY = "iwr -useb https://raw.githubusercontent.com/spicetify/cli/main/install.ps1 | iex";
const GET_SPOTIFY = "https://www.spotify.com/download/windows/";

type Mark = "ok" | "todo" | "wait";

const MarkIcon = ({ mark }: { mark: Mark }) => (
  <svg className="su-mark" viewBox="0 0 16 16" aria-hidden="true">
    <circle cx="8" cy="8" r="7.5" />
    {mark === "ok" && <polyline points="4.8 8.3 7 10.5 11.2 5.9" />}
    {mark === "todo" && <line x1="8" y1="4.5" x2="8" y2="11.5" />}
    {mark === "wait" && <line x1="5" y1="8" x2="11" y2="8" />}
  </svg>
);

function Step({ mark, name, children }: { mark: Mark; name: string; children: React.ReactNode }) {
  return (
    <li className={`su-step su-step--${mark}`}>
      <MarkIcon mark={mark} />
      <div className="su-body">
        <div className="su-name">{name}</div>
        <div className="su-detail">{children}</div>
      </div>
    </li>
  );
}

/**
 * The Setup window (#setup): Spotify, Spicetify and the bridge as a checklist.
 * Opened by the main process on the first run, and later when the bridge
 * waits on `spicetify apply` (see shouldShowSetup in spicetifySetup.cjs).
 * Same card and manners as the changelog: Escape closes, Tab stays inside.
 */
export default function Setup() {
  const [state, setState] = useState<SetupState | null>(null);
  const [busy, setBusy] = useState<"check" | "apply" | null>(null);
  const [copied, setCopied] = useState(false);
  const [open, setOpen] = useState(false);
  const [closing, setClosing] = useState(false);
  const [art] = usePersisted<string | null>(KEYS.art, null);
  const cardRef = useRef<HTMLDivElement>(null);
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [gettingKey, setGettingKey] = useState(false);

  useEffect(() => {
    window.player?.setupState?.().then(setState);
  }, []);

  useEffect(() => {
    const refresh = () => window.player?.spicyKeyStatus?.().then((k) => setHasKey(k.set));
    refresh();
    return window.player?.onSpicyKeyChanged?.(refresh);
  }, []);

  const getKey = () => {
    if (gettingKey) return;
    setGettingKey(true);
    window.player?.connectSpicyKey?.().finally(() => setGettingKey(false));
  };

  const run = (kind: "check" | "apply") => {
    const call = kind === "apply" ? window.player?.applySetup : window.player?.recheckSetup;
    if (!call || busy) return;
    setBusy(kind);
    call()
      .then(setState)
      .finally(() => setBusy(null));
  };

  const copy = () => {
    window.player?.copyText?.(INSTALL_SPICETIFY).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  };

  const close = () => {
    // an apply in flight finishes either way; the window just stays to show it
    if (closing || busy === "apply") return;
    setClosing(true);
    setOpen(false);
    setTimeout(() => window.player?.closeSetup?.(), CLOSE_MS);
  };

  // Open on the frame after mounting, so the entrance transitions run.
  useEffect(() => {
    const id = requestAnimationFrame(() => setOpen(true));
    return () => cancelAnimationFrame(id);
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close();
        return;
      }
      if (event.key !== "Tab") return;
      const focusable = [...(cardRef.current?.querySelectorAll<HTMLElement>("button:not(:disabled)") ?? [])];
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const current = document.activeElement as HTMLElement;
      if (event.shiftKey && (current === first || !focusable.includes(current))) {
        event.preventDefault();
        last?.focus();
      } else if (!event.shiftKey && (current === last || !focusable.includes(current))) {
        event.preventDefault();
        first?.focus();
      }
    };
    document.addEventListener("keydown", onKeyDown, true);
    return () => document.removeEventListener("keydown", onKeyDown, true);
  });

  // Size the window to the card, the same way the changelog does.
  useEffect(() => {
    const el = cardRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() =>
      window.player?.resizeSetup?.(el.getBoundingClientRect().height + STAGE_PAD * 2),
    );
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const s = state?.status;
  const checking = !state || busy === "check";
  const missing = !!s && (!s.spotify || !s.spicetify);
  const stagger = (i: number) => ({ "--cl-i": i }) as React.CSSProperties;

  let bridge: { mark: Mark; text: React.ReactNode };
  if (busy === "apply") bridge = { mark: "wait", text: "Applying Spicetify. Spotify restarts…" };
  else if (checking) bridge = { mark: "wait", text: "Checking…" };
  else if (state.ready) bridge = { mark: "ok", text: "Running in Spotify." };
  else if (!s!.spicetify) bridge = { mark: "wait", text: "Set up once Spicetify is installed." };
  else if (state.needsApply)
    bridge = {
      mark: "todo",
      text: "In place. Spotify loads it once Spicetify is applied, which restarts Spotify.",
    };
  else bridge = { mark: "todo", text: "Not set up yet." };

  return (
    <div className="stage" onContextMenu={(e) => e.preventDefault()}>
      <div
        ref={cardRef}
        className={["cl-card", "su-card", open && "is-open", closing && "is-closing"].filter(Boolean).join(" ")}
        role="dialog"
        aria-modal="true"
        aria-labelledby="su-title"
      >
        {art && <div className="cl-bg" style={{ backgroundImage: `url(${art})` }} />}
        <div className="cl-mark">
          <AppMark />
        </div>
        <div className="cl-heading" style={stagger(0)}>
          <p className="cl-brand">player</p>
          <h2 className="cl-title" id="su-title">
            {state?.ready && hasKey && !busy ? "You're all set" : "Set up lyrics"}
          </h2>
        </div>
        <p className="cl-text" style={stagger(1)}>
          Lyrics work with any player. A free Spicy Lyrics key gets word-by-word timing; Spotify and
          Spicetify add Spotify's own track and queue.
        </p>
        <ul className="su-steps" style={stagger(2)}>
          <Step mark={hasKey === null ? "wait" : hasKey ? "ok" : "todo"} name="Spicy Lyrics key">
            {hasKey === null ? (
              "Checking…"
            ) : hasKey ? (
              "Saved. Change it any time in Settings → Behavior."
            ) : gettingKey ? (
              "Sign in, create an application, then copy its key. player picks it up from the clipboard."
            ) : (
              <>
                Your own free key for word-by-word lyrics.{" "}
                <button type="button" className="su-link" onClick={getKey}>
                  Get key
                </button>
              </>
            )}
          </Step>
          <Step mark={checking ? "wait" : s!.spotify ? "ok" : "todo"} name="Spotify">
            {checking ? (
              "Checking…"
            ) : s!.spotify ? (
              "Desktop app found."
            ) : (
              <>
                Install the desktop app from spotify.com. The Microsoft Store version can't run Spicetify.{" "}
                <button type="button" className="su-link" onClick={() => window.player?.openLink?.(GET_SPOTIFY)}>
                  Get Spotify
                </button>
              </>
            )}
          </Step>
          <Step mark={checking ? "wait" : s!.spicetify ? "ok" : "todo"} name="Spicetify">
            {checking ? (
              "Checking…"
            ) : s!.spicetify ? (
              "Installed."
            ) : (
              <>
                Paste this into PowerShell, follow its prompts, then Check again:
                <span className="su-cmd">
                  <code>{INSTALL_SPICETIFY}</code>
                  <button type="button" className="su-copy" onClick={copy}>
                    {copied ? "Copied" : "Copy"}
                  </button>
                </span>
              </>
            )}
          </Step>
          <Step mark={bridge.mark} name="Bridge">
            {bridge.text}
            {state?.error && !busy && <span className="su-error">{state.error}</span>}
          </Step>
        </ul>
        <div className="cl-actions" style={stagger(3)}>
          {/* Apply checks everything again too */}
          {missing && !state?.needsApply && (
            <button type="button" className="cl-button cl-button--quiet" disabled={!!busy} onClick={() => run("check")}>
              Check again
            </button>
          )}
          {state?.needsApply ? (
            <>
              <button type="button" className="cl-button cl-button--quiet" disabled={!!busy} onClick={close}>
                Later
              </button>
              <button
                type="button"
                className="cl-button cl-button--primary"
                disabled={!!busy}
                onClick={() => run("apply")}
              >
                {busy === "apply" ? "Applying…" : "Apply now (restarts Spotify)"}
              </button>
            </>
          ) : (
            <button type="button" className="cl-button cl-button--primary" disabled={busy === "apply"} onClick={close}>
              {state?.ready ? "Done" : "Close"}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
