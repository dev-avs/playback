import { useEffect, useRef, useState } from "react";
import "./Changelog.css";
import { AppMark } from "./Changelog.tsx";
import { CHANGE_KINDS, compareVersions, type ChangeKind } from "./changelog";
import { KEYS, usePersisted } from "./settings";
import type { Release, UpdateState } from "./player";

// keep in sync with STAGE_PAD in electron/main.cjs
const STAGE_PAD = 22;
const CLOSE_MS = 200;
/** Points of a release's notes shown before Read more, each cut to two lines. */
const NOTES_PREVIEW = 1;
/** About what fits on two lines; a longer first point gets Read more too. */
const PREVIEW_CHARS = 90;
/** How long the warning's Revert has to be held, like Uninstall in Settings. */
const HOLD_MS = 1500;

interface Section {
  kind: ChangeKind | null;
  name: string;
  points: string[];
}

/** A release body (`### Section` headings, `- ` points) as changelog sections. */
function sections(body: string): Section[] {
  const out: Section[] = [];
  for (const raw of body.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line) continue;
    if (line.startsWith("#")) {
      const name = line.replace(/^#+\s*/, "");
      const kind = (Object.keys(CHANGE_KINDS) as ChangeKind[]).find((k) => CHANGE_KINDS[k] === name) ?? null;
      out.push({ kind, name, points: [] });
    } else {
      if (!out.length) out.push({ kind: null, name: "", points: [] });
      out[out.length - 1].points.push(line.replace(/^[-*]\s*/, ""));
    }
  }
  return out;
}

/** The first `n` points, keeping the headings they sit under. */
function firstPoints(list: Section[], n: number): Section[] {
  const out: Section[] = [];
  for (const s of list) {
    if (n <= 0) break;
    out.push({ ...s, points: s.points.slice(0, n) });
    n -= s.points.length;
  }
  return out;
}

/**
 * The version control window (#versions), from Settings → Behavior → Version
 * control: every release from GitHub that can be installed (1.3.7 on), newest
 * first, each with the start of its notes. Update or Revert installs that one
 * and restarts player. Read more opens one release's notes at a time.
 *
 * Versions before 1.3.7 have no updater: once on one, player cannot update
 * itself or come back here. Reverting to one first shows a warning whose
 * Revert has to be held, like Uninstall in Settings.
 */
