use std::sync::OnceLock;

#[cfg(target_os = "windows")]
pub fn windows_dark_mode() -> bool {
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    static CACHE: OnceLock<Mutex<(bool, Instant)>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new((query_dark_mode_registry(), Instant::now())));
    let mut guard = cache.lock().unwrap();
    if guard.1.elapsed() > Duration::from_millis(500) {
        let new_val = query_dark_mode_registry();
        *guard = (new_val, Instant::now());
    }
    guard.0
}

#[cfg(target_os = "windows")]
fn query_dark_mode_registry() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ, REG_DWORD,
    };

    let subkey: Vec<u16> =
        OsStr::new("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
    let value_name: Vec<u16> = OsStr::new("AppsUseLightTheme")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut hkey = std::ptr::null_mut();
    let status =
        unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_READ, &mut hkey) };
    if status != ERROR_SUCCESS || hkey.is_null() {
        return fallback_dark_mode_via_reg_exe();
    }
    let mut data: u32 = 0;
    let mut data_size = std::mem::size_of::<u32>() as u32;
    let mut value_type: u32 = 0;
    let status = unsafe {
        RegQueryValueExW(
            hkey,
            value_name.as_ptr(),
            std::ptr::null_mut(),
            &mut value_type,
            &mut data as *mut u32 as *mut u8,
            &mut data_size,
        )
    };
    unsafe { RegCloseKey(hkey) };
    if status == ERROR_SUCCESS && value_type == REG_DWORD {
        // 0 = dark, 1 = light
        data == 0
    } else {
        fallback_dark_mode_via_reg_exe()
    }
}

#[cfg(target_os = "windows")]
fn fallback_dark_mode_via_reg_exe() -> bool {
    std::process::Command::new("reg")
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
        .unwrap_or(false)
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
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, FindWindowW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    };

    let title: Vec<u16> = OsStr::new(WINDOW_TITLE)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let pid = unsafe { GetCurrentProcessId() };

    // Fast path: FindWindowW and verify it belongs to this process.
    let candidate = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if !candidate.is_null() {
        let mut wnd_pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(candidate, &mut wnd_pid) };
        if wnd_pid == pid {
            return candidate;
        }
    }

    // Fallback: enumerate top-level windows and find one with matching title and pid.
    struct Ctx {
        pid: u32,
        title: Vec<u16>,
        found: windows_sys::Win32::Foundation::HWND,
    }
    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: isize) -> i32 {
        let ctx = &mut *(lparam as *mut Ctx);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut wnd_pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut wnd_pid);
        if wnd_pid != ctx.pid {
            return 1;
        }
        // Compare window text
        let mut buf = [0u16; 512];
        let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if len <= 0 {
            return 1;
        }
        let wnd_title = &buf[..len as usize];
        // Compare without trailing null; ctx.title includes null terminator, so compare without last element
        let expected = &ctx.title[..ctx.title.len() - 1];
        if wnd_title == expected {
            ctx.found = hwnd;
            return 0; // stop enumeration
        }
        1
    }

    let mut ctx = Ctx {
        pid,
        title,
        found: std::ptr::null_mut(),
    };
    unsafe { EnumWindows(Some(enum_proc), &mut ctx as *mut Ctx as isize) };
    ctx.found
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

pub const WINDOW_TITLE: &str = concat!("HEAT3 Поворотник v", env!("CARGO_PKG_VERSION"));

pub const INNER_WIDTH: f32 = 460.0;
pub const INNER_HEIGHT: f32 = 600.0;
pub const SIDE_WIDTH: f32 = 140.0;
