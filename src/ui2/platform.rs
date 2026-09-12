use std::sync::OnceLock;

#[cfg(target_os = "windows")]
pub fn windows_dark_mode() -> bool {
    static CACHE: OnceLock<std::sync::Mutex<bool>> = OnceLock::new();
    static DONE: std::sync::Once = std::sync::Once::new();

    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(false));
    DONE.call_once(|| {
        // Query once; never spawn `reg` on the UI thread again (it caused
        // periodic freezes). The value only changes when the app restarts.
        let value = std::process::Command::new("reg")
            .args([
                "query",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
                "/v",
                "AppsUseLightTheme",
            ])
            .output()
            .ok()
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .map(|output| output.contains("0x0"))
            .unwrap_or(false);
        *cache.lock().unwrap() = value;
    });
    *cache.lock().unwrap()
}

#[cfg(not(target_os = "windows"))]
pub fn windows_dark_mode() -> bool {
    false
}

#[cfg(target_os = "windows")]
pub fn app_icon() -> Option<iced::window::Icon> {
    let image = image::load_from_memory_with_format(
        include_bytes!("../../resources/logocube.ico"),
        image::ImageFormat::Ico,
    )
    .ok()?
    .into_rgba8();
    let (width, height) = image.dimensions();
    iced::window::icon::from_rgba(image.into_raw(), width, height).ok()
}

#[cfg(not(target_os = "windows"))]
pub fn app_icon() -> Option<iced::window::Icon> {
    None
}

#[cfg(target_os = "windows")]
fn app_window_handle() -> windows_sys::Win32::Foundation::HWND {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::FindWindowW;

    let title: Vec<u16> = OsStr::new(WINDOW_TITLE)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) }
}

#[cfg(target_os = "windows")]
pub fn apply_titlebar_theme(dark: bool) {
    use windows_sys::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DWMWA_USE_IMMERSIVE_DARK_MODE,
    };

    unsafe {
        let hwnd = app_window_handle();
        if !hwnd.is_null() {
            let value: u32 = u32::from(dark);
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                &value as *const u32 as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );

            // Windows 11 can keep a light caption even after the immersive
            // flag changes. Explicit caption colors make the native chrome
            // follow the app theme; older Windows versions simply ignore
            // these two unsupported attributes.
            let caption_color: u32 = if dark { 0x0017_110D } else { 0x00F8_F6F4 };
            let text_color: u32 = if dark { 0x00FA_F7F5 } else { 0x002B_2118 };
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_CAPTION_COLOR as u32,
                &caption_color as *const u32 as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_TEXT_COLOR as u32,
                &text_color as *const u32 as *const std::ffi::c_void,
                std::mem::size_of::<u32>() as u32,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_titlebar_theme(_: bool) {}

pub const WINDOW_TITLE: &str = "HEAT3 Поворотник v2.0.0";

pub const INNER_WIDTH: f32 = 460.0;
pub const INNER_HEIGHT: f32 = 600.0;
pub const SIDE_WIDTH: f32 = 140.0;
