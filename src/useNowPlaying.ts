import { useCallback, useEffect, useRef, useState } from "react";
import type { PlayerCaps, SmtcEvent, SmtcState } from "./player";

export interface NowPlaying {
  appId: string;
  title: string;
  artist: string;
  album: string;
  trackKey: string;
  playing: boolean;
  duration: number;
  shuffle: boolean;
  repeat: string;
  caps: PlayerCaps;
  art: string | null;
  /** 0..1, or null when the app has no addressable audio session */
  volume: number | null;
}

const NO_CAPS: PlayerCaps = {
  play: false,
  pause: false,
  next: false,
  prev: false,
  seek: false,
  shuffle: false,
  repeat: false,
};

const EMPTY: NowPlaying = {
  appId: "",
  title: "",
  artist: "",
  album: "",
  trackKey: "",
  playing: false,
  duration: 0,
  shuffle: false,
  repeat: "None",
  caps: NO_CAPS,
  art: null,
  volume: null,
};

// Friendly labels for the app IDs Windows reports.
const APP_NAMES: Record<string, string> = {
  "spotify.exe": "Spotify",
  "applemusic.exe": "Apple Music",
  "apple.music": "Apple Music",
  "msedge.exe": "Edge",
  "chrome.exe": "Chrome",
  "firefox.exe": "Firefox",
  "vlc.exe": "VLC",
  "foobar2000.exe": "foobar2000",
  "tidal.exe": "TIDAL",
  "deezer.exe": "Deezer",
};

export function appLabel(appId: string): string {
  if (!appId) return "";
  const key = appId.toLowerCase();
  for (const [k, v] of Object.entries(APP_NAMES)) {
    if (key.includes(k)) return v;
  }
  // AUMIDs look like "AppleInc.AppleMusicWin_nzyj5cx40ttqa!App" — take the
  // publisher-qualified name and strip the packaging noise.
  const base = appId.split("!")[0].split("_")[0].replace(/^.*\./, "");
  return base || appId;
}

/**
 * Live now-playing state from Windows SMTC.
 *
 * Position is deliberately *not* React state: SMTC only emits on real changes
 * (track/status/seek plus a 5 s heartbeat), so the smooth clock is derived from
 * an anchor and read imperatively by the caller's rAF loop.
 */
/** How long a clicked shuffle/repeat state is shown before SMTC must agree. */
const MODES_HOLD_MS = 3000;

