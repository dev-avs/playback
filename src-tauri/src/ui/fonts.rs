//! Installed font families, for the settings window's font list.
//!
//! Read through GDI, as the Electron build did (through .NET's GDI+ view), so
//! the names match what the page saved before, and a font installed per user
//! shows up too. Read once while the settings window is open.

use std::collections::BTreeSet;
use std::sync::Mutex;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::Graphics::Gdi::{
    EnumFontFamiliesExW, GetDC, ReleaseDC, DEFAULT_CHARSET, LOGFONTW, NEWTEXTMETRICW, TEXTMETRICW,
};
use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

/// The outline fonts GDI+ listed: TrueType, and OpenType either way. Bitmap
/// and vector fonts (Fixedsys, Modern…) were never offered.
const TRUETYPE_FONTTYPE: u32 = 4;
const RASTER_FONTTYPE: u32 = 1;
const NTM_PS_OPENTYPE: u32 = 0x20000;

static LISTED: Mutex<Option<Vec<String>>> = Mutex::new(None);

unsafe extern "system" fn add(font: *const LOGFONTW, metric: *const TEXTMETRICW, kind: u32, names: LPARAM) -> i32 {
    let names = &mut *(names.0 as *mut BTreeSet<String>);
    let face = &(*font).lfFaceName;
    let len = face.iter().position(|&c| c == 0).unwrap_or(face.len());
    let name = String::from_utf16_lossy(&face[..len]);
    // "@" names are the same family set vertically
    // GDI hands every non-bitmap font its NEWTEXTMETRIC, whose flags tell
    // OpenType with PostScript outlines (CFF) apart
    let outline = kind & TRUETYPE_FONTTYPE != 0
        || (kind & RASTER_FONTTYPE == 0 && (*(metric as *const NEWTEXTMETRICW)).ntmFlags & NTM_PS_OPENTYPE != 0);
    if outline && !name.is_empty() && !name.starts_with('@') {
        names.insert(name);
    }
    1
}

fn read() -> Vec<String> {
    let mut names = BTreeSet::new();
    unsafe {
        let screen = GetDesktopWindow();
        let dc = GetDC(Some(screen));
        let wanted = LOGFONTW { lfCharSet: DEFAULT_CHARSET, ..Default::default() };
        EnumFontFamiliesExW(dc, &wanted, Some(add), LPARAM(&mut names as *mut _ as isize), 0);
        ReleaseDC(Some(screen), dc);
    }
    names.into_iter().collect()
}

pub fn list() -> Vec<String> {
    let mut listed = LISTED.lock().unwrap();
    if listed.is_none() {
        // an empty answer is a failure: the next request tries again
        let names = read();
        if names.is_empty() {
            return names;
        }
        *listed = Some(names);
    }
    listed.clone().unwrap_or_default()
}

/// Called when the settings window closes.
pub fn forget() {
    *LISTED.lock().unwrap() = None;
}
