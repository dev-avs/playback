import { useEffect, useRef, useState } from "react";
import "./Changelog.css";
import { CHANGELOG, compareVersions, type ChangelogEntry } from "./changelog";
import { KEYS, usePersisted } from "./settings";

// keep in sync with STAGE_PAD in electron/main.cjs
const STAGE_PAD = 22;
const CLOSE_MS = 200;

/** The app icon (public/logo.svg) at mark size: glass tile and visualizer bars. */
const AppMark = () => (
  <svg viewBox="0 0 64 64" aria-hidden="true">
    <defs>
      <radialGradient id="cl-p" cx="18" cy="16" r="36" gradientUnits="userSpaceOnUse">
        <stop offset="0" stopColor="#e0408f" />
        <stop offset="1" stopColor="#e0408f" stopOpacity="0" />
      </radialGradient>
      <radialGradient id="cl-b" cx="50" cy="52" r="40" gradientUnits="userSpaceOnUse">
        <stop offset="0" stopColor="#3f6bff" />
        <stop offset="1" stopColor="#3f6bff" stopOpacity="0" />
      </radialGradient>
      <clipPath id="cl-c">
        <rect x="2" y="2" width="60" height="60" rx="14" />
      </clipPath>
    </defs>
    <g clipPath="url(#cl-c)">
      <rect width="64" height="64" fill="#18181b" />
      <rect width="64" height="64" fill="url(#cl-p)" opacity=".9" />
      <rect width="64" height="64" fill="url(#cl-b)" opacity=".9" />
    </g>
    <rect x="2.5" y="2.5" width="59" height="59" rx="13.5" fill="none" stroke="#fff" strokeOpacity=".14" />
    <g fill="#fff">
      <rect x="14" y="35" width="6" height="12" rx="3" />
      <rect x="24" y="23" width="6" height="24" rx="3" />
      <rect x="34" y="29" width="6" height="18" rx="3" />
      <rect x="44" y="17" width="6" height="30" rx="3" />
    </g>
  </svg>
);

const fmtDate = (iso: string) =>
  new Date(`${iso}T12:00:00`).toLocaleDateString([], { month: "short", day: "numeric" });

function Entries({ entries, heads }: { entries: ChangelogEntry[]; heads: boolean }) {
  return (
    <div className="cl-entries">
      {entries.map((e) => (
        <section key={e.version} className="cl-entry">
          {heads && (
            <div className="cl-entry-head">
              <span className="cl-version">{e.version}</span>
              <span className="cl-date">{fmtDate(e.date)}</span>
            </div>
          )}
          <ul>
            {e.changes.map((c) => (
              <li key={c}>{c}</li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
}

/**
 * The changelog windows. `#changelog` shows only the latest update: opened by
 * the main process on a version's first launch (with `?from=<old version>`)
 * or from Settings → What's new. `#changelog-past` is the separate window its
 * Past changelogs button opens, listing every version before that one.
 * Behaves like Spicy Lyrics' notice dialog: content staggers in, Escape
 * closes, Tab stays inside, the primary button has focus.
 */
export default function Changelog() {
  const past = location.hash.startsWith("#changelog-past");
  const from = new URLSearchParams(location.hash.split("?")[1] ?? "").get("from");
  const [version, setVersion] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [closing, setClosing] = useState(false);
  const [art] = usePersisted<string | null>(KEYS.art, null);
  const cardRef = useRef<HTMLDivElement>(null);
  const primaryRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    window.player?.appVersion?.().then(setVersion).catch(() => setVersion(CHANGELOG[0].version));
  }, []);

  const to = version ?? CHANGELOG[0].version;
  // the newest entry this build knows about, at or below its own version
  const latest = CHANGELOG.find((e) => compareVersions(e.version, to) <= 0) ?? CHANGELOG[0];
  const entries = past ? CHANGELOG.filter((e) => compareVersions(e.version, latest.version) < 0) : [latest];

  const close = () => {
    if (closing) return;
    setClosing(true);
    setOpen(false);
    setTimeout(() => window.player?.closeChangelog?.(), CLOSE_MS);
  };

  // Open on the frame after mounting, so the entrance transitions run.
  useEffect(() => {
    const id = requestAnimationFrame(() => setOpen(true));
    primaryRef.current?.focus({ preventScroll: true });
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
      const focusable = [...(cardRef.current?.querySelectorAll<HTMLElement>("button") ?? [])];
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

  // Size the window to the card, the same way the widget and settings do.
  useEffect(() => {
    const el = cardRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() =>
      window.player?.resizeChangelog?.(el.getBoundingClientRect().height + STAGE_PAD * 2),
    );
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const stagger = (i: number) => ({ "--cl-i": i }) as React.CSSProperties;

  return (
    <div className="stage" onContextMenu={(e) => e.preventDefault()}>
      <div
        ref={cardRef}
        className={["cl-card", open && "is-open", closing && "is-closing"].filter(Boolean).join(" ")}
        role="dialog"
        aria-modal="true"
        aria-labelledby="cl-title"
      >
        {art && <div className="cl-bg" style={{ backgroundImage: `url(${art})` }} />}
        <div className="cl-mark">
          <AppMark />
        </div>
        <div className="cl-heading" style={stagger(0)}>
          <p className="cl-brand">player</p>
          <h2 className="cl-title" id="cl-title">
            {past ? "Past changelogs" : from ? `Updated to ${latest.version}` : "What's new"}
          </h2>
        </div>
        <p className="cl-text" style={stagger(1)}>
          {past ? (
            <>Everything before {latest.version}, newest first.</>
          ) : from ? (
            <>
              You were on <span className="cl-pill">{from}</span>. Here's what changed:
            </>
          ) : (
            <>
              <span className="cl-pill">{latest.version}</span> · {fmtDate(latest.date)}
            </>
          )}
        </p>
        <div style={stagger(2)} className="cl-stagger">
          <Entries entries={entries} heads={past} />
        </div>
        <div className="cl-actions" style={stagger(3)}>
          {!past && (
            <button
              type="button"
              className="cl-button cl-button--quiet"
              onClick={() => window.player?.openPastChangelog?.()}
            >
              Past changelogs
            </button>
          )}
          <button type="button" ref={primaryRef} className="cl-button cl-button--primary" onClick={close}>
            {from ? "Got it" : "Done"}
          </button>
        </div>
      </div>
    </div>
  );
}