export function useNowPlaying() {
  const [np, setNp] = useState<NowPlaying>(EMPTY);
  const [connected, setConnected] = useState(false);

  // Predicted clock: advances with wall time; SMTC updates fold in gently
  // unless the delta is big enough to look like a seek/track change.
  //
  // Position (s) at `anchorAt` (performance-clock ms). Snap deltas larger than
  // JITTER_SNAP snap immediately (real seeks); smaller deltas are treated as
  // reporting jitter and folded in without a visible jump.
  const JITTER_SNAP = 0.5; // seconds
  const anchorPos = useRef(0);
  const anchorAt = useRef(0);
  const playingRef = useRef(false);
  const durationRef = useRef(0);
  const volumeSetAt = useRef(0);
  // Shuffle/repeat as just clicked, shown until SMTC reports the same (or
  // MODES_HOLD_MS passes, and then SMTC's word stands: verified, not assumed).
  const expectedModes = useRef<{ shuffle: boolean; repeat: string; at: number } | null>(null);
  const prevTrackRef = useRef<string>("");

  const predictedNow = useCallback(() => {
    const base = anchorPos.current;
    if (!playingRef.current) return base;
    return base + (performance.now() - anchorAt.current) / 1000;
  }, []);

  /** Hard-set the clock. For seeks, track changes and first sample. */
  const snapAnchor = useCallback((pos: number) => {
    anchorPos.current = pos;
    anchorAt.current = performance.now();
  }, []);

  /**
   * Fold a fresh SMTC sample into the predicted clock.
   *
   * SMTC emits on real changes plus a ~5 s heartbeat, and the reported
   * position isn't perfectly accurate against `updatedAt` for every app. So
   * each sample carries some measurement noise; snapping the anchor to raw
   * measured values reprints the noise as visible jumps in the lyric clock —
   * "sometimes ahead, sometimes behind".
   *
   *   - big delta (>= JITTER_SNAP) → real seek/track change/large stall; snap.
   *   - small delta → low-pass toward measured. α is chosen so ~3 heartbeats
   *     bring the clock back within a few ms even if a heartbeat is skipped.
   *
   * Deliberately a per-sample constant rather than spicy-lyrics' frame-rate
   * independent `1 - exp(-elapsed / 300)`. That form assumes their polling
   * model, where the position is re-read every frame and the correction is a
   * small continuous pull. We are pushed a sample roughly every 5 s, and at
   * that spacing the exponential resolves to ~1.0 — i.e. a hard snap that
   * reprints every bit of measurement noise, which is the exact thing this
   * filter exists to suppress.
   */
  const FOLD_ALPHA = 0.4;
  const foldAnchor = useCallback(
    (measured: number) => {
      const now = performance.now();
      if (!playingRef.current) {
        anchorPos.current = measured;
        anchorAt.current = now;
        return;
      }
      const predicted =
        anchorPos.current + (now - anchorAt.current) / 1000;
      const delta = measured - predicted;
      if (Math.abs(delta) >= JITTER_SNAP) {
        anchorPos.current = measured;
      } else {
        anchorPos.current = predicted + delta * FOLD_ALPHA;
      }
      anchorAt.current = now;
    },
    [],
  );

  const getPosition = useCallback(() => {
    const pos = predictedNow();
    const dur = durationRef.current;
    return dur > 0 ? Math.min(pos, dur) : pos;
  }, [predictedNow]);

  useEffect(() => {
    const bridge = window.player;
    if (!bridge) return;

    const clear = () => {
      playingRef.current = false;
      durationRef.current = 0;
      prevTrackRef.current = "";
      snapAnchor(0);
      setNp(EMPTY);
    };
    // A track change reports an empty title for about half a second while the
    // session stays open. Clearing on it flashed the "Nothing playing" card
    // (and dropped fullscreen's layout for that moment), so the old track stays
    // until the next one arrives, unless the title stays empty.
    let clearTimer: ReturnType<typeof setTimeout> | undefined;

    const off = bridge.onEvent((msg: SmtcEvent) => {
      if (msg.type === "error") {
        console.error("[smtc]", msg.message);
        return;
      }

      const s = msg as SmtcState;
      setConnected(true);

      if (s.session === null) {
        clearTimeout(clearTimer);
        clearTimer = undefined;
        clear();
        return;
      }
      if (!s.title) {
        clearTimer ??= setTimeout(clear, 1500);
        return;
      }
      clearTimeout(clearTimer);
      clearTimer = undefined;

      // `position` is only valid as of `updatedAt`, and players like Spotify
      // refresh their timeline just every few seconds. Measure the staleness
      // against *this* moment rather than against the script's sample time:
      // both clocks are the same system clock, so doing it here also cancels
      // out the PowerShell->IPC pipeline delay. Measured accurate to ~1 ms.
      const receivedAt = Date.now();
      const updatedAt = s.updatedAt ?? s.sampledAt ?? receivedAt;
      const staleMs = receivedAt - updatedAt;
      const lead = staleMs > 0 && staleMs < 30_000 ? staleMs / 1000 : 0;

      playingRef.current = !!s.playing;
      durationRef.current = s.duration ?? 0;
      const measured = (s.position ?? 0) + (s.playing ? lead : 0);
      const trackKey = s.trackKey ?? "";
      // Track changes and paused-state samples reset the clock; only mid-track
      // heartbeats get the low-pass fold so we don't reprint measurement noise.
      if (trackKey !== prevTrackRef.current || !s.playing) {
        snapAnchor(measured);
      } else {
        foldAnchor(measured);
      }
      prevTrackRef.current = trackKey;

      // A poll can be in flight when the user drags the slider; ignore
      // reported volume briefly after a local change so it does not snap back.
      const vol = typeof s.volume === "number" && s.volume >= 0 ? s.volume : null;
      const fresh = performance.now() - volumeSetAt.current > 1500;

      let shuffle = !!s.shuffle;
      let repeat = s.repeat ?? "None";
      const exp = expectedModes.current;
      if (exp && performance.now() - exp.at < MODES_HOLD_MS && (exp.shuffle !== shuffle || exp.repeat !== repeat)) {
        shuffle = exp.shuffle;
        repeat = exp.repeat;
      } else {
        expectedModes.current = null;
      }

      setNp((prev) => ({
        appId: s.appId ?? "",
        title: s.title ?? "",
        artist: s.artist ?? "",
        album: s.album ?? "",
        trackKey: s.trackKey ?? "",
        playing: !!s.playing,
        duration: s.duration ?? 0,
        shuffle,
        repeat,
        caps: s.caps ?? NO_CAPS,
        // keep existing art until the new track's art arrives
        art: prev.trackKey === s.trackKey ? prev.art : null,
        volume: fresh ? vol : prev.volume,
      }));
    });

    return () => {
      clearTimeout(clearTimer);
      off();
    };
  }, [foldAnchor, snapAnchor]);

  // Optimistic seek: move the local anchor now so the scrubber does not snap
  // back during the up-to-250 ms round trip to SMTC.
  const seek = useCallback(
    (seconds: number) => {
      const clamped = Math.max(0, Math.min(seconds, durationRef.current || seconds));
      snapAnchor(clamped);
      window.player?.seek(clamped);
    },
    [snapAnchor],
  );

  const togglePlay = useCallback(() => {
    // Reflect the flip immediately; the next SMTC emit confirms it.
    snapAnchor(getPosition());
    playingRef.current = !playingRef.current;
    setNp((prev) => ({ ...prev, playing: !prev.playing }));
    window.player?.playPause();
  }, [getPosition, snapAnchor]);

  // Look again once Spotify reports the track: a lookup made before that could
  // only use iTunes/Deezer.
  const [artNonce, setArtNonce] = useState(0);
  useEffect(() => window.player?.onBridgedTrack?.(() => setArtNonce((n) => n + 1)), []);

  // Album art is looked up over HTTP by the main process, keyed on the track.
  useEffect(() => {
    if (!np.trackKey || !window.player) return;
    let cancelled = false;
    const key = np.trackKey;
    window.player
      .fetchArt({ artist: np.artist, title: np.title, album: np.album, appId: np.appId })
      .then((url) => {
        if (cancelled || !url) return;
        setNp((prev) => (prev.trackKey === key ? { ...prev, art: url } : prev));
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
    // artist/title/album are constant for a given trackKey
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [np.trackKey, artNonce]);

  const setVolume = useCallback((level: number) => {
    const v = Math.max(0, Math.min(1, level));
    volumeSetAt.current = performance.now();
    setNp((prev) => ({ ...prev, volume: v }));
    window.player?.setVolume(v);
  }, []);

  // Optimistic shuffle/repeat: the button flips on click, the next SMTC emit
  // confirms it (see expectedModes).
  const toggleShuffle = useCallback(() => {
    setNp((prev) => {
      expectedModes.current = { shuffle: !prev.shuffle, repeat: prev.repeat, at: performance.now() };
      return { ...prev, shuffle: !prev.shuffle };
    });
    window.player?.shuffle();
  }, []);

  // Same order as smtc.ps1's 'repeat' command: None → List → Track → None
  const cycleRepeat = useCallback(() => {
    setNp((prev) => {
      const repeat = prev.repeat === "None" ? "List" : prev.repeat === "List" ? "Track" : "None";
      expectedModes.current = { shuffle: prev.shuffle, repeat, at: performance.now() };
      return { ...prev, repeat };
    });
    window.player?.repeat();
  }, []);

  return { np, connected, getPosition, seek, togglePlay, setVolume, toggleShuffle, cycleRepeat };
}
