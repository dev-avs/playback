//! The media app's own volume. The media session has no volume, so the
//! slider drives the app's audio session on the default output device.
//!
//! COM must already be set up on the calling thread (the media thread does).

use windows::core::{Interface, Result};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::Media::Audio::{
    eMultimedia, eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
    MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};

/// Every running process as (id, exe name without ".exe").
fn processes() -> Vec<(u32, String)> {
    let mut out = Vec::new();
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snap, &mut entry).is_ok();
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]);
            out.push((entry.th32ProcessID, strip_exe(&name).to_string()));
            more = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

fn strip_exe(name: &str) -> &str {
    if name.to_lowercase().ends_with(".exe") {
        &name[..name.len() - 4]
    } else {
        name
    }
}

fn alnum_lower(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

/// The processes that may own `app_id`'s audio. Ids are a bare exe name
/// ("Spotify.exe") or a packaged app id ("AppleInc.AppleMusicWin_…!App"), and
/// apps like Spotify run several processes, so every candidate is returned.
fn pids_for(app_id: &str, procs: &[(u32, String)]) -> Vec<u32> {
    let exe = strip_exe(app_id);
    let exact: Vec<u32> = procs.iter().filter(|(_, n)| n.eq_ignore_ascii_case(exe)).map(|p| p.0).collect();
    if !exact.is_empty() {
        return exact;
    }
    let id = alnum_lower(app_id);
    procs
        .iter()
        .filter(|(_, n)| {
            let n = alnum_lower(n);
            n.len() > 3 && (id.contains(&n) || n.contains(&id))
        })
        .map(|p| p.0)
        .collect()
}

/// The volume control of the first audio session owned by one of `pids`.
fn session_for(pids: &[u32]) -> Result<Option<ISimpleAudioVolume>> {
    unsafe {
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = devices.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
        let sessions = manager.GetSessionEnumerator()?;
        for i in 0..sessions.GetCount()? {
            let Ok(control) = sessions.GetSession(i) else { continue };
            let Ok(control) = control.cast::<IAudioSessionControl2>() else { continue };
            if control.GetProcessId().is_ok_and(|pid| pids.contains(&pid)) {
                return Ok(control.cast::<ISimpleAudioVolume>().ok());
            }
        }
    }
    Ok(None)
}

fn volume_of(app_id: &str) -> Option<ISimpleAudioVolume> {
    let pids = pids_for(app_id, &processes());
    if pids.is_empty() {
        return None;
    }
    session_for(&pids).ok().flatten()
}

/// 0..1, or None when the app has no audio session.
pub fn get(app_id: &str) -> Option<f64> {
    let v = volume_of(app_id)?;
    unsafe { v.GetMasterVolume().ok() }.map(|l| (f64::from(l) * 1000.0).round() / 1000.0)
}

pub fn set(app_id: &str, level: f32) -> bool {
    let Some(v) = volume_of(app_id) else { return false };
    unsafe { v.SetMasterVolume(level.clamp(0.0, 1.0), std::ptr::null()).is_ok() }
}

#[cfg(test)]
mod tests {
    use super::pids_for;

    #[test]
    fn finds_an_apps_processes() {
        let procs = [(1, "Spotify".to_string()), (2, "Spotify".into()), (3, "AppleMusic".into()), (4, "svc".into())];
        assert_eq!(pids_for("Spotify.exe", &procs), [1, 2]);
        assert_eq!(pids_for("AppleInc.AppleMusicWin_nzyj5cx40ttqa!App", &procs), [3]);
        assert!(pids_for("Chrome", &procs).is_empty());
    }
}
