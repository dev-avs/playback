import { useEffect, useRef, useState } from "react";
import { ArrowRight, TriangleAlert } from "lucide-react";
import "./Changelog.css";
import { AppMark } from "./Changelog.tsx";
import { CHANGE_KINDS, FLAVORS, compareVersions, progressText, revisionFlavor, versionLabel, type ChangeKind } from "../changelog";
import { DEFAULTS, KEYS, usePersisted } from "../settings";
import type { Release, UpdateState } from "../types/player";

const FLAVOR_LABELS: Record<string, string> = FLAVORS;
/** this build's flavor (the same page runs in both shells) */
const CURRENT_FLAVOR = "__TAURI_INTERNALS__" in window ? "tauri" : "electron";

/**
 * The flavors Version control offers for a release. A flavor revision
 * (1.6.1-a) is one flavor's build, so only that one: the changelog names it,
 * the release carries only its files, or it is the one flavor whose package
 * is not the same size as in an older release of the same number. A Tauri
 * revision (1.6.3-a) also carries copies of the other flavors' 1.6.3 files,
 * which are not offered; a build older than the revision has no changelog
 * entry for it, so the sizes tell them apart.
 */
function offered(r: Release, list: Release[]): string[] {
  const all = r.flavors ?? [];
  if (!r.version.includes("-")) return all;
  const base = r.version.split("-")[0];
  const copy = (f: string) =>
    !!r.sizes?.[f] &&
    list.some((o) => o.version.split("-")[0] === base && compareVersions(o.version, r.version) < 0 && o.sizes?.[f] === r.sizes?.[f]);
  const rebuilt = all.filter((f) => !copy(f));
  const only = revisionFlavor(r.version) ?? (all.length === 1 ? all[0] : rebuilt.length === 1 ? rebuilt[0] : undefined);
  return only ? all.filter((f) => f === only) : all;
}

function snapshotSettingsJson(): string {
  const out: Record<string, unknown> = {};
  for (const [k, def] of Object.entries(DEFAULTS)) {
    const storageKey = KEYS[k as keyof typeof DEFAULTS];
    try {
      const raw = storageKey ? localStorage.getItem(storageKey) : null;
      out[k] = raw != null ? JSON.parse(raw) : def;
    } catch {
      out[k] = def;
    }
  }
  return JSON.stringify(out, null, 2);
}

