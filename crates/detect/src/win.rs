//! Windows implementation: window enumeration (`win_windows.cpp`), open files
//! (`win_open_files.cpp`) and UI Automation (`win_ui_automation.cpp`).
//!
//! The open-files strategy uses `NtQuerySystemInformation` and
//! `NtQueryObject`, which are internal but stable in practice; Anisthesia and
//! other tools rely on them.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU16, Ordering};

use windows::Win32::Foundation::{
    CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, HWND, LPARAM,
};
use windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_NAME_NORMALIZED, FILE_TYPE_DISK, GETFINALPATHNAMEBYHANDLE_FLAGS,
    GetFileAttributesW, GetFileType, GetFinalPathNameByHandleW, INVALID_FILE_ATTRIBUTES,
    VOLUME_NAME_DOS,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTreeWalker,
    IUIAutomationValuePattern, UIA_ComboBoxControlTypeId, UIA_DocumentControlTypeId,
    UIA_EditControlTypeId, UIA_IsEnabledPropertyId, UIA_IsKeyboardFocusablePropertyId,
    UIA_IsValuePatternAvailablePropertyId, UIA_MenuBarControlTypeId, UIA_PROPERTY_ID,
    UIA_TabControlTypeId, UIA_TabItemControlTypeId, UIA_TitleBarControlTypeId,
    UIA_ValueIsReadOnlyPropertyId, UIA_ValuePatternId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GWL_STYLE, GetClassNameW, GetForegroundWindow, GetWindowLongW,
    GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_POPUP,
};
use windows::core::{BOOL, PCWSTR, PWSTR};

use crate::matching::{apply_window_title_format, clean_path, infer_media_kind, is_player_window};
use crate::{Detection, MediaInfo, MediaInfoKind, Player, Strategy};

pub fn foreground_window() -> Option<isize> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_invalid()).then_some(hwnd.0 as isize)
}

pub fn detect(players: &[Player]) -> Vec<Detection> {
    let mut detections = Vec::new();
    for window in enumerate_windows() {
        let Some(player) = players
            .iter()
            .find(|p| is_player_window(p, &window.class_name, &window.process_name))
        else {
            continue;
        };
        let media = apply_strategies(player, &window);
        if media.is_empty() {
            continue;
        }
        detections.push(Detection {
            player: player.name.clone(),
            player_type: player.r#type,
            window: window.handle,
            window_title: window.text,
            process_id: window.process_id,
            process_name: window.process_name,
            media,
        });
    }
    detections
}

fn apply_strategies(player: &Player, window: &WindowInfo) -> Vec<MediaInfo> {
    let mut media = Vec::new();
    let mut add = |kind: MediaInfoKind, value: String| {
        if !value.is_empty() {
            media.push(MediaInfo { kind, value });
        }
    };
    for strategy in &player.strategies {
        match strategy {
            Strategy::WindowTitle => {
                let title = apply_window_title_format(&player.window_title_format, &window.text);
                add(infer_media_kind(&title), title);
            }
            Strategy::OpenFiles => {
                for path in open_files(window.process_id) {
                    add(MediaInfoKind::File, path);
                }
            }
            Strategy::UiAutomation => {
                if let Some(info) = web_browser_information(window.handle) {
                    add(
                        MediaInfoKind::Title,
                        apply_window_title_format(&player.window_title_format, &info.title),
                    );
                    add(MediaInfoKind::Url, info.address);
                    for tab in info.tabs {
                        add(MediaInfoKind::Tab, tab);
                    }
                }
            }
        }
    }
    media
}

// --- Windows -----------------------------------------------------------------

struct WindowInfo {
    handle: isize,
    class_name: String,
    text: String,
    process_id: u32,
    process_name: String,
}

fn hwnd(handle: isize) -> HWND {
    HWND(handle as *mut c_void)
}

fn enumerate_windows() -> Vec<WindowInfo> {
    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the `Vec` passed to `EnumWindows` below, which
        // outlives the enumeration.
        let windows = unsafe { &mut *(lparam.0 as *mut Vec<WindowInfo>) };
        if let Some(info) = window_info(hwnd) {
            windows.push(info);
        }
        BOOL(1)
    }
    let mut windows: Vec<WindowInfo> = Vec::new();
    // Only top-level windows of desktop apps (not UWP) are enumerated.
    let _ = unsafe { EnumWindows(Some(callback), LPARAM(&mut windows as *mut _ as isize)) };
    windows
}

