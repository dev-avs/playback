// macOS now-playing helper: the Mac counterpart of electron/smtc.ps1.
//
// Same protocol (see electron/mediaHelper.cjs):
//   stdout : one JSON object per line ({"type":"state"|"error", ...})
//   stdin  : one command per line (play|pause|playpause|next|prev|seek <sec>|
//            volume <0..1>|shuffle|repeat; hide-spotify is Windows-only, ignored)
//
// Sources: Spotify and Apple Music, read and controlled through AppleScript.
// It never starts either app: a script only runs when the app is already in
// NSWorkspace's running applications, and the script itself checks
// `is running` again before its `tell` block.
//
// Build: npm run build:mac-helper (scripts/build-mac-helper.sh).
//
// UNVERIFIED: written without a Mac and never compiled. Check on a real Mac
// (mac-port.md, "Checklist for the friend's Mac").

import AppKit
import Foundation

// ── Output ─────────────────────────────────────────────────────

func emit(_ obj: [String: Any]) {
  guard let data = try? JSONSerialization.data(withJSONObject: obj) else { return }
  FileHandle.standardOutput.write(data)
  FileHandle.standardOutput.write("\n".data(using: .utf8)!)
}

// Errors repeat every poll (e.g. Automation permission denied); report each
// distinct message once so stdout is not flooded.
var lastError = ""
func emitError(_ message: String) {
  if message == lastError { return }
  lastError = message
  emit(["type": "error", "message": message])
}

func nowMs() -> Double { Date().timeIntervalSince1970 * 1000 }

// ── Sources ────────────────────────────────────────────────────

struct Track {
  var state: String  // AppleScript player state: playing, paused, stopped, ...
  var title = "", artist = "", album = ""
  var duration = 0.0  // seconds
  var position = 0.0  // seconds
  var shuffle = false
  var repeatMode = "None"  // None | List | Track, as smtc.ps1 reports it
  var volume = -1.0  // 0..1
  var spotifyUri = "", artUrl = ""
}

struct Source {
  let bundleId: String
  // What the renderer sees. "spotify.exe" keeps the existing Spotify checks
  // (main.cjs isSpotifyPlaying, appLabel) working unchanged; "com.apple.Music"
  // is labelled "Apple Music" by appLabel ("apple.music").
  let appId: String
  let readScript: String
  var compiled: NSAppleScript? = nil

  var isRunning: Bool {
    NSWorkspace.shared.runningApplications.contains { $0.bundleIdentifier == bundleId }
  }
}

// Each read script returns a list in this order:
// {state, name, artist, album, duration (s), position (s), shuffle (bool),
//  repeat (None|List|Track), volume (0..100), spotify uri, artwork url}
// or {state} alone when nothing is loaded.
// Spotify reports duration in ms and has a plain on/off `repeating`.
let spotifyRead = """
  if application id "com.spotify.client" is running then
    tell application id "com.spotify.client"
      set st to player state as text
      if st is "stopped" then return {st}
      set t to current track
      if repeating then
        set rp to "List"
      else
        set rp to "None"
      end if
      return {st, name of t, artist of t, album of t, (duration of t) / 1000, player position, shuffling, rp, sound volume, id of t, artwork url of t}
    end tell
  end if
  return {"stopped"}
  """

// Music: duration in seconds, `song repeat` is off/one/all. `current track`
// errors when nothing is loaded (or for some radio streams), hence the try.
let musicRead = """
  if application id "com.apple.Music" is running then
    tell application id "com.apple.Music"
      set st to player state as text
      if st is "stopped" then return {st}
      try
        set t to current track
      on error
        return {"stopped"}
      end try
      set r to song repeat as text
      if r is "one" then
        set rp to "Track"
      else if r is "all" then
        set rp to "List"
      else
        set rp to "None"
      end if
      return {st, name of t, artist of t, album of t, duration of t, player position, shuffle enabled, rp, sound volume, "", ""}
    end tell
  end if
  return {"stopped"}
  """

