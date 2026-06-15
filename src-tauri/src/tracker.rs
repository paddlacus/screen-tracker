/// Returns the name of the currently focused application, or None if it can't be determined.
pub fn get_active_app() -> Option<String> {
    #[cfg(target_os = "macos")]
    return get_active_app_mac();

    #[cfg(target_os = "windows")]
    return get_active_app_windows();

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    None
}

/// Returns seconds since the last keyboard or mouse input, or None on error.
pub fn idle_seconds() -> Option<u64> {
    #[cfg(target_os = "macos")]
    return idle_seconds_mac();

    #[cfg(target_os = "windows")]
    return idle_seconds_windows();

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    None
}

// ── macOS ─────────────────────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn get_active_app_mac() -> Option<String> {
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get name of first application process whose frontmost is true")
        .output()
        .ok()?;
    if output.status.success() {
        let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if name.is_empty() { None } else { Some(name) }
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn idle_seconds_mac() -> Option<u64> {
    // ioreg reports HIDIdleTime in nanoseconds
    let output = std::process::Command::new("ioreg")
        .args(["-c", "IOHIDSystem"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("HIDIdleTime") {
            // line looks like: "HIDIdleTime" = 12345678901
            if let Some(val_str) = line.split('=').nth(1) {
                if let Ok(ns) = val_str.trim().parse::<u64>() {
                    return Some(ns / 1_000_000_000);
                }
            }
        }
    }
    None
}

// ── Windows ───────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn get_active_app_windows() -> Option<String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0 == 0 {
            return None;
        }
        let mut pid: u32 = 0;
        windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 260];
        let mut len = buf.len() as u32;
        windows::Win32::System::Threading::QueryFullProcessImageNameW(
            handle,
            windows::Win32::System::Threading::PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        // Return just the exe name without path or extension
        std::path::Path::new(&path)
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_string())
    }
}

#[cfg(target_os = "windows")]
fn idle_seconds_windows() -> Option<u64> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
    use windows::Win32::System::SystemInformation::GetTickCount;

    unsafe {
        let mut info = LASTINPUTINFO {
            cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
            dwTime: 0,
        };
        if GetLastInputInfo(&mut info).as_bool() {
            let now = GetTickCount();
            let elapsed_ms = now.wrapping_sub(info.dwTime) as u64;
            Some(elapsed_ms / 1000)
        } else {
            None
        }
    }
}