fn window_info(hwnd: HWND) -> Option<WindowInfo> {
    if !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return None;
    }
    // Skip toolbars, tooltips and similar top-most windows.
    let style = unsafe { GetWindowLongW(hwnd, GWL_STYLE) } as u32;
    let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) } as u32;
    let tool = ex_style & WS_EX_TOOLWINDOW.0 != 0;
    if tool && (style & WS_POPUP.0 != 0 || ex_style & WS_EX_TOPMOST.0 != 0) {
        return None;
    }

    let mut buffer = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    let class_name = String::from_utf16_lossy(&buffer[..len.max(0) as usize]);
    const INVALID_CLASSES: [&str; 11] = [
        "#32770",
        "CabinetWClass",
        "ComboLBox",
        "DDEMLEvent",
        "DDEMLMom",
        "DirectUIHWND",
        "GDI+ Hook Window Class",
        "IME",
        "Internet Explorer_Hidden",
        "MSCTFIME UI",
        "tooltips_class32",
    ];
    if class_name.is_empty() || INVALID_CLASSES.contains(&class_name.as_str()) {
        return None;
    }

    let mut text = [0u16; 1024];
    let len = unsafe { GetWindowTextW(hwnd, &mut text) };
    let text = String::from_utf16_lossy(&text[..len.max(0) as usize]);

    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
    let path = process_path(process_id)?;
    if is_system_directory(&path) {
        return None;
    }
    let file_name = path.rsplit(['/', '\\']).next().unwrap_or(&path);
    let process_name = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem)
        .to_owned();
    const INVALID_PROCESSES: [&str; 5] =
        ["explorer", "taskeng", "taskhost", "taskhostex", "Taskmgr"];
    if process_name.is_empty() || INVALID_PROCESSES.contains(&process_name.as_str()) {
        return None;
    }

    Some(WindowInfo {
        handle: hwnd.0 as isize,
        class_name,
        text,
        process_id,
        process_name,
    })
}

/// Closes the handle when dropped.
struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}

fn process_path(process_id: u32) -> Option<String> {
    // Limited access works for elevated processes too; SYSTEM processes fail.
    let process =
        Owned(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }.ok()?);
    let mut buffer = [0u16; 1024];
    let mut size = buffer.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut size,
        )
    }
    .ok()?;
    Some(String::from_utf16_lossy(&buffer[..size as usize]))
}

fn is_system_directory(path: &str) -> bool {
    let windir = std::env::var("windir").unwrap_or_else(|_| r"C:\Windows".to_owned());
    let path = path.trim_start_matches(['\\', '?']);
    path.len() >= windir.len() && path[..windir.len()].eq_ignore_ascii_case(&windir)
}

// --- Open files ----------------------------------------------------------------

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtQuerySystemInformation(
        class: u32,
        info: *mut c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
    fn NtQueryObject(
        handle: *mut c_void,
        class: u32,
        info: *mut c_void,
        length: u32,
        returned: *mut u32,
    ) -> i32;
}

/// `SystemHandleInformation` is limited to 16-bit process IDs.
const SYSTEM_EXTENDED_HANDLE_INFORMATION: u32 = 64;
const OBJECT_TYPE_INFORMATION: u32 = 2;
const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;

const FILE_READ_DATA: u32 = 0x0001;
const FILE_APPEND_DATA: u32 = 0x0004;
const FILE_WRITE_EA: u32 = 0x0010;
const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;

#[repr(C)]
#[derive(Clone, Copy)]
struct HandleEntry {
    object: *mut c_void,
    unique_process_id: usize,
    handle_value: *mut c_void,
    granted_access: u32,
    creator_back_trace_index: u16,
    object_type_index: u16,
    handle_attributes: u32,
    reserved: u32,
}

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

/// 8-byte aligned buffer for the NT query results.
fn aligned(size: usize) -> Vec<u64> {
    vec![0u64; size.div_ceil(8)]
}

fn system_handles() -> Option<Vec<u64>> {
    let mut size: u32 = 1 << 20;
    const MAX_SIZE: u32 = 1 << 26;
    loop {
        let mut buffer = aligned(size as usize);
        let mut returned = 0u32;
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_EXTENDED_HANDLE_INFORMATION,
                buffer.as_mut_ptr().cast(),
                size,
                &mut returned,
            )
        };
        if status == STATUS_INFO_LENGTH_MISMATCH && size < MAX_SIZE {
            size = if returned > size {
                returned + (1 << 16)
            } else {
                size * 2
            };
            continue;
        }
        return (status >= 0).then_some(buffer);
    }
}

fn object_type_name(handle: HANDLE) -> Option<String> {
    let mut buffer = aligned(1024);
    let mut returned = 0u32;
    let status = unsafe {
        NtQueryObject(
            handle.0,
            OBJECT_TYPE_INFORMATION,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            &mut returned,
        )
    };
    if status < 0 {
        return None;
    }
    // SAFETY: on success the buffer starts with the type name.
    let name = unsafe { &*(buffer.as_ptr() as *const UnicodeString) };
    if name.buffer.is_null() || name.length == 0 {
        return None;
    }
    let chars = unsafe { std::slice::from_raw_parts(name.buffer, name.length as usize / 2) };
    Some(String::from_utf16_lossy(chars))
}