export default function Versions() {
  const [releases, setReleases] = useState<Release[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  // the legacy release whose warning is up, and whether Revert is held
  const [warn, setWarn] = useState<Release | null>(null);
  const [holding, setHolding] = useState(false);
  const holdTimer = useRef<number | null>(null);
  const [state, setState] = useState<UpdateState | null>(null);
  const [version, setVersion] = useState("");
  const [open, setOpen] = useState(false);
  const [closing, setClosing] = useState(false);
  const [art] = usePersisted<string | null>(KEYS.art, null);
  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    window.player
      ?.listReleases?.()
      .then((r) => (r.ok ? setReleases(r.list) : setError(r.error)))
      .catch((e) => setError(String(e?.message ?? e)));
    window.player?.updateState?.().then(setState);
    window.player?.appVersion?.().then(setVersion);
    return window.player?.onUpdateChanged?.(setState);
  }, []);

  const busy = state?.status === "downloading";
  const canInstall = !!state && !["checking", "downloading", "dev"].includes(state.status);
  const holdStart = () => {
    if (!warn) return;
    setHolding(true);
    holdTimer.current = window.setTimeout(() => {
      holdTimer.current = null;
      setHolding(false);
      window.player?.bumpTo?.(warn.version);
      setWarn(null);
    }, HOLD_MS);
  };
  const holdEnd = () => {
    if (holdTimer.current == null) return;
    clearTimeout(holdTimer.current);
    holdTimer.current = null;
    setHolding(false);
  };
  const install = (r: Release) => (r.legacy ? setWarn(r) : window.player?.bumpTo?.(r.version));

  const close = () => {
    if (warn) {
      holdEnd();
      setWarn(null);
      return;
    }
    if (closing || busy) return;
    setClosing(true);
    setOpen(false);
    setTimeout(() => window.player?.closeChangelog?.(), CLOSE_MS);
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
      window.player?.resizeChangelog?.(el.getBoundingClientRect().height + STAGE_PAD * 2),
    );
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const stagger = (i: number) => ({ "--cl-i": i }) as React.CSSProperties;
  const text =
    state?.status === "downloading"
      ? `Downloading ${state.version} · ${state.percent}%… player restarts by itself when it's done.`
      : state?.status === "error"
        ? `That didn't work: ${state.error}`
        : state?.status === "dev"
          ? "Only the installed app can switch versions."
          : `You have ${version}. Update or revert to any version; player restarts on it.`;

  return (
    <div className="stage" onContextMenu={(e) => e.preventDefault()}>
      <div
        ref={cardRef}
        className={["cl-card", open && "is-open", closing && "is-closing"].filter(Boolean).join(" ")}
        role="dialog"
        aria-modal="true"
        aria-labelledby="vc-title"
      >
        {art && <div className="cl-bg" style={{ backgroundImage: `url(${art})` }} />}
        <div className="cl-mark">
          <AppMark />
        </div>
        <div className="cl-heading" style={stagger(0)}>
          <p className="cl-brand">player</p>
          <h2 className="cl-title" id="vc-title">
            Version control
          </h2>
        </div>
        <p className="cl-text" style={stagger(1)}>
          {text}
        </p>
        <div style={stagger(2)} className="cl-stagger">
          {error ? (
            <p className="cl-text">Couldn't load the versions: {error}</p>
          ) : !releases ? (
            <p className="cl-text">Loading versions…</p>
          ) : (
            <div className="cl-entries vc-list">
              {releases.map((r) => {
                const cmp = compareVersions(r.version, version);
                const all = sections(r.notes);
                const total = all.reduce((n, s) => n + s.points.length, 0);
                const isOpen = expanded === r.version;
                const more = total > NOTES_PREVIEW || (all[0]?.points[0]?.length ?? 0) > PREVIEW_CHARS;
                return (
                  <section
                    key={r.version}
                    className={["cl-entry vc-entry", cmp === 0 && "is-current", !isOpen && "is-collapsed"]
                      .filter(Boolean)
                      .join(" ")}
                  >
                    <div className="cl-entry-head">
                      <span className="cl-version">{r.version}</span>
                      <span className="cl-date">
                        {new Date(r.date).toLocaleDateString([], { month: "short", day: "numeric" })}
                        {cmp === 0 && " · installed"}
                        {r.legacy && " · no updates"}
                      </span>
                      {cmp !== 0 && (
                        <button
                          type="button"
                          className="cl-button cl-button--quiet vc-go"
                          disabled={!canInstall}
                          onClick={() => install(r)}
                        >
                          {cmp > 0 ? "Update" : "Revert"}
                        </button>
                      )}
                    </div>
                    {(isOpen ? all : firstPoints(all, NOTES_PREVIEW)).map((s, i) => (
                      <div key={i} className={`cl-kind${s.kind ? ` cl-kind--${s.kind}` : ""}`}>
                        {s.name && <div className="cl-kind-name">{s.name}</div>}
                        <ul>
                          {s.points.map((p) => (
                            <li key={p}>{p}</li>
                          ))}
                        </ul>
                      </div>
                    ))}
                    {more && (
                      <button type="button" className="vc-more" onClick={() => setExpanded(isOpen ? null : r.version)}>
                        {isOpen ? "Show less" : "Read more"}
                      </button>
                    )}
                  </section>
                );
              })}
            </div>
          )}
        </div>
        <div className="cl-actions" style={stagger(3)}>
          <button type="button" className="cl-button cl-button--primary" disabled={busy} onClick={close}>
            Done
          </button>
        </div>
        {warn && (
          <div className="vc-warn" role="alertdialog" aria-labelledby="vc-warn-title">
            <div className="vc-warn-box">
              <h3 className="vc-warn-title" id="vc-warn-title">
                Revert to {warn.version}?
              </h3>
              <p className="cl-text">
                {warn.version} is from before player could update itself. Once you're on it, there's no Update, no
                Version control and no way back from inside player: to get a newer version again, download the
                installer from GitHub and run it yourself.
              </p>
              <p className="cl-text">Your settings stay. player closes, and its installer opens it again on {warn.version}.</p>
              <div className="cl-actions">
                <button
                  type="button"
                  className="cl-button cl-button--quiet"
                  onClick={() => {
                    holdEnd();
                    setWarn(null);
                  }}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  className={`cl-button vc-hold${holding ? " holding" : ""}`}
                  style={{ "--hold-ms": `${HOLD_MS}ms` } as React.CSSProperties}
                  onPointerDown={holdStart}
                  onPointerUp={holdEnd}
                  onPointerLeave={holdEnd}
                  onPointerCancel={holdEnd}
                >
                  {holding ? "Keep holding…" : "Hold to revert"}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
