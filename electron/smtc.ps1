# Long-lived Windows SMTC bridge.
#   stdout : one JSON object per line  ({"type":"state"|"art"|"error", ...})
#   stdin  : one command per line      (play|pause|playpause|next|prev|seek <sec>|shuffle|repeat)
#
# Must run under Windows PowerShell 5.1 (powershell.exe) — WinRT projection is
# unavailable in PowerShell 7 / .NET Core.

$ErrorActionPreference = 'Stop'
$OutputEncoding = [System.Text.UTF8Encoding]::new($false)
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

Add-Type -AssemblyName System.Runtime.WindowsRuntime

$asTaskGeneric = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
  $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and
  $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
})[0]

function Await($op, $resultType) {
  $task = $asTaskGeneric.MakeGenericMethod($resultType).Invoke($null, @($op))
  if (-not $task.Wait(5000)) { throw 'WinRT operation timed out' }
  $task.Result
}

function Emit($obj) {
  [Console]::Out.WriteLine(($obj | ConvertTo-Json -Compress -Depth 6))
  [Console]::Out.Flush()
}

# Set SMTC_LOG to a file path to trace the poll loop.
$TraceLog = $env:SMTC_LOG
function Trace($m) {
  if ($TraceLog) { Add-Content -Path $TraceLog -Value "$([DateTime]::Now.ToString('HH:mm:ss.fff')) $m" }
}

[Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager, Windows.Media.Control, ContentType=WindowsRuntime] | Out-Null

$TMediaProps = [Windows.Media.Control.GlobalSystemMediaTransportControlsSessionMediaProperties]

$mgr = Await ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager]::RequestAsync()) ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager])

# Music apps win: the widget follows one of these over anything else (a TikTok
# or YouTube video in the browser), playing first, then paused with a song
# loaded. The one exception is the Spotify the player starts hidden for
# lookups (see Hide-Spotify): while that is settling it is passed over unless
# it actually plays.
$MusicApps = 'spotify|cider|applemusic|itunes'

function Get-Session {
  $music = @($mgr.GetSessions() | Where-Object {
      $_.SourceAppUserModelId -match $MusicApps -and
      "$($_.GetPlaybackInfo().PlaybackStatus)" -eq 'Playing'
    })
  if ($music.Count) {
    $cur = $mgr.GetCurrentSession()
    # several playing: keep the one Windows considers current, if it is one of them
    if ($null -ne $cur -and $cur.SourceAppUserModelId -match $MusicApps -and
        "$($cur.GetPlaybackInfo().PlaybackStatus)" -eq 'Playing') { return $cur }
    return $music[0]
  }

  # none playing: a paused one, unless it is the hidden Spotify settling in
  $paused = @($mgr.GetSessions() | Where-Object {
      $_.SourceAppUserModelId -match $MusicApps -and
      "$($_.GetPlaybackInfo().PlaybackStatus)" -eq 'Paused' -and
      -not ((Get-Date) -lt $hideSpotifyUntil -and $_.SourceAppUserModelId -match 'spotify')
    })
  if ($paused.Count) {
    $cur = $mgr.GetCurrentSession()
    if ($null -ne $cur -and ($paused | Where-Object { $_.SourceAppUserModelId -eq $cur.SourceAppUserModelId })) { return $cur }
    return $paused[0]
  }

  $cur = $mgr.GetCurrentSession()
  # A Spotify started hidden by the player (see Hide-Spotify) registers a
  # paused session that Windows can make the current one. Look past it to what
  # the user is listening to, unless it has actually started playing.
  if ($null -ne $cur -and (Get-Date) -lt $hideSpotifyUntil -and
      $cur.SourceAppUserModelId -match 'spotify' -and
      "$($cur.GetPlaybackInfo().PlaybackStatus)" -ne 'Playing') {
    $other = @($mgr.GetSessions() | Where-Object { $_.SourceAppUserModelId -notmatch 'spotify' } |
      Sort-Object { "$($_.GetPlaybackInfo().PlaybackStatus)" -ne 'Playing' })
    if ($other.Count) { return $other[0] }
  }
  $cur
}