/// The object type index of files varies between Windows versions, so it's
/// learned from the first handle whose type name is `File`.
static FILE_TYPE_INDEX: AtomicU16 = AtomicU16::new(0);

fn is_file_object(handle: Option<HANDLE>, type_index: u16) -> bool {
    let known = FILE_TYPE_INDEX.load(Ordering::Relaxed);
    if known != 0 {
        return type_index == known;
    }
    let Some(handle) = handle else {
        return true; // can't tell yet
    };
    if object_type_name(handle).as_deref() == Some("File") {
        FILE_TYPE_INDEX.store(type_index, Ordering::Relaxed);
        return true;
    }
    false
}

/// Some handles (mostly named pipes) make `NtQueryObject` and
/// `GetFinalPathNameByHandle` hang. Players read video files and don't write
/// to them, so only read-only handles are checked.
fn is_read_only_access(mask: u32) -> bool {
    mask & FILE_READ_DATA != 0
        && mask & (FILE_APPEND_DATA | FILE_WRITE_EA | FILE_WRITE_ATTRIBUTES) == 0
}

fn final_path(handle: HANDLE) -> Option<String> {
    let mut buffer = vec![0u16; 1024];
    let flags = GETFINALPATHNAMEBYHANDLE_FLAGS(FILE_NAME_NORMALIZED.0 | VOLUME_NAME_DOS.0);
    let mut len = unsafe { GetFinalPathNameByHandleW(handle, &mut buffer, flags) } as usize;
    if len > buffer.len() {
        buffer.resize(len, 0);
        len = unsafe { GetFinalPathNameByHandleW(handle, &mut buffer, flags) } as usize;
    }
    (len > 0 && len <= buffer.len()).then(|| String::from_utf16_lossy(&buffer[..len]))
}

fn is_existing_file(path: &str) -> bool {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let attributes = unsafe { GetFileAttributesW(PCWSTR(wide.as_ptr())) };
    attributes != INVALID_FILE_ATTRIBUTES && attributes & FILE_ATTRIBUTE_DIRECTORY.0 == 0
}

/// Files the process has open for reading, outside the Windows folder.
pub fn open_files(process_id: u32) -> Vec<String> {
    let Ok(process) = (unsafe { OpenProcess(PROCESS_DUP_HANDLE, false, process_id) }) else {
        return Vec::new();
    };
    let process = Owned(process);
    let Some(buffer) = system_handles() else {
        return Vec::new();
    };
    // Layout: number of handles, a reserved word, then the entries.
    let count = buffer[0] as usize;
    let entries = unsafe {
        let start = buffer.as_ptr().add(2) as *const HandleEntry;
        let available = (buffer.len() * 8 - 16) / std::mem::size_of::<HandleEntry>();
        std::slice::from_raw_parts(start, count.min(available))
    };

    let mut files = Vec::new();
    for entry in entries {
        if entry.unique_process_id != process_id as usize
            || !is_file_object(None, entry.object_type_index)
            || !is_read_only_access(entry.granted_access)
        {
            continue;
        }
        let mut duplicate = HANDLE::default();
        let duplicated = unsafe {
            DuplicateHandle(
                process.0,
                HANDLE(entry.handle_value),
                GetCurrentProcess(),
                &mut duplicate,
                0,
                false,
                DUPLICATE_SAME_ACCESS,
            )
        };
        if duplicated.is_err() {
            continue;
        }
        let duplicate = Owned(duplicate);
        if !is_file_object(Some(duplicate.0), entry.object_type_index)
            || unsafe { GetFileType(duplicate.0) } != FILE_TYPE_DISK
        {
            continue;
        }
        let Some(path) = final_path(duplicate.0) else {
            continue;
        };
        let path = clean_path(&path);
        if !is_system_directory(&path) && is_existing_file(&path) {
            files.push(path);
        }
    }
    files
}

// --- UI Automation -----------------------------------------------------------

struct BrowserInfo {
    title: String,
    address: String,
    tabs: Vec<String>,
}

thread_local! {
    static AUTOMATION: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
}

fn automation() -> Option<IUIAutomation> {
    AUTOMATION.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            unsafe {
                // May already be initialised on this thread; that's fine.
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                *cell = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok();
            }
        }
        cell.clone()
    })
}

fn bool_property(element: &IUIAutomationElement, property: UIA_PROPERTY_ID) -> Option<bool> {
    let value = unsafe { element.GetCurrentPropertyValue(property) }.ok()?;
    bool::try_from(&value).ok()
}

