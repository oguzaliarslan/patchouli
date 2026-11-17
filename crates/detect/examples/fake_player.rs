//! A stand-in media player for testing detection without a real one.
//!
//! Opens a file for reading (like a player playing it) and shows a window
//! with a given class and title, then exits after some seconds. Copy the
//! built executable to the player's executable name (e.g. `mpv.exe`), since
//! detection matches on it:
//!
//! ```text
//! cargo build -p patchouli-detect --example fake_player
//! copy target\debug\examples\fake_player.exe %TEMP%\mpv.exe
//! %TEMP%\mpv.exe mpv "Show - 01.mkv - mpv" D:\Anime\Show - 01.mkv 30
//! ```

#[cfg(windows)]
fn main() {
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DispatchMessageW, MSG, PM_REMOVE,
        PeekMessageW, RegisterClassW, SW_SHOWNOACTIVATE, ShowWindow, TranslateMessage,
        WINDOW_EX_STYLE, WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };
    use windows::core::PCWSTR;

    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    let args: Vec<String> = std::env::args().collect();
    let [_, class, title, file, seconds] = args.as_slice() else {
        eprintln!("usage: fake_player <window class> <window title> <file> <seconds>");
        std::process::exit(2);
    };
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let _open = std::fs::File::open(file).expect("open file");

    unsafe {
        let instance = GetModuleHandleW(None).expect("module handle");
        let class_name = wide(class);
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let title = wide(title);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            320,
            200,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .expect("create window");
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        let end = Instant::now() + Duration::from_secs(seconds.parse().unwrap_or(30));
        let mut msg = MSG::default();
        while Instant::now() < end {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows only");
}
