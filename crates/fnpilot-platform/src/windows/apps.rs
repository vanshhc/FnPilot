use fnpilot_core::Result;
use std::{collections::BTreeMap, path::PathBuf};
use windows::Win32::{
    Foundation::{HWND, LPARAM},
    UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible},
};
use windows::core::BOOL;

pub fn running() -> Vec<(String, HWND)> {
    unsafe extern "system" fn collect(hwnd: HWND, context: LPARAM) -> BOOL {
        let apps = unsafe { &mut *(context.0 as *mut BTreeMap<u32, (String, HWND)>) };
        if unsafe { IsWindowVisible(hwnd) }.as_bool()
            && let Ok((target, title)) = super::targets::target(hwnd)
            && !title.is_empty()
        {
            let path = PathBuf::from(target.app_identity.trim_start_matches("win32:"));
            let name = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            apps.entry(target.process_id).or_insert((name, hwnd));
        }
        BOOL(1)
    }
    let mut apps = BTreeMap::new();
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM((&mut apps as *mut BTreeMap<u32, (String, HWND)>) as isize),
        );
    }
    apps.into_values().collect()
}
pub fn windows_for_process(process: u32) -> Vec<HWND> {
    unsafe extern "system" fn collect(hwnd: HWND, context: LPARAM) -> BOOL {
        let (wanted, windows) = unsafe { &mut *(context.0 as *mut (u32, Vec<HWND>)) };
        let mut pid = 0;
        unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, Some(&mut pid));
        }
        if pid == *wanted && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            windows.push(hwnd);
        }
        BOOL(1)
    }
    let mut result = (process, vec![]);
    unsafe {
        let _ = EnumWindows(
            Some(collect),
            LPARAM((&mut result as *mut (u32, Vec<HWND>)) as isize),
        );
    }
    result.1
}
#[derive(Clone)]
struct Installed {
    name: String,
    parsing: String,
    executable: Option<String>,
    app_id: Option<String>,
}
thread_local! {
    static CATALOG: std::cell::RefCell<Option<(std::time::Instant, Vec<Installed>)>> = const { std::cell::RefCell::new(None) };
    static LAUNCHED: std::cell::RefCell<BTreeMap<String,(u32,u64)>> = const { std::cell::RefCell::new(BTreeMap::new()) };
}
fn owned_string(value: windows::core::PWSTR) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let result = unsafe { value.to_string().ok() };
    unsafe {
        windows::Win32::System::Com::CoTaskMemFree(Some(value.0.cast()));
    }
    result.filter(|s| !s.is_empty())
}
fn installed() -> Result<Vec<Installed>> {
    use windows::{
        Win32::{
            Storage::EnhancedStorage::{PKEY_AppUserModel_ID, PKEY_Link_TargetParsingPath},
            UI::Shell::*,
        },
        core::w,
    };
    if let Some(cached) = CATALOG.with(|c| {
        c.borrow()
            .as_ref()
            .filter(|(at, _)| at.elapsed().as_secs() < 60)
            .map(|(_, entries)| entries.clone())
    }) {
        return Ok(cached);
    }
    let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut entries = Vec::new();
    unsafe {
        let folder: IShellItem = SHCreateItemFromParsingName(w!("shell:AppsFolder"), None)
            .map_err(|_| "Windows installed-app catalog is unavailable.")?;
        let enumerator: IEnumShellItems = folder
            .BindToHandler(None, &BHID_EnumItems)
            .map_err(|_| "Could not enumerate installed applications.")?;
        loop {
            if entries.len() >= 4096 || std::time::Instant::now() > until {
                return Err(
                    "Installed-app discovery was incomplete. Retry or focus the app explicitly."
                        .into(),
                );
            }
            let mut items = [None];
            let mut fetched = 0;
            enumerator
                .Next(&mut items, Some(&mut fetched))
                .map_err(|_| "Installed-app enumeration failed.")?;
            if fetched == 0 {
                break;
            }
            if let Some(item) = items[0].take() {
                let name = item
                    .GetDisplayName(SIGDN_NORMALDISPLAY)
                    .ok()
                    .and_then(owned_string);
                let parsing = item
                    .GetDisplayName(SIGDN_DESKTOPABSOLUTEPARSING)
                    .ok()
                    .and_then(owned_string);
                let details: IShellItem2 = windows::core::Interface::cast(&item)
                    .map_err(|_| "App identity details unavailable.")?;
                let executable = details
                    .GetString(&PKEY_Link_TargetParsingPath)
                    .ok()
                    .and_then(owned_string)
                    .filter(|p| {
                        PathBuf::from(p).is_absolute() && p.to_lowercase().ends_with(".exe")
                    });
                let app_id = details
                    .GetString(&PKEY_AppUserModel_ID)
                    .ok()
                    .and_then(owned_string);
                if let (Some(name), Some(parsing)) = (name, parsing) {
                    entries.push(Installed {
                        name,
                        parsing,
                        executable,
                        app_id,
                    });
                }
            }
        }
    }
    entries.sort_by(|a, b| a.parsing.cmp(&b.parsing));
    entries.dedup_by(|a, b| a.parsing == b.parsing);
    CATALOG.with(|c| *c.borrow_mut() = Some((std::time::Instant::now(), entries.clone())));
    Ok(entries)
}
fn selected(name: &str) -> Result<Option<Installed>> {
    let normalized = name.trim().to_lowercase();
    let matches = installed()?
        .into_iter()
        .filter(|app| {
            app.name.to_lowercase() == normalized
                || app.parsing.eq_ignore_ascii_case(name)
                || app
                    .app_id
                    .as_ref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(name))
        })
        .collect::<Vec<_>>();
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        _ => Err(
            "Several installed apps match. Use the exact application identity from context.".into(),
        ),
    }
}
pub fn context_apps() -> Vec<fnpilot_core::context::AppInfo> {
    installed()
        .unwrap_or_default()
        .into_iter()
        .map(|app| fnpilot_core::context::AppInfo {
            name: app.name,
            bundle_id: app.app_id.unwrap_or(app.parsing),
        })
        .collect()
}
fn process_app_id(pid: u32) -> Option<String> {
    use windows::Win32::{
        Foundation::CloseHandle,
        Storage::Packaging::Appx::GetApplicationUserModelId,
        System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut length = 0;
        let _ = GetApplicationUserModelId(process, &mut length, None);
        let result = if length > 0 && length < 32768 {
            let mut text = vec![0u16; length as usize];
            if GetApplicationUserModelId(
                process,
                &mut length,
                Some(windows::core::PWSTR(text.as_mut_ptr())),
            )
            .is_ok()
            {
                Some(String::from_utf16_lossy(
                    &text[..length.saturating_sub(1) as usize],
                ))
            } else {
                None
            }
        } else {
            None
        };
        let _ = CloseHandle(process);
        result
    }
}
fn process_start(pid: u32) -> Option<u64> {
    use windows::Win32::{
        Foundation::{CloseHandle, FILETIME},
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let (mut start, mut end, mut kernel, mut user) = (
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
            FILETIME::default(),
        );
        let result = GetProcessTimes(process, &mut start, &mut end, &mut kernel, &mut user)
            .ok()
            .map(|_| ((start.dwHighDateTime as u64) << 32) | start.dwLowDateTime as u64);
        let _ = CloseHandle(process);
        result
    }
}
pub fn resolve(name: &str) -> Result<HWND> {
    let normalized = name.trim().to_lowercase();
    let selected = selected(name)?;
    let launched = LAUNCHED.with(|m| m.borrow().get(&normalized).copied());
    let matches = running()
        .into_iter()
        .filter(|(label, window)| {
            let Ok((target, _)) = super::targets::target(*window) else {
                return false;
            };
            label.to_lowercase() == normalized
                || launched == Some((target.process_id, target.process_start))
                || selected.as_ref().is_some_and(|app| {
                    app.executable.as_ref().is_some_and(|exe| {
                        target
                            .app_identity
                            .trim_start_matches("win32:")
                            .eq_ignore_ascii_case(exe)
                    }) || app
                        .app_id
                        .as_ref()
                        .is_some_and(|id| process_app_id(target.process_id).as_ref() == Some(id))
                })
        })
        .collect::<Vec<_>>();
    match matches.len() {
        1 => Ok(matches[0].1),
        0 => Err("That app has no accessible running window.".into()),
        _ => Err(
            "Several running apps match. Close duplicate instances or clarify the target.".into(),
        ),
    }
}
pub fn launch(name: &str) -> Result<()> {
    use windows::{
        Win32::{
            Foundation::CloseHandle,
            System::{
                Com::{CLSCTX_LOCAL_SERVER, CoCreateInstance},
                Threading::GetProcessId,
            },
            UI::Shell::*,
        },
        core::{PCWSTR, w},
    };
    let app = selected(name)?.ok_or("No installed application matches that exact name.")?;
    let mut pid = 0;
    unsafe {
        if let Some(id) = app.app_id.as_ref().filter(|id| id.contains('!')) {
            let manager: IApplicationActivationManager =
                CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_LOCAL_SERVER)
                    .map_err(|_| "Packaged app activation is unavailable.")?;
            let id: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
            pid = manager
                .ActivateApplication(PCWSTR(id.as_ptr()), w!(""), AO_NONE)
                .map_err(|_| "Could not activate the selected packaged app.")?;
        } else {
            let parsing: Vec<u16> = app.parsing.encode_utf16().chain(Some(0)).collect();
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(parsing.as_ptr()), None)
                .map_err(|_| "Installed app identity changed.")?;
            let item_id =
                SHGetIDListFromObject(&item).map_err(|_| "App launch identity unavailable.")?;
            let mut info = SHELLEXECUTEINFOW {
                cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                fMask: SEE_MASK_IDLIST | SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI,
                lpVerb: w!("open"),
                lpIDList: item_id.cast(),
                nShow: 1,
                ..Default::default()
            };
            let result = ShellExecuteExW(&mut info);
            windows::Win32::System::Com::CoTaskMemFree(Some(item_id.cast()));
            result.map_err(|_| "Could not launch the selected installed application.")?;
            if !info.hProcess.is_invalid() {
                pid = GetProcessId(info.hProcess);
                let _ = CloseHandle(info.hProcess);
            }
        }
    }
    if pid != 0 {
        LAUNCHED.with(|m| {
            m.borrow_mut().insert(
                name.trim().to_lowercase(),
                (pid, process_start(pid).unwrap_or(0)),
            )
        });
    }
    Ok(())
}
