export interface PlayerCaps {
  play: boolean;
  pause: boolean;
  next: boolean;
  prev: boolean;
  seek: boolean;
  shuffle: boolean;
  repeat: boolean;
}

export interface SmtcState {
  type: "state";
  appId?: string;
  title?: string;
  artist?: string;
  album?: string;
  trackKey?: string;
  status?: string;
  playing?: boolean;
  position?: number;
  duration?: number;
  updatedAt?: number;
  sampledAt?: number;
  shuffle?: boolean;
  repeat?: "None" | "Track" | "List" | string;
  /** app audio-session volume 0..1, or -1 when no session was found */
  volume?: number;
  caps?: PlayerCaps;
  session?: null;
}

export interface SmtcError {
  type: "error";
  message: string;
}

export type SmtcEvent = SmtcState | SmtcError;

export interface LyricsRequest {
  artist: string;
  title: string;
  album?: string;
  duration?: number;
  geniusKey?: string;
  /** ask just this provider instead of the whole chain */
  only?: string;
}

export interface LyricsResponse {
  /** NetEase `yrc` — word-by-word karaoke timing */
  wordLyrics?: string;
  syncedLyrics?: string;
  plainLyrics?: string;
  source: "lrclib" | "netease" | "genius";
}

/** electron/updater.cjs */
export interface UpdateState {
  status: "idle" | "checking" | "none" | "available" | "downloading" | "error" | "dev";
  /** the newest version on GitHub, once checked */
  version: string | null;
  /** download progress, 0..100 */
  /** bytes downloaded so far */
  downloaded: number;
  /** the whole download's size, when known (only for old versions' full installers) */
  total: number | null;
  error: string | null;
}

/** A GitHub release, as electron/updater.cjs lists it for version control. */
export interface Release {
  version: string;
  /** ISO time it was published */
  date: string;
  /** the release body: `### Section` headings and `- ` points */
  notes: string;
  /** before 1.3.7: installed with its full installer, and has no updater of its own */
  legacy: boolean;
}

/** Spicetify and the bridge, as electron/spicetifySetup.cjs reports them. */
export interface SetupState {
  firstRun: boolean;
  ready: boolean;
  needsApply: boolean;
  actions: string[];
  error?: string;
  status: {
    spotify: boolean;
    spicetify: boolean;
    bridgeCopied: boolean;
    registered: boolean;
    applied: boolean;
    appliedCurrent: boolean;
    legacy: string[];
  };
}

export interface PlayerBridge {
  onEvent(cb: (msg: SmtcEvent) => void): () => void;
  /** the Spicetify extension delivered a Spotify token — see tokenBridge.cjs */
  onTokenArrived?(cb: () => void): () => void;
  play(): Promise<boolean>;
  pause(): Promise<boolean>;
  playPause(): Promise<boolean>;
  next(): Promise<boolean>;
  prev(): Promise<boolean>;
  seek(seconds: number): Promise<boolean>;
  setVolume(level: number): Promise<boolean>;
  shuffle(): Promise<boolean>;
  repeat(): Promise<boolean>;
  fetchLyrics(req: LyricsRequest): Promise<LyricsResponse | null>;
  fetchArt(req: { artist: string; title: string; album?: string }): Promise<string | null>;
  /** romanized LRC for the track, or null when the source has none */
  fetchRomanized(req: { artist: string; title: string }): Promise<string | null>;

  /** Spicy Lyrics payload, still binary-packed; unpacked in src/spicy.ts */
  fetchSpicyLyrics(req: {
    artist: string;
    title: string;
    album?: string;
    duration?: number;
  }): Promise<{
    status: "ok" | "no-auth" | "no-track" | "not-found" | "queued" | "error" | "disabled";
    packed?: unknown;
    /**
     * Already-unpacked lyrics, handed over from the Spicy Lyrics client's own
     * cache by the Spicetify extension. Present instead of `packed`.
     */
    lyrics?: unknown;
    trackId?: string;
    httpStatus?: number;
  } | null>;

  /**
   * The Spicetify extension handed over lyrics Spicy Lyrics had cached, so the
   * current track may now have a finer source than the one on screen.
   */
  onSpicyLyrics?(cb: () => void): () => void;