var sources = [
  Source(bundleId: "com.spotify.client", appId: "spotify.exe", readScript: spotifyRead),
  Source(bundleId: "com.apple.Music", appId: "com.apple.Music", readScript: musicRead),
]

// Run AppleScript source; nil (and an error line) on failure.
// -1743 = the user denied Automation for this app; -600 = app quit meanwhile.
func run(_ source: String) -> NSAppleEventDescriptor? {
  var err: NSDictionary?
  let result = NSAppleScript(source: source)?.executeAndReturnError(&err)
  if let err = err {
    emitError("applescript: \(err[NSAppleScript.errorMessage] ?? "") (\(err[NSAppleScript.errorNumber] ?? ""))")
    return nil
  }
  return result
}

func readSource(_ i: Int) -> Track? {
  guard sources[i].isRunning else { return nil }
  // Compile once, only after the app is known to be running (compiling a
  // `tell` could otherwise look the app up; unverified whether it launches it).
  if sources[i].compiled == nil {
    let s = NSAppleScript(source: sources[i].readScript)
    var err: NSDictionary?
    if s?.compileAndReturnError(&err) != true {
      emitError("applescript compile: \(err?[NSAppleScript.errorMessage] ?? "")")
      return nil
    }
    sources[i].compiled = s
  }
  var err: NSDictionary?
  guard let d = sources[i].compiled?.executeAndReturnError(&err), err == nil else {
    emitError("applescript: \(err?[NSAppleScript.errorMessage] ?? "") (\(err?[NSAppleScript.errorNumber] ?? ""))")
    return nil
  }
  lastError = ""
  // AppleScript lists are 1-based.
  var t = Track(state: d.atIndex(1)?.stringValue ?? "stopped")
  if d.numberOfItems < 11 { return t }
  t.title = d.atIndex(2)?.stringValue ?? ""
  t.artist = d.atIndex(3)?.stringValue ?? ""
  t.album = d.atIndex(4)?.stringValue ?? ""
  t.duration = d.atIndex(5)?.doubleValue ?? 0
  t.position = d.atIndex(6)?.doubleValue ?? 0
  t.shuffle = d.atIndex(7)?.booleanValue ?? false
  t.repeatMode = d.atIndex(8)?.stringValue ?? "None"
  if let v = d.atIndex(9)?.doubleValue { t.volume = v / 100 }
  t.spotifyUri = d.atIndex(10)?.stringValue ?? ""
  t.artUrl = d.atIndex(11)?.stringValue ?? ""
  return t
}

// ── Picking the source ─────────────────────────────────────────

var lastPlayed = 0  // index into sources: the one that played most recently
var current: Int? = nil  // the source commands go to

func pick() -> (Int, Track)? {
  var loaded: [(Int, Track)] = []
  for i in sources.indices {
    if let t = readSource(i), t.state != "stopped", !t.title.isEmpty { loaded.append((i, t)) }
  }
  // Prefer the one playing; otherwise the last one that played.
  if let p = loaded.first(where: { $0.1.state == "playing" }) {
    lastPlayed = p.0
    return p
  }
  return loaded.first(where: { $0.0 == lastPlayed }) ?? loaded.first
}

// ── Polling and emitting (same rules as smtc.ps1) ──────────────

var lastKey = ""
var lastStatus = ""
var anchorPos = -1.0
var anchorAt = 0.0
var lastEmit = 0.0