// keep in sync with STAGE_PAD in electron/main.cjs
const STAGE_PAD = 22;
const CLOSE_MS = 200;
/** Points of a release's notes shown before Read more, each cut to two lines. */
const NOTES_PREVIEW = 1;
/** About what fits on two lines; a longer first point gets Read more too. */
const PREVIEW_CHARS = 90;
/** How long the warning's Revert has to be held, like Uninstall in Settings. */
const HOLD_MS = 1500;
/** A flavor switch's warning, like the installer's GPUI gate: its button unlocks after this long… */
const SWITCH_WAIT_MS = 5000;
/** …and then has to be held this long. */
const SWITCH_HOLD_MS = 5000;
/** Seconds left, with two decimals (4.37), like the installer's gate. */
const secs = (ms: number) => (Math.max(0, ms) / 1000).toFixed(2);

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
 * The version control window (#versions), from Settings → App → Version
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
  // an install that changes the flavor (Tauri → Electron 1.6.1-a), whose warning is up
  const [switching, setSwitching] = useState<{ r: Release; flavor: string } | null>(null);
  // when the switch's button was pressed (performance.now), or null
  const switchHeld = useRef<number | null>(null);
  // the switch's countdowns as last drawn: ms left to wait, ms held (null when not held)
  const [gate, setGate] = useState<{ left: number; held: number | null }>({ left: SWITCH_WAIT_MS, held: null });
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
  const switchReady = !!switching && gate.left <= 0;
  const holdStart = () => {
    if (switching) {
      if (switchReady) switchHeld.current = performance.now();
      return;
    }
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
    switchHeld.current = null;
    if (holdTimer.current == null) return;
    clearTimeout(holdTimer.current);
    holdTimer.current = null;
    setHolding(false);
  };
  const install = (r: Release, flavor?: string) => {
    if (r.legacy) setWarn(r);
    else if (flavor && flavor !== CURRENT_FLAVOR) {
      switchHeld.current = null;
      setGate({ left: SWITCH_WAIT_MS, held: null });
      setSwitching({ r, flavor });
    }
    else window.player?.bumpTo?.(r.version, flavor, snapshotSettingsJson());
  };
  const cancelWarning = () => {
    holdEnd();
    setWarn(null);
    setSwitching(null);
  };
  // the switch's countdowns: the wait before its button unlocks, then the hold
  useEffect(() => {
    if (!switching) return;
    const at = performance.now();
    let id = 0;
    const step = () => {
      const t = performance.now();
      const held = switchHeld.current == null ? null : t - switchHeld.current;
      if (held != null && held >= SWITCH_HOLD_MS) {
        switchHeld.current = null;
        window.player?.bumpTo?.(switching.r.version, switching.flavor, snapshotSettingsJson());
        setSwitching(null);
        return;
      }
      setGate({ left: SWITCH_WAIT_MS - (t - at), held });
      id = requestAnimationFrame(step);
    };
    id = requestAnimationFrame(step);
    return () => cancelAnimationFrame(id);
  }, [switching]);

  const close = () => {
    if (warn || switching) {
      cancelWarning();
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
      ? `Downloading ${state.version} · ${progressText(state)}… Playback restarts by itself when it's done.`
      : state?.status === "error"
        ? `That didn't work: ${state.error}`
        : state?.status === "dev"
          ? "Only the installed app can switch versions."
          : `You have ${version}. Update or revert to any version; Playback restarts on it.`;

  return (
    <div className="stage" onContextMenu={(e) => e.preventDefault()}>
      <div
        ref={cardRef}
        className={["cl-card", open && "is-open", closing && "is-closing"].filter(Boolean).join(" ")}
        role="dialog"
        aria-modal="true"
        aria-labelledby="vc-title"
      >
        {art ? (
          <div className="cl-bg" style={{ backgroundImage: `url(${art})` }} />
        ) : (
          <div className="cl-bg cl-bg-mono" />
        )}
        <div className="cl-mark">
          <AppMark />
        </div>
        <div className="cl-heading" style={stagger(0)}>
          <p className="cl-brand">Playback</p>
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
                const flavors = offered(r, releases);
                const single = flavors.length === 1 ? flavors[0] : undefined;
                const label = r.version.includes("-") && single ? versionLabel(r.version, single as keyof typeof FLAVORS) : r.version;
                return (
                  <section
                    key={r.version}
                    className={["cl-entry vc-entry", cmp === 0 && "is-current", !isOpen && "is-collapsed"]
                      .filter(Boolean)
                      .join(" ")}
                  >
                    <div className="cl-entry-head">
                      <span className="cl-version">{label}</span>
                      <span className="cl-date">
                        {new Date(r.date).toLocaleDateString([], { month: "short", day: "numeric" })}
                        {cmp === 0 && " · installed"}
                        {r.legacy && " · no updates"}
                      </span>
                      {flavors.length > 1 ? (
                        <div style={{ display: "flex", gap: 4, marginLeft: "auto" }}>
                          {flavors.map((fl) => (
                            <button
                              key={fl}
                              type="button"
                              className="cl-button cl-button--quiet vc-go"
                              disabled={!canInstall}
                              onClick={() => install(r, fl)}
                              title={`Install ${r.version} (${FLAVOR_LABELS[fl] ?? fl}) — keeps all settings`}
                            >
                              {FLAVOR_LABELS[fl] ?? fl}
                            </button>
                          ))}
                        </div>
                      ) : single && single !== CURRENT_FLAVOR ? (
                        <button
                          type="button"
                          className="cl-button cl-button--quiet vc-go"
                          disabled={!canInstall}
                          onClick={() => install(r, single)}
                          title={`Install ${r.version} (${FLAVOR_LABELS[single] ?? single}) — keeps all settings`}
                        >
                          {FLAVOR_LABELS[single] ?? single}
                        </button>
                      ) : (
                        cmp !== 0 && (
                          <button
                            type="button"
                            className="cl-button cl-button--quiet vc-go"
                            disabled={!canInstall}
                            onClick={() => install(r, single)}
                          >
                            {cmp > 0 ? "Update" : "Revert"}
                          </button>
                        )
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
                {warn.version} is from before Playback could update itself (it was still called player then). Once you're on it, there's no Update, no
                Version control and no way back from inside player: to get a newer version again, download the
                installer from GitHub and run it yourself.
              </p>
              <p className="cl-text">It uses your settings as they were when you first opened Playback. Playback closes, and the installer opens player {warn.version}.</p>
              <div className="cl-actions">
                <button type="button" className="cl-button cl-button--quiet" onClick={cancelWarning}>
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
        {switching && (() => {
          const to = FLAVOR_LABELS[switching.flavor] ?? switching.flavor;
          const from = FLAVOR_LABELS[CURRENT_FLAVOR];
          const held = Math.min(gate.held ?? 0, SWITCH_HOLD_MS);
          return (
            <div className="vc-warn" role="alertdialog" aria-labelledby="vc-switch-title">
              <div className="vc-warn-box">
                <div className="vc-switch-head">
                  <span className="vc-switch-icon" aria-hidden="true">
                    <TriangleAlert size={16} strokeWidth={2.2} />
                  </span>
                  <h3 className="vc-warn-title" id="vc-switch-title">
                    Switch to {to}?
                  </h3>
                </div>
                <div className="vc-switch-path" aria-label={`${from} to ${to}`}>
                  <span className="vc-switch-chip">{from}</span>
                  <ArrowRight size={14} strokeWidth={2.4} aria-hidden="true" />
                  <span className="vc-switch-chip vc-switch-chip--to">{to}</span>
                </div>
                <p className="cl-text">
                  Installing {versionLabel(switching.r.version, switching.flavor as keyof typeof FLAVORS)} changes
                  Playback from {from} to {to}. Your settings come along.
                </p>
                {switching.flavor === "gpui" && (
                  <p className="cl-text">
                    GPUI is experimental and very buggy: screens that look different, stuttering or missing animations,
                    freezes and crashes. Not for everyday use.
                  </p>
                )}
                <p className="cl-text">To go back, install {/^[AEIOU]/.test(from) ? "an" : "a"} {from} version from here. Playback closes and restarts on {to}.</p>
                <div className="cl-actions">
                  <button type="button" className="cl-button cl-button--quiet" onClick={cancelWarning}>
                    Cancel
                  </button>
                  <button
                    type="button"
                    className="cl-button vc-gate"
                    style={{ "--p": held / SWITCH_HOLD_MS } as React.CSSProperties}
                    disabled={!switchReady}
                    onPointerDown={holdStart}
                    onPointerUp={holdEnd}
                    onPointerLeave={holdEnd}
                    onPointerCancel={holdEnd}
                  >
                    <span className="vc-gate-fill" />
                    <span className="vc-gate-label">
                      {!switchReady
                        ? `Read this first · ${secs(gate.left)}`
                        : gate.held != null
                          ? secs(SWITCH_HOLD_MS - held)
                          : `Switch to ${to}`}
                    </span>
                  </button>
                </div>
              </div>
            </div>
          );
        })()}
      </div>
    </div>
  );
}