# ── Per-app volume (Core Audio) ────────────────────────────────
# SMTC carries no volume channel, so the slider drives the media app's own
# audio-session volume via IAudioSessionManager2. Unused vtable slots are
# declared as PreserveSig stubs purely to keep the interface layout correct.
Add-Type -Language CSharp @'
using System;
using System.Runtime.InteropServices;

namespace CoreAudio {
  [ComImport, Guid("A95664D2-9614-4F35-A746-DE8DB63617E6"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  public interface IMMDeviceEnumerator {
    [PreserveSig] int EnumAudioEndpoints(int dataFlow, int stateMask, out IntPtr devices);
    [PreserveSig] int GetDefaultAudioEndpoint(int dataFlow, int role, out IMMDevice device);
    [PreserveSig] int GetDevice(string id, out IMMDevice device);
    [PreserveSig] int RegisterEndpointNotificationCallback(IntPtr client);
    [PreserveSig] int UnregisterEndpointNotificationCallback(IntPtr client);
  }

  [ComImport, Guid("D666063F-1587-4E43-81F1-B948E807363F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  public interface IMMDevice {
    [PreserveSig] int Activate(ref Guid iid, int clsCtx, IntPtr activationParams,
      [MarshalAs(UnmanagedType.IUnknown)] out object iface);
    [PreserveSig] int OpenPropertyStore(int access, out IntPtr store);
    [PreserveSig] int GetId([MarshalAs(UnmanagedType.LPWStr)] out string id);
    [PreserveSig] int GetState(out int state);
  }

  [ComImport, Guid("77AA99A0-1BD6-484F-8BC7-2C654C9A9B6F"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  public interface IAudioSessionManager2 {
    [PreserveSig] int GetAudioSessionControl(IntPtr guid, int flags, out IntPtr ctl);
    [PreserveSig] int GetSimpleAudioVolume(IntPtr guid, int flags, out IntPtr vol);
    [PreserveSig] int GetSessionEnumerator(out IAudioSessionEnumerator e);
    [PreserveSig] int RegisterSessionNotification(IntPtr n);
    [PreserveSig] int UnregisterSessionNotification(IntPtr n);
    [PreserveSig] int RegisterDuckNotification(string s, IntPtr n);
    [PreserveSig] int UnregisterDuckNotification(IntPtr n);
  }

  [ComImport, Guid("E2F5BB11-0570-40CA-ACDD-3AA01277DEE8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  public interface IAudioSessionEnumerator {
    [PreserveSig] int GetCount(out int count);
    [PreserveSig] int GetSession(int index, [MarshalAs(UnmanagedType.IUnknown)] out object session);
  }

  [ComImport, Guid("BFB7FF88-7239-4FC9-8FA2-07C950BE9C6D"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  public interface IAudioSessionControl2 {
    [PreserveSig] int GetState(out int state);
    [PreserveSig] int GetDisplayName(out IntPtr name);
    [PreserveSig] int SetDisplayName(string name, IntPtr ctx);
    [PreserveSig] int GetIconPath(out IntPtr path);
    [PreserveSig] int SetIconPath(string path, IntPtr ctx);
    [PreserveSig] int GetGroupingParam(out Guid g);
    [PreserveSig] int SetGroupingParam(ref Guid g, IntPtr ctx);
    [PreserveSig] int RegisterAudioSessionNotification(IntPtr n);
    [PreserveSig] int UnregisterAudioSessionNotification(IntPtr n);
    [PreserveSig] int GetSessionIdentifier(out IntPtr id);
    [PreserveSig] int GetSessionInstanceIdentifier(out IntPtr id);
    [PreserveSig] int GetProcessId(out uint pid);
    [PreserveSig] int IsSystemSoundsSession();
    [PreserveSig] int SetDuckingPreference(bool opt);
  }

  [ComImport, Guid("87CE5498-68D6-44E5-9215-6DA47EF883D8"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
  public interface ISimpleAudioVolume {
    [PreserveSig] int SetMasterVolume(float level, ref Guid ctx);
    [PreserveSig] int GetMasterVolume(out float level);
    [PreserveSig] int SetMute(bool mute, ref Guid ctx);
    [PreserveSig] int GetMute(out bool mute);
  }

  [ComImport, Guid("BCDE0395-E52F-467C-8E3D-C4579291692E")]
  public class MMDeviceEnumerator { }

  public static class Volume {
    static Guid IID_MGR2 = new Guid("77AA99A0-1BD6-484F-8BC7-2C654C9A9B6F");

    // Returns the ISimpleAudioVolume of the render session owned by `pid`.
    static ISimpleAudioVolume FindSession(uint pid) {
      var enumr = (IMMDeviceEnumerator)(new MMDeviceEnumerator());
      IMMDevice dev;
      if (enumr.GetDefaultAudioEndpoint(0, 1, out dev) != 0 || dev == null) return null;
      object mgrObj;
      if (dev.Activate(ref IID_MGR2, 1, IntPtr.Zero, out mgrObj) != 0) return null;
      var mgr = (IAudioSessionManager2)mgrObj;
      IAudioSessionEnumerator sessions;
      if (mgr.GetSessionEnumerator(out sessions) != 0) return null;
      int count;
      if (sessions.GetCount(out count) != 0) return null;
      for (int i = 0; i < count; i++) {
        object s;
        if (sessions.GetSession(i, out s) != 0) continue;
        var ctl = s as IAudioSessionControl2;
        if (ctl == null) continue;
        uint spid;
        if (ctl.GetProcessId(out spid) != 0) continue;
        if (spid == pid) return s as ISimpleAudioVolume;
      }
      return null;
    }

    public static bool Set(uint pid, float level) {
      var v = FindSession(pid);
      if (v == null) return false;
      Guid ctx = Guid.Empty;
      return v.SetMasterVolume(level, ref ctx) == 0;
    }

    public static float Get(uint pid) {
      var v = FindSession(pid);
      if (v == null) return -1f;
      float level;
      return v.GetMasterVolume(out level) == 0 ? level : -1f;
    }
  }
}
'@

# Map an SMTC SourceAppUserModelId to a running process id. Ids are either a
# bare exe name ("Spotify.exe") or a packaged AUMID
# ("AppleInc.AppleMusicWin_nzyj5cx40ttqa!App"), so compare on alphanumerics.
function Get-AppPids($appId) {
  $exe = ($appId -replace '\.exe$', '')
  $norm = ($appId -replace '[^A-Za-z0-9]', '').ToLower()
  $procs = @(Get-Process -ErrorAction SilentlyContinue)

  # Apps like Spotify run several processes and only one owns the audio
  # session, so return every candidate and let the caller probe them.
  $hits = @($procs | Where-Object { $_.ProcessName -ieq $exe })
  if (-not $hits.Count) {
    $hits = @($procs | Where-Object {
      $pn = ($_.ProcessName -replace '[^A-Za-z0-9]', '').ToLower()
      $pn.Length -gt 3 -and ($norm.Contains($pn) -or $pn.Contains($norm))
    })
  }
  return @($hits | ForEach-Object { $_.Id })
}

function Get-AppVolume($appId) {
  try {
    foreach ($procId in Get-AppPids $appId) {
      $v = [CoreAudio.Volume]::Get([uint32]$procId)
      if ($v -ge 0) { return [Math]::Round($v, 3) }
    }
  } catch { }
  return -1
}

function Set-AppVolume($appId, $level) {
  try {
    foreach ($procId in Get-AppPids $appId) {
      if ([CoreAudio.Volume]::Set([uint32]$procId, [float]$level)) { return }
    }
  } catch {
    Emit @{ type = 'error'; message = "volume: $($_.Exception.Message)" }
  }
}

# NOTE: the SMTC thumbnail is deliberately not read here. PowerShell's WinRT
# projection does not survive IAsyncOperation<interface> — OpenReadAsync comes
# back as a raw __ComObject that cannot be cast to IRandomAccessStream — so
# album art is fetched over HTTP in the main process instead (see main.cjs),
# which also yields far better resolution than the thumbnail.

# ── Hidden Spotify (see spotifyQuickOpen.cjs) ──────────────────
# When the player starts Spotify for a token or a song lookup, nothing should
# appear: --minimized still puts a window on the taskbar. 'hide-spotify' has
# this loop hide that window the moment it shows, for as long as such a
# Spotify is ever kept open.
Add-Type -Name Win -Namespace HideSpotify -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool ShowWindow(System.IntPtr h, int cmd);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(System.IntPtr h);
'@
$hideSpotifyUntil = [DateTime]::MinValue

function Hide-Spotify {
  foreach ($p in Get-Process Spotify -ErrorAction SilentlyContinue) {
    $h = $p.MainWindowHandle
    if ($h -ne [IntPtr]::Zero -and [HideSpotify.Win]::IsWindowVisible($h)) {
      [void][HideSpotify.Win]::ShowWindow($h, 0)   # SW_HIDE
    }
  }
}

function Handle-Command($line) {
  # needs no media session, so before the check below
  if ($line.Trim() -eq 'hide-spotify') {
    $script:hideSpotifyUntil = (Get-Date).AddSeconds(25)
    return
  }
  $s = Get-Session
  if ($null -eq $s) { return }
  $parts = $line.Trim() -split '\s+', 2
  try {
    switch ($parts[0]) {
      'play'      { $s.TryPlayAsync()         | Out-Null }
      'pause'     { $s.TryPauseAsync()        | Out-Null }
      'playpause' { $s.TryTogglePlayPauseAsync() | Out-Null }
      'next'      { $s.TrySkipNextAsync()     | Out-Null }
      'prev'      { $s.TrySkipPreviousAsync() | Out-Null }
      'seek' {
        $sec = [double]$parts[1]
        $s.TryChangePlaybackPositionAsync([long]($sec * 10000000)) | Out-Null
      }
      'shuffle' {
        $cur = $s.GetPlaybackInfo().IsShuffleActive
        $s.TryChangeShuffleActiveAsync(-not $cur) | Out-Null
      }
      'volume' {
        Set-AppVolume "$($s.SourceAppUserModelId)" ([double]$parts[1])
      }
      'repeat' {
        $cur = $s.GetPlaybackInfo().AutoRepeatMode
        $next = switch ("$cur") { 'None' { 'List' } 'List' { 'Track' } default { 'None' } }
        $s.TryChangeAutoRepeatModeAsync([Windows.Media.MediaPlaybackAutoRepeatMode]::$next) | Out-Null
      }
    }
  } catch {
    Emit @{ type = 'error'; message = "command '$line' failed: $($_.Exception.Message)" }
  }
}

$lastKey = ''
$lastStatus = ''
$anchorPos = -1.0      # position at the last emitted state
$anchorAt = 0.0        # local ms when that state was emitted
$lastEmit = 0.0        # local ms of last emit, for the drift-correcting heartbeat

# Read the raw stdin handle rather than [Console]::In: under -NonInteractive,
# PowerShell hands out a reader that is already at EOF, which would make the
# loop quit on its first pass.
$stdinReader = [System.IO.StreamReader]::new(
  [Console]::OpenStandardInput(), [System.Text.UTF8Encoding]::new($false))
$readTask = $stdinReader.ReadLineAsync()

$iter = 0
while ($true) {
  $iter++
  Trace "iter $iter begin"
  try {
    $s = Get-Session
    if ($null -eq $s) {
      if ($lastKey -ne 'NONE') {
        $lastKey = 'NONE'
        Emit @{ type = 'state'; session = $null }
      }
    } else {
      $props = Await ($s.TryGetMediaPropertiesAsync()) $TMediaProps
      $tl = $s.GetTimelineProperties()
      $pb = $s.GetPlaybackInfo()

      $appId  = "$($s.SourceAppUserModelId)"
      $title  = "$($props.Title)"
      $artist = "$($props.Artist)"
      $key    = "$appId|$title|$artist|$($props.AlbumTitle)"
      $status = "$($pb.PlaybackStatus)"
      $pos    = $tl.Position.TotalSeconds
      $end    = $tl.EndTime.TotalSeconds

      # Emit on track change, status flip, a genuine seek, or every 5 s as a
      # drift-correcting heartbeat.
      #
      # Spotify only refreshes its timeline every few seconds, so `Position` on
      # its own goes stale between refreshes. Correcting it by the age of
      # LastUpdatedTime gives the true current position; comparing *that*
      # against the anchor's prediction is what isolates a real seek. Comparing
      # raw Position would flag a seek roughly once a second.
      $nowMs = [double]([DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds())
      $updatedMs = [double]([DateTimeOffset]$tl.LastUpdatedTime).ToUnixTimeMilliseconds()
      $ageSec = ($nowMs - $updatedMs) / 1000.0
      if ($ageSec -lt 0 -or $ageSec -gt 30) { $ageSec = 0 }
      $actual = if ($status -eq 'Playing') { $pos + $ageSec } else { $pos }

      $expected = if ($status -eq 'Playing') { $anchorPos + ($nowMs - $anchorAt) / 1000.0 } else { $anchorPos }
      $seeked = $anchorPos -ge 0 -and [Math]::Abs($actual - $expected) -gt 1.5

      if ($key -ne $lastKey -or $status -ne $lastStatus -or $seeked -or ($nowMs - $lastEmit) -gt 5000) {
        $state = @{
          type       = 'state'
          appId      = $appId
          title      = $title
          artist     = $artist
          album      = "$($props.AlbumTitle)"
          trackKey   = $key
          status     = $status
          playing    = ($status -eq 'Playing')
          position   = $pos
          duration   = $end
          # ms since epoch, so the renderer can extrapolate from the exact
          # moment Windows last refreshed the timeline
          updatedAt  = [long]([DateTimeOffset]$tl.LastUpdatedTime).ToUnixTimeMilliseconds()
          sampledAt  = [long][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
          shuffle    = [bool]$pb.IsShuffleActive
          repeat     = "$($pb.AutoRepeatMode)"
          volume     = Get-AppVolume $appId   # -1 when no audio session found
          caps       = @{
            play  = [bool]$pb.Controls.IsPlayEnabled
            pause = [bool]$pb.Controls.IsPauseEnabled
            next  = [bool]$pb.Controls.IsNextEnabled
            prev  = [bool]$pb.Controls.IsPreviousEnabled
            seek  = [bool]$pb.Controls.IsPlaybackPositionEnabled
            shuffle = [bool]$pb.Controls.IsShuffleEnabled
            repeat  = [bool]$pb.Controls.IsRepeatEnabled
          }
        }
        Emit $state

        $lastKey = $key
        $lastStatus = $status
        $anchorPos = $actual
        $anchorAt = $nowMs
        $lastEmit = $nowMs
      }
    }
  } catch {
    Emit @{ type = 'error'; message = "$($_.Exception.Message)" }
  }

  # Drain any pending commands after publishing state, so a run with no stdin
  # attached still emits at least once before it sees EOF and exits.
  while ($readTask.IsCompleted) {
    $line = $readTask.Result
    Trace "stdin line: '$line'"
    if ($null -eq $line) { Trace 'stdin EOF — exiting'; exit 0 }   # parent is gone
    Handle-Command $line
    $readTask = $stdinReader.ReadLineAsync()
  }

  # Polled fast while a hidden Spotify is starting, so its window is gone
  # before it can be noticed.
  if ((Get-Date) -lt $hideSpotifyUntil) {
    Hide-Spotify
    Start-Sleep -Milliseconds 40
  } else {
    Start-Sleep -Milliseconds 250
  }
}