func poll() {
  guard let picked = pick() else {
    current = nil
    if lastKey != "NONE" {
      lastKey = "NONE"
      emit(["type": "state", "session": NSNull()])
    }
    return
  }
  let (i, t) = picked
  current = i
  let src = sources[i]
  let status = t.state == "playing" ? "Playing" : t.state == "paused" ? "Paused" : t.state
  let key = "\(src.appId)|\(t.title)|\(t.artist)|\(t.album)"
  let now = nowMs()
  // AppleScript's position is current as of this call, so no staleness
  // correction is needed; only a real seek moves it off the prediction.
  let expected = status == "Playing" ? anchorPos + (now - anchorAt) / 1000 : anchorPos
  let seeked = anchorPos >= 0 && abs(t.position - expected) > 1.5

  if key != lastKey || status != lastStatus || seeked || now - lastEmit > 5000 {
    var state: [String: Any] = [
      "type": "state",
      "appId": src.appId,
      "title": t.title,
      "artist": t.artist,
      "album": t.album,
      "trackKey": key,
      "status": status,
      "playing": status == "Playing",
      "position": t.position,
      "duration": t.duration,
      "updatedAt": Int64(now),
      "sampledAt": Int64(now),
      "shuffle": t.shuffle,
      "repeat": t.repeatMode,
      "volume": t.volume,
      "caps": [
        "play": true, "pause": true, "next": true, "prev": true,
        "seek": true, "shuffle": true, "repeat": true,
      ],
    ]
    // macOS-only extras, not used by the app yet.
    if !t.spotifyUri.isEmpty { state["spotifyUri"] = t.spotifyUri }
    if !t.artUrl.isEmpty { state["artUrl"] = t.artUrl }
    emit(state)
    lastKey = key
    lastStatus = status
    anchorPos = t.position
    anchorAt = now
    lastEmit = now
  }
}

// ── Commands ───────────────────────────────────────────────────

func handle(_ line: String) {
  let parts = line.trimmingCharacters(in: .whitespaces).split(separator: " ", maxSplits: 1).map(String.init)
  guard let cmd = parts.first, let i = current, sources[i].isRunning else { return }
  let arg = Double(parts.count > 1 ? parts[1] : "") ?? 0
  let isMusic = sources[i].bundleId == "com.apple.Music"
  let body: String
  switch cmd {
  case "play": body = "play"
  case "pause": body = "pause"
  case "playpause": body = "playpause"
  case "next": body = "next track"
  case "prev": body = "previous track"
  // String(format:) with %f always uses "." whatever the user's locale;
  // AppleScript source also expects ".".
  case "seek": body = String(format: "set player position to %.3f", arg)
  case "volume": body = "set sound volume to \(Int((max(0, min(1, arg)) * 100).rounded()))"
  case "shuffle":
    body = isMusic ? "set shuffle enabled to not shuffle enabled" : "set shuffling to not shuffling"
  case "repeat":
    // Windows cycles None -> List -> Track -> None; Music can do the same,
    // Spotify's AppleScript only has on/off.
    body = isMusic
      ? """
        if song repeat is off then
          set song repeat to all
        else if song repeat is all then
          set song repeat to one
        else
          set song repeat to off
        end if
        """
      : "set repeating to not repeating"
  default: return
  }
  let id = sources[i].bundleId
  _ = run("if application id \"\(id)\" is running then\ntell application id \"\(id)\"\n\(body)\nend tell\nend if")
  poll()  // report the change right away
}

// ── Main loop ──────────────────────────────────────────────────

// stdin on a background thread; commands run on the main thread, like every
// AppleScript call (NSAppleScript is not thread-safe). EOF = parent is gone.
Thread.detachNewThread {
  while let line = readLine() {
    DispatchQueue.main.async { handle(line) }
  }
  exit(0)
}

// Both players announce changes; re-read at once instead of waiting a tick.
for name in ["com.spotify.client.PlaybackStateChanged", "com.apple.Music.playerInfo"] {
  DistributedNotificationCenter.default().addObserver(
    forName: Notification.Name(name), object: nil, queue: .main
  ) { _ in poll() }
}

// Once a second: catches seeks and keeps the 5 s heartbeat going.
Timer.scheduledTimer(withTimeInterval: 1, repeats: true) { _ in poll() }
poll()
RunLoop.main.run()