fn has_properties(element: &IUIAutomationElement, properties: &[(UIA_PROPERTY_ID, bool)]) -> bool {
    properties
        .iter()
        .all(|&(property, expected)| bool_property(element, property) == Some(expected))
}

fn element_name(element: &IUIAutomationElement) -> String {
    unsafe { element.CurrentName() }
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn element_value(element: &IUIAutomationElement) -> String {
    unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
        .and_then(|pattern| unsafe { pattern.CurrentValue() })
        .map(|s| s.to_string())
        .unwrap_or_default()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    /// This element only.
    Element,
    /// Look at the children, but not further.
    Children,
    Descendants,
}

fn web_browser_information(handle: isize) -> Option<BrowserInfo> {
    let automation = automation()?;
    let root = unsafe { automation.ElementFromHandle(hwnd(handle)) }.ok()?;
    let title = element_name(&root);
    let walker = unsafe { automation.ControlViewWalker() }.ok()?;
    let mut address = String::new();
    let mut tabs = Vec::new();

    // Walks the tree rather than using FindAll, so that page content
    // (documents) can be skipped, which is much faster.
    let mut visit = |element: &IUIAutomationElement| -> Scope {
        let control = unsafe { element.CurrentControlType() }.unwrap_or_default();
        if control == UIA_DocumentControlTypeId
            || control == UIA_MenuBarControlTypeId
            || control == UIA_TitleBarControlTypeId
        {
            Scope::Element
        } else if control == UIA_ComboBoxControlTypeId || control == UIA_EditControlTypeId {
            // The first editable, focusable edit with a value is the address bar.
            let is_address_bar = has_properties(
                element,
                &[
                    (UIA_IsEnabledPropertyId, true),
                    (UIA_IsKeyboardFocusablePropertyId, true),
                    (UIA_IsValuePatternAvailablePropertyId, true),
                    (UIA_ValueIsReadOnlyPropertyId, false),
                ],
            );
            if address.is_empty() && is_address_bar {
                address = element_value(element);
                Scope::Element
            } else {
                Scope::Descendants // Opera nests the address field
            }
        } else if control == UIA_TabControlTypeId {
            if tabs.is_empty() && has_properties(element, &[(UIA_ValueIsReadOnlyPropertyId, true)])
            {
                Scope::Children
            } else {
                Scope::Element
            }
        } else if control == UIA_TabItemControlTypeId {
            tabs.push(element_name(element));
            Scope::Element
        } else if !address.is_empty() && !tabs.is_empty() {
            Scope::Element // done
        } else {
            Scope::Descendants
        }
    };
    walk(&walker, &root, Scope::Descendants, 0, &mut visit);
    Some(BrowserInfo {
        title,
        address,
        tabs,
    })
}

fn walk(
    walker: &IUIAutomationTreeWalker,
    parent: &IUIAutomationElement,
    scope: Scope,
    depth: usize,
    visit: &mut impl FnMut(&IUIAutomationElement) -> Scope,
) {
    const MAX_DEPTH: usize = 16;
    if depth > MAX_DEPTH || scope == Scope::Element {
        return;
    }
    let mut child = unsafe { walker.GetFirstChildElement(parent) }.ok();
    while let Some(element) = child {
        let child_scope = visit(&element);
        if child_scope != Scope::Element {
            walk(walker, &element, child_scope, depth + 1, visit);
        }
        child = unsafe { walker.GetNextSiblingElement(&element) }.ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerates_without_crashing() {
        // Whatever is open on this machine; mostly checks the FFI paths.
        let windows = enumerate_windows();
        for w in &windows {
            assert!(!w.class_name.is_empty());
            assert!(!w.process_name.is_empty());
        }
        let _ = foreground_window();
    }

    #[test]
    fn open_files_of_this_process() {
        let path = std::env::temp_dir().join(format!("tracker-open-{}.mkv", std::process::id()));
        std::fs::write(&path, b"x").unwrap();
        let file = std::fs::File::open(&path).unwrap();
        let files = open_files(std::process::id());
        drop(file);
        let _ = std::fs::remove_file(&path);
        let expected = path
            .canonicalize()
            .map(|p| clean_path(&p.display().to_string()));
        assert!(
            files.iter().any(|f| Ok(f.to_lowercase())
                == expected.as_ref().map(|e| e.to_lowercase()).map_err(|_| ())
                || f.to_lowercase()
                    .ends_with(&format!("tracker-open-{}.mkv", std::process::id()))),
            "{files:?}"
        );
    }

    #[test]
    fn read_only_access_mask() {
        assert!(is_read_only_access(0x0012_0089));
        assert!(!is_read_only_access(0x0012_019f)); // write access
        assert!(!is_read_only_access(0x0010_0000)); // synchronize only
    }
}