  /** Updates from GitHub releases; "dev" in the dev build, which never updates. */
  updateState?(): Promise<UpdateState>;
  checkForUpdate?(): Promise<UpdateState>;
  /** Downloads and restarts into the new version, no confirmation. */
  installUpdate?(): Promise<UpdateState>;
  /** Update window → Skip this version: launch stops asking about it. */
  skipUpdate?(): Promise<void>;
  /** Releases version control can install (1.3.7 on), newest first. */
  listReleases?(): Promise<{ ok: true; list: Release[] } | { ok: false; error: string }>;
  /** Installs that release, newer or older, and restarts into it. */
  bumpTo?(version: string): Promise<UpdateState>;
  /** Opens the version control window (#versions). */
  openVersions?(): Promise<void>;
  onUpdateChanged?(cb: (state: UpdateState) => void): () => void;
  closeUpdate?(): Promise<void>;
  resizeUpdate?(height: number): Promise<void>;

  /** Removes the bridge, the app and its data, then quits; not ok in the dev build. */
  uninstall?(): Promise<{ ok: boolean; error?: string }>;

  /** The user's Spicy Lyrics API key: whether one is saved, and its last four characters. */
  spicyKeyStatus?(): Promise<{ set: boolean; ending: string | null }>;
  /** Opens the Spicy Lyrics catalog to get a key; ok once one is checked and saved. */
  connectSpicyKey?(): Promise<{ ok: boolean; error?: string }>;
  setSpicyKey?(raw: string): Promise<{ ok: boolean; error?: string }>;
  clearSpicyKey?(): Promise<void>;
  onSpicyKeyChanged?(cb: () => void): () => void;

  /** The Spicetify extension reported a new track; Spotify's cover is now known. */
  onBridgedTrack?(cb: () => void): () => void;

  spotifyStatus(): Promise<{
    configured: boolean;
    signedIn: boolean;
    redirectUri: string;
    /** epoch ms at which a hand-pasted token expires, if known */
    tokenExpiresAt: number | null;
  }>;
  spotifyLogin(): Promise<{ ok: boolean; error?: string }>;
  spotifySetToken(
    token: string,
  ): Promise<{ ok: boolean; error?: string; expiresAt?: number | null }>;
  spotifySignOut(): Promise<void>;
  /**
   * Begin/end a window drag. The main process follows the OS cursor while a
   * drag is active, so no pointer position is ever sent across.
   */
  dragStart(): Promise<void>;
  dragEnd(): Promise<void>;
  resizeTo(height: number, hovered?: boolean): Promise<void>;
  setAlwaysOnTop(on: boolean): Promise<void>;

  toggleSettings?(): Promise<void>;
  openSettings?(): Promise<void>;
  closeSettings?(): Promise<void>;
  resizeSettings?(height: number): Promise<void>;

  // The changelog window.
  appVersion?(): Promise<string>;
  openChangelog?(): Promise<void>;
  openPastChangelog?(): Promise<void>;
  /** at launch: opens the changelog if this version is new and `enabled` */
  checkChangelog?(enabled: boolean): Promise<void>;
  setWidgetHidden?(hidden: boolean): Promise<void>;
  setWidgetWidth?(width: number): Promise<void>;
  closeChangelog?(): Promise<void>;
  resizeChangelog?(height: number): Promise<void>;
  // The Setup window.
  /** where Spotify, Spicetify and the bridge stand, once the launch check is done */
  setupState?(): Promise<SetupState>;
  /** checks again (Check again); never restarts Spotify */
  recheckSetup?(): Promise<SetupState>;
  /** sets up and runs `spicetify apply`, which restarts Spotify */
  applySetup?(): Promise<SetupState>;
  copyText?(text: string): Promise<void>;
  /** opens an https link in the browser */
  openLink?(url: string): Promise<void>;
  closeSetup?(): Promise<void>;
  resizeSetup?(height: number): Promise<void>;
  /** installed font family names */
  listFonts?(): Promise<string[]>;
  /** next track in Spotify's queue (via the Spicetify extension), or null */
  getNext?(req: { title: string }): Promise<{ id: string; name: string; artist: string; image: string | null } | null>;
  minimize(): Promise<void>;
  close(): Promise<void>;
}

declare global {
  interface Window {
    player?: PlayerBridge;
  }
}
