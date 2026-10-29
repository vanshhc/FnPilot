use fnpilot_core::{Result, runner::Target};
use windows::{
    Win32::{
        Foundation::{CloseHandle, FILETIME, HANDLE, HWND},
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::Threading::{
            GetProcessTimes, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
        UI::WindowsAndMessaging::{GetWindowTextW, GetWindowThreadProcessId},
    },
    core::PWSTR,
};

struct Owned(HANDLE);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn target(hwnd: HWND) -> Result<(Target, String)> {
    unsafe {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return Err("No ordinary desktop target is available.".into());
        }
        let process = Owned(
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(
                |_| "Protected or inaccessible target. FnPilot does not request elevation.",
            )?,
        );
        let mut token = HANDLE::default();
        OpenProcessToken(process.0, TOKEN_QUERY, &mut token)
            .map_err(|_| "Target permissions could not be inspected.")?;
        let token = Owned(token);
        let mut elevation = TOKEN_ELEVATION::default();
        let mut needed = 0;
        GetTokenInformation(
            token.0,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut needed,
        )
        .map_err(|_| "Target integrity could not be inspected.")?;
        if elevation.TokenIsElevated != 0 {
            return Err("Elevated applications and secure desktops are unsupported. Use an ordinary-user target.".into());
        }
        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        GetProcessTimes(process.0, &mut created, &mut exited, &mut kernel, &mut user)
            .map_err(|_| "Target process identity is unavailable.")?;
        let mut path = vec![0u16; 32768];
        let mut size = path.len() as u32;
        QueryFullProcessImageNameW(
            process.0,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut size,
        )
        .map_err(|_| "Target application identity is unavailable.")?;
        let mut title = vec![0u16; 1024];
        let n = GetWindowTextW(hwnd, &mut title);
        Ok((
            Target {
                platform: "windows".into(),
                app_identity: format!("win32:{}", String::from_utf16_lossy(&path[..size as usize])),
                process_id: pid,
                process_start: ((created.dwHighDateTime as u64) << 32)
                    | created.dwLowDateTime as u64,
                window_id: hwnd.0 as u64,
                generation: 0,
            },
            String::from_utf16_lossy(&title[..n.max(0) as usize]),
        ))
    }
}
