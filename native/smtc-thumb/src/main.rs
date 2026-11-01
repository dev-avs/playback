// Prints the cover Windows' media controls hold for one app, as a data: URL.
//
//   smtc-thumb.exe <SourceAppUserModelId> <title>
//
// electron/smtc.ps1 cannot read it: PowerShell's WinRT projection hands back
// OpenReadAsync's stream as a bare __ComObject. Exit codes: 0 printed, 2 the
// session has no thumbnail (yet), 3 the session is gone or now on another
// title, 1 anything else.

use base64::Engine;
use std::process::ExitCode;
use windows::Media::Control::GlobalSystemMediaTransportControlsSessionManager as Manager;
use windows::Storage::Streams::DataReader;

fn run(app_id: &str, title: &str) -> windows::core::Result<u8> {
    let mgr = Manager::RequestAsync()?.join()?;
    let sessions = mgr.GetSessions()?;
    let Some(session) = sessions
        .into_iter()
        .find(|s| s.SourceAppUserModelId().map(|id| id == app_id).unwrap_or(false))
    else {
        return Ok(3);
    };
    let props = session.TryGetMediaPropertiesAsync()?.join()?;
    // The thumbnail must belong to the song that asked, not the one before it.
    if props.Title()? != title {
        return Ok(3);
    }
    let Ok(thumb) = props.Thumbnail() else { return Ok(2) };
    let stream = thumb.OpenReadAsync()?.join()?;
    let size = stream.Size()? as u32;
    if size == 0 {
        return Ok(2);
    }
    let kind = stream.ContentType()?.to_string();
    let reader = DataReader::CreateDataReader(&stream)?;
    reader.LoadAsync(size)?.join()?;
    let mut bytes = vec![0u8; size as usize];
    reader.ReadBytes(&mut bytes)?;
    let kind = if kind.starts_with("image/") { kind } else { "image/png".into() };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    print!("data:{kind};base64,{b64}");
    Ok(0)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: smtc-thumb <appId> <title>");
        return ExitCode::from(1);
    }
    match run(&args[1], &args[2]) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
