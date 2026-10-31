import { useEffect, useRef, useState } from "react";
import "./Changelog.css";
import { AppMark } from "./Changelog.tsx";
import { KEYS, usePersisted } from "./settings";
import { progressText } from "./changelog";
import type { UpdateState } from "./player";

// keep in sync with STAGE_PAD in electron/main.cjs
const STAGE_PAD = 22;
const CLOSE_MS = 200;

/**
 * The update window (#update): opened on launch when GitHub has a newer
 * version. Later closes it; Skip this version closes it and stops launch from
 * asking about this one; Update downloads and restarts into it. Same card and
 * manners as the changelog and Setup.
 */
export default function Update() {
  const [state, setState] = useState<UpdateState | null>(null);
  const [version, setVersion] = useState("");
  const [open, setOpen] = useState(false);
  const [closing, setClosing] = useState(false);
  const [art] = usePersisted<string | null>(KEYS.art, null);
  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    window.player?.updateState?.().then(setState);
    window.player?.appVersion?.().then(setVersion);
    return window.player?.onUpdateChanged?.(setState);
  }, []);

  const busy = state?.status === "downloading";
  const close = () => {
    if (closing || busy) return;
    setClosing(true);
    setOpen(false);
    setTimeout(() => window.player?.closeUpdate?.(), CLOSE_MS);
  };

  useEffect(() => {
    const id = requestAnimationFrame(() => setOpen(true));
    return () => cancelAnimationFrame(id);
  }, []);

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        close();
      }
    };
    document.addEventListener("keydown", onKeyDown, true);
    return () => document.removeEventListener("keydown", onKeyDown, true);
  });

  useEffect(() => {
    const el = cardRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() =>
      window.player?.resizeUpdate?.(el.getBoundingClientRect().height + STAGE_PAD * 2),
    );
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const stagger = (i: number) => ({ "--cl-i": i }) as React.CSSProperties;
  const text =
    state?.status === "downloading"
      ? `Downloading ${progressText(state)}… player restarts by itself when it's done.`
      : state?.status === "error"
        ? `The update failed: ${state.error}`
        : `player ${state?.version ?? ""} is out. You have ${version}.`;

  return (
    <div className="stage" onContextMenu={(e) => e.preventDefault()}>
      <div
        ref={cardRef}
        className={["cl-card", open && "is-open", closing && "is-closing"].filter(Boolean).join(" ")}
        role="dialog"
        aria-modal="true"
        aria-labelledby="up-title"
      >
        {art && <div className="cl-bg" style={{ backgroundImage: `url(${art})` }} />}
        <div className="cl-mark">
          <AppMark />
        </div>
        <div className="cl-heading" style={stagger(0)}>
          <p className="cl-brand">player</p>
          <h2 className="cl-title" id="up-title">
            Update available
          </h2>
        </div>
        <p className="cl-text" style={stagger(1)}>
          {text}
        </p>
        <div className="cl-actions" style={stagger(2)}>
          <button
            type="button"
            className="cl-button cl-button--quiet cl-button--start"
            disabled={busy}
            onClick={() => {
              window.player?.skipUpdate?.();
              close();
            }}
          >
            Skip this version
          </button>
          <button type="button" className="cl-button cl-button--quiet" disabled={busy} onClick={close}>
            Later
          </button>
          <button
            type="button"
            className="cl-button cl-button--primary"
            disabled={busy}
            onClick={() => window.player?.installUpdate?.()}
          >
            {busy ? "Updating…" : state?.status === "error" ? "Try again" : "Update"}
          </button>
        </div>
      </div>
    </div>
  );
}
