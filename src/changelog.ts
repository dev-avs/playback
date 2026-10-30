// What changed in each version, newest first, shown by the changelog window
// after an update and from Settings → What's new. Written for the person using
// the widget, not for whoever reads the code: what they will notice.

/** The sections a version's changes are filed under, in the order shown. */
export const CHANGE_KINDS = {
  new: "New",
  appearance: "Appearance",
  qol: "Quality of life",
  performance: "Performance",
  fixes: "Fixes",
} as const;
export type ChangeKind = keyof typeof CHANGE_KINDS;

export interface ChangelogEntry {
  version: string;
  date: string;
  changes: Partial<Record<ChangeKind, string[]>>;
}

export const CHANGELOG: ChangelogEntry[] = [
  {
    version: "1.3.6",
    date: "2026-10-30",
    changes: {
      fixes: [
        "Uninstall in Settings → Behavior only took the bridge out: player itself, its Start menu entry and your saved data stayed. It now removes all of it.",
        "Right-click didn't open Settings while nothing was playing.",
      ],
    },
  },
  {
    version: "1.3.5",
    date: "2026-10-30",
    changes: {
      new: [
        "Uninstall player from Settings → Behavior: click once, then hold to confirm. It removes the app, your settings, Spicy Lyrics key and saved lyrics, and takes the bridge out of Spotify (Spotify restarts once).",
      ],
    },
  },
  {
    version: "1.3.4",
    date: "2026-10-30",
    changes: {
      new: [
        "Your own Spicy Lyrics key for word-by-word lyrics, no Spotify token needed. Get key opens Spicy Lyrics: sign in, create an application (the name and project are filled in for you) and copy the key. player picks it up from the clipboard.",
        "Setup has a Spicy Lyrics key step, and Settings → Behavior lets you get, paste, change or clear the key.",
        "Lyrics from Spicy Lyrics show where they came from under the lyrics, and who made community syncs, with links.",
      ],
      qol: [
        "The Spotify section is gone from Settings; the Spotify token now sits in Behavior, next to the Spicy Lyrics key.",
        "Without a key, or if it stops working, lyrics come through Spotify and the bridge as before.",
      ],
      fixes: ["Saved lyrics are fetched again after 30 days, as Spicy Lyrics' rules ask."],
    },
  },
  {
    version: "1.3.3",
    date: "2026-10-08",
    changes: {
      new: [
        "Setup: a checklist for Spotify, Spicetify and the bridge, with Spicetify's install command to copy and Check again. It opens on the first launch, and later only when something needs you.",
        "On a new PC the first launch sets the bridge up by itself (Spotify restarts once). After that, Setup asks first: Apply now (restarts Spotify).",
      ],
      performance: ["A smaller installer: it carries only what the app runs, and the old development leftovers are gone."],
      qol: [
        "The Spotify companion script is now player-bridge.js. The old player-token.js is removed for you; Spotify needs one restart to load the new one, and Setup offers it.",
        "The changelog waits until Setup is closed, so the two never stack.",
        "The Settings window keeps one size whichever section you open.",
      ],
      fixes: ["A long song title running under the NEXT hint; it now cuts off before it."],
    },
  },
  {
    version: "1.3.2",
    date: "2026-10-07",
    changes: {
      new: [
        "Settings, rebuilt: a sidebar of ten sections, a search box across every setting, the preview pinned at the top, and Reset for one section at a time.",
        "50 new settings across Text, Highlight, Surface, Layout, Album mode, Lyrics and Fun.",
        "Fonts for Japanese, Korean and Chinese lyrics; only fonts you have are offered.",
        "Fun: spinning or floating cover, party background, rainbow and spin speeds.",
      ],
      appearance: [
        "Text: weight, line height, line and letter spacing, italic and title size.",
        "Highlight: strength, glow, sweep direction and edge, and how bright upcoming and sung lines are.",
        "Surface: glass tint, cover blur, colour and brightness, edge highlight and shadow.",
        "Layout: widget width, lyrics height, cover shape, and switches for the cover, artist, times, progress bar, shuffle & repeat and volume.",
        "Album mode: lyric bubble darkness and lyric size.",
        "The changelog is sorted into sections like these.",
      ],
      qol: [
        "Fade the widget while the pointer is away, keep the controls always visible, or open in album mode.",
        "Turn off click-to-jump, line times, instrumental dots, backing vocals or credits; change scroll and follow speed.",
        "The Settings preview shows nearly every look setting, the spinning cover included.",
      ],
      fixes: [
        "The Settings background going black when the song changes; covers now fade into each other.",
        "A gap left in a lyric's hover outline when line times are off.",
        "Settings flickering when the pointer rested on a setting.",
      ],
    },
  },
  {
    version: "1.2.9",
    date: "2026-10-06",
    changes: {
      new: [
        "Fun tab: highlight colour (Pink, Gold, Mint, Sky or Rainbow) and lyric motion (Calm, Normal or Bouncy).",
        "Look settings: centred lyrics, corner roundness, and switches for the Next label, the source label and the album-mode visualizer.",
        "Behavior settings: hide the widget while nothing plays, and song length instead of time left.",
        "Past changelogs: older versions in their own window; What's new shows just the latest.",
      ],
      appearance: [
        "Version numbers read 1.2.9 instead of 1.0.29.",
        "Squarer volume pop-up; the volume percentage uses the same glass.",
        "Every chip and button in Settings, and the Next label, uses SF Pro like the rest (some were Arial).",
        "Settings is wider and in tabs.",
      ],
      qol: [
        "The changelog opens on the first launch of every version (turn it off in Settings → Behavior).",
        "Point at a font, Romanize, Blur, alignment or colour to try it in the preview.",
        "Hold a setting's name for 1 second (was 3) to reset it.",
        "Click the time on the right to switch between time left and the song's length.",
        "Opening the app again brings the running widget forward instead of starting a second one.",
      ],
      performance: ["About half the CPU while playing, with no change in how it looks."],
      fixes: [
        "The changelog window could not be dragged.",
        "Album mode showed \"• • •\" or an empty bubble during instrumental breaks.",
      ],
    },
  },
  {
    version: "1.2.8",
    date: "2026-10-06",
    changes: {
      new: ["This changelog: it opens after each update, and any time from Settings → What's new."],
      qol: ["The Settings preview can show Japanese or Korean lyrics, with a button to see them romanized."],
    },
  },
  {
    version: "1.2.7",
    date: "2026-10-06",
    changes: {
      new: ["Settings has a live preview of the widget, and explains each setting when you point at it."],
      appearance: ["Background goes from 0% (clear) through 72% (the usual glass) to 100% (solid)."],
      qol: ["Hold a setting's name for 3 seconds to reset it. It fills up like a sung syllable while you hold."],
    },
  },
  {
    version: "1.2.6",
    date: "2026-10-06",
    changes: {
      new: ["Background setting for how see-through the widget is."],
      qol: [
        "After you scroll, the lyrics jump back to the current line on their own (3 seconds, changeable in Settings).",
        "Scrolling stops once the end of the lyrics reaches the middle, instead of scrolling to an empty panel.",
      ],
    },
  },
  {
    version: "1.2.5",
    date: "2026-10-06",
    changes: {
      fixes: ["The mouse wheel did nothing when lyrics were open at launch, or after album mode."],
    },
  },
  {
    version: "1.2.4",
    date: "2026-10-06",
    changes: {
      qol: ["Scrolling the lyrics glides instead of jumping."],
      performance: ["The next song's lyrics and cover are loaded ahead of time, so they show the moment it starts."],
    },
  },
  {
    version: "1.2.3",
    date: "2026-10-06",
    changes: { appearance: ["New app icon."] },
  },
  {
    version: "1.2.2",
    date: "2026-10-06",
    changes: {
      appearance: [
        "The volume icon shows the level: an ✕ when muted, then one, two or three waves.",
        "The album-mode visualizer is rounded bars that glide with the music.",
      ],
      qol: ["The あ romanize button is back next to Aa, on songs that have something to romanize."],
    },
  },
  {
    version: "1.2.1",
    date: "2026-10-06",
    changes: {
      appearance: [
        "The volume pop-up takes the song's colours, like Settings, and its bar matches the progress bar.",
        "The volume percentage on hover is bigger.",
        "With nothing playing, the widget shows soft placeholder lines and no lyrics panel.",
      ],
    },
  },
  {
    version: "1.2.0",
    date: "2026-10-05",
    changes: {
      new: [
        "Word-by-word lyrics for Apple Music, browsers and other players, not just Spotify. Spotify briefly opens hidden to look the song up.",
      ],
    },
  },
  {
    version: "1.1.9",
    date: "2026-10-05",
    changes: {
      appearance: ["Redesigned volume pop-up."],
      qol: ["You can no longer scroll the lyrics past their start."],
      fixes: [
        "A crash when closing the widget from the taskbar.",
        "The bottom of the widget getting cut off after minimizing or dragging.",
        "Gaps inside words like \"Tues day\" on the line being sung.",
        "Clicking a lyric ignored the lyric offset.",
      ],
    },
  },
  {
    version: "1.1.8",
    date: "2026-10-05",
    changes: {
      appearance: ["With nothing playing, the widget keeps the player's shape instead of showing a message."],
      qol: ["Songs without lyrics keep the lyrics panel closed, and the Aa button greys out."],
    },
  },
  {
    version: "1.1.7",
    date: "2026-10-05",
    changes: {
      appearance: ["\"Nothing playing\" and \"No lyrics found\" use the same font as the rest of the widget."],
    },
  },
  {
    version: "1.1.6",
    date: "2026-10-05",
    changes: { appearance: ["Undid 1.1.5's look changes."] },
  },
];

/** -1, 0 or 1, comparing dotted version numbers part by part. */
export function compareVersions(a: string, b: string): number {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return Math.sign(d);
  }
  return 0;
}
