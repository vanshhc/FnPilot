use fnpilot_core::Result;
use std::{
    sync::{OnceLock, mpsc},
    time::{Duration, Instant},
};
use windows::{
    Win32::{Foundation::RECT, Graphics::Gdi::*, UI::WindowsAndMessaging::*},
    core::{PCWSTR, w},
};

pub fn show(message: Option<String>, epoch: u64) -> Result<()> {
    type Update = (Option<String>, u64);
    static WORKER: OnceLock<mpsc::Sender<Update>> = OnceLock::new();
    let worker = WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Update>();
        std::thread::spawn(move || unsafe {
            let Ok(window) = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
                w!("STATIC"),
                w!("FnPilot"),
                WS_POPUP | WS_BORDER | WINDOW_STYLE(1 | 0x80), // SS_CENTER | SS_NOPREFIX
                GetSystemMetrics(SM_CXSCREEN) / 2 - 120,
                GetSystemMetrics(SM_CYSCREEN) - 100,
                240,
                42,
                None,
                None,
                None,
                None,
            ) else {
                return;
            };
            let mut dismiss = None;
            loop {
                match rx.recv_timeout(Duration::from_millis(16)) {
                    Ok((message, epoch))
                        if super::EPOCH.load(std::sync::atomic::Ordering::SeqCst) == epoch =>
                    {
                        if let Some(message) = message {
                            let width = if message.contains('\n') {
                                560.min(GetSystemMetrics(SM_CXSCREEN) - 48)
                            } else {
                                240
                            };
                            let mut measured: Vec<u16> = message.encode_utf16().collect();
                            let dc = GetDC(Some(window));
                            let mut bounds = RECT {
                                right: width - 24,
                                ..Default::default()
                            };
                            DrawTextW(
                                dc,
                                &mut measured,
                                &mut bounds,
                                DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX,
                            );
                            ReleaseDC(Some(window), dc);
                            let height = (bounds.bottom - bounds.top + 24)
                                .max(42)
                                .min(GetSystemMetrics(SM_CYSCREEN) - 100);
                            let _ = SetWindowPos(
                                window,
                                None,
                                (GetSystemMetrics(SM_CXSCREEN) - width) / 2,
                                GetSystemMetrics(SM_CYSCREEN) - height - 58,
                                width,
                                height,
                                SWP_NOACTIVATE | SWP_NOZORDER,
                            );
                            let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
                            let _ = SetWindowTextW(window, PCWSTR(text.as_ptr()));
                            let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
                            dismiss = match message.lines().next().unwrap_or_default() {
                                "Ready" | "Done" if message.contains('\n') => {
                                    Some(Instant::now() + Duration::from_secs(5))
                                }
                                "Ready" | "Done" => {
                                    Some(Instant::now() + Duration::from_millis(1200))
                                }
                                "Could not finish" => Some(Instant::now() + Duration::from_secs(5)),
                                _ => None,
                            };
                        } else {
                            let _ = ShowWindow(window, SW_HIDE);
                            dismiss = None;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    _ => {}
                }
                if dismiss.is_some_and(|deadline| Instant::now() >= deadline) {
                    let _ = ShowWindow(window, SW_HIDE);
                    dismiss = None;
                }
                let mut message = MSG::default();
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            let _ = DestroyWindow(window);
        });
        tx
    });
    worker
        .send((message, epoch))
        .map_err(|_| "The native overlay is unavailable.".into())
}
