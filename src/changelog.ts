// What changed in each version, newest first, shown by the changelog window
// after an update and from Settings → What's new. Written for the person using
// the widget, not for whoever reads the code: what they will notice.

export interface ChangelogEntry {
  version: string;
  date: string;
  changes: string[];
}

export const CHANGELOG: ChangelogEntry[] = [
  {
    version: "1.3.0",
    date: "2026-10-06",
    changes: [
      "Settings, rebuilt: a sidebar of ten sections, a search box across every setting, the preview pinned at the top, and Reset for one section at a time.",
      "50 new settings. Text: weight, line height, line and letter spacing, italic, title size, and fonts for Japanese, Korean and Chinese lyrics.",
      "Highlight: strength, glow, sweep direction and edge, and how bright upcoming and sung lines are.",
      "Surface: glass tint, cover blur, colour and brightness, edge highlight, shadow, and fading while the pointer is away.",
      "Layout: widget width, lyrics height, cover shape, and switches for the cover, artist, times, progress bar, shuffle & repeat, volume and always-visible controls.",
      "Album mode: open in it on launch, lyric bubble darkness and lyric size. Fun: spinning or floating cover, party background, rainbow and spin speeds, ALL CAPS.",
      "Lyrics: turn off click-to-jump, line times, instrumental dots, backing vocals or credits; change scroll and follow speed.",
      "The preview now shows nearly every look setting, the spinning cover included, and only fonts you have are offered for each language.",
      "Fixed the Settings background going black when the song changes; covers now fade into each other.",
    ],
  },
  {
    version: "1.2.9",
    date: "2026-10-06",
    changes: [
      "Version numbers now read 1.2.9 instead of 1.0.29.",
      "What's new shows just the latest update; Past changelogs opens the older ones in their own window.",
      "The changelog opens on the first launch of every version (turn it off in Settings → Behavior).",
      "New Fun tab: highlight colour (Pink, Gold, Mint, Sky or Rainbow) and lyric motion (Calm, Normal or Bouncy).",
      "New Look settings: centred lyrics, corner roundness, and switches for the Next label, the source label and the album-mode visualizer.",
      "New Behavior settings: hide the widget while nothing plays, and song length instead of time left.",
      "Settings is wider and in tabs; point at a font, Romanize, Blur, alignment or colour to try it in the preview.",
      "Hold a setting's name for 1 second (was 3) to reset it.",
      "Click the time on the right to switch between time left and the song's length.",
      "Squarer volume pop-up; the volume percentage uses the same glass.",
      "The changelog window can be dragged.",
      "Every chip and button in Settings, and the Next label, now uses SF Pro like the rest (some were Arial).",
      "Album mode no longer shows \"• • •\" or an empty bubble during instrumental breaks.",
      "Opening the app again brings the running widget forward instead of starting a second one.",
      "Uses about half the CPU while playing, with no change in how it looks.",
    ],
  },
  {
    version: "1.2.8",
    date: "2026-10-06",
    changes: [
      "This changelog: it opens after each update, and any time from Settings → What's new.",
      "The Settings preview can show Japanese or Korean lyrics, with a button to see them romanized.",
    ],
  },
  {
    version: "1.2.7",
    date: "2026-10-06",
    changes: [
      "Background goes from 0% (clear) through 72% (the usual glass) to 100% (solid).",
      "Hold a setting's name for 3 seconds to reset it. It fills up like a sung syllable while you hold.",
      "Settings has a live preview of the widget, and explains each setting when you point at it.",
    ],
  },
  {
    version: "1.2.6",
    date: "2026-10-06",
    changes: [
      "Scrolling stops once the end of the lyrics reaches the middle, instead of scrolling to an empty panel.",
      "After you scroll, the lyrics jump back to the current line on their own (3 seconds, changeable in Settings).",
      "New Background setting for how see-through the widget is.",
    ],
  },
  {
    version: "1.2.5",
    date: "2026-10-06",
    changes: [
      "Fixed the mouse wheel doing nothing when lyrics were open at launch, or after album mode.",
    ],
  },
  {
    version: "1.2.4",
    date: "2026-10-06",
    changes: [
      "The next song's lyrics and cover are loaded ahead of time, so they show the moment it starts.",
      "Scrolling the lyrics glides instead of jumping.",
    ],
  },
  {
    version: "1.2.3",
    date: "2026-10-06",
    changes: ["New app icon."],
  },
  {
    version: "1.2.2",
    date: "2026-10-06",
    changes: [
      "The あ romanize button is back next to Aa, on songs that have something to romanize.",
      "The volume icon shows the level: an ✕ when muted, then one, two or three waves.",
      "The album-mode visualizer is now rounded bars that glide with the music.",
    ],
  },
  {
    version: "1.2.1",
    date: "2026-10-06",
    changes: [
      "The volume pop-up takes the song's colours, like Settings, and its bar matches the progress bar.",
      "The volume percentage on hover is bigger.",
      "With nothing playing, the widget shows soft placeholder lines and no lyrics panel.",
    ],
  },
  {
    version: "1.2.0",
    date: "2026-10-05",
    changes: [
      "Word-by-word lyrics for Apple Music, browsers and other players, not just Spotify. Spotify briefly opens hidden to look the song up.",
    ],
  },
  {
    version: "1.1.9",
    date: "2026-10-05",
    changes: [
      "Fixed a crash when closing the widget from the taskbar.",
      "Fixed the bottom of the widget getting cut off after minimizing or dragging.",
      "Fixed gaps inside words like \"Tues day\" on the line being sung.",
      "Clicking a lyric now respects the lyric offset.",
      "You can no longer scroll the lyrics past their start.",
      "Redesigned volume pop-up.",
    ],
  },
  {
    version: "1.1.8",
    date: "2026-10-05",
    changes: [
      "With nothing playing, the widget keeps the player's shape instead of showing a message.",
      "Songs without lyrics keep the lyrics panel closed, and the Aa button greys out.",
    ],
  },
  {
    version: "1.1.7",
    date: "2026-10-05",
    changes: ["\"Nothing playing\" and \"No lyrics found\" use the same font as the rest of the widget."],
  },
  {
    version: "1.1.6",
    date: "2026-10-05",
    changes: ["Undid 1.1.5's look changes."],
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
