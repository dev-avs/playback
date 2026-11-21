// GPUI's Windows backend loads the window icon from resource id 1 of the exe (gpui platform.rs `load_icon`);
// without it the taskbar shows a blank icon. Same icon as the Electron build (package.json `build.icon`).
fn main() {
    println!("cargo:rerun-if-changed=../build/icon.ico");
    let mut res = winresource::WindowsResource::new();
    res.set_icon_with_id("../build/icon.ico", "1");
    res.compile().expect("embed icon");
}
