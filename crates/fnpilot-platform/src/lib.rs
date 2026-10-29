#[cfg(any(target_os = "macos", target_os = "windows"))]
mod capture;
mod screen;
pub use fnpilot_core::runner::{Observation, Outcome, Platform, Target};
use fnpilot_core::{Action, Result};
use serde_json::Value;

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

pub struct NativePlatform;

impl fnpilot_core::computer::Computer for NativePlatform {
    fn capture(&mut self) -> Result<fnpilot_core::computer::Screen> {
        serde_json::from_value(request(serde_json::json!({"operation":"computerCapture"}))?)
            .map_err(|_| "Invalid native screenshot.".into())
    }
    fn act(
        &mut self,
        step: &fnpilot_core::computer::Step,
        expected: &fnpilot_core::computer::Screen,
    ) -> Result<()> {
        step.validate(expected)?;
        request(
            serde_json::json!({"operation":"computerAct","step":step,"frameID":expected.id,"target":expected.target}),
        )?;
        Ok(())
    }
}
pub fn current_epoch() -> u64 {
    #[cfg(target_os = "macos")]
    {
        macos::current_epoch()
    }
    #[cfg(target_os = "windows")]
    {
        windows::current_epoch()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        0
    }
}
pub fn invalidate() -> u64 {
    #[cfg(target_os = "macos")]
    {
        macos::invalidate()
    }
    #[cfg(target_os = "windows")]
    {
        windows::invalidate()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        0
    }
}
impl Platform for NativePlatform {
    fn observe(&mut self) -> Result<Observation> {
        serde_json::from_value(request(serde_json::json!({"operation":"observe"}))?)
            .map_err(|_| "Invalid native observation.".into())
    }
    fn execute(&mut self, action: &Action, expected: &Observation) -> Result<Outcome> {
        action.validate()?;
        serde_json::from_value(request(
            serde_json::json!({"operation":"execute","action":action,"expected":expected}),
        )?)
        .map_err(|_| "Invalid native outcome.".into())
    }
}

pub fn request(value: Value) -> Result<Value> {
    #[cfg(target_os = "macos")]
    {
        macos::request(value)
    }
    #[cfg(target_os = "windows")]
    {
        windows::request(value)
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = value;
        Err("Desktop execution supports macOS and Windows only.".into())
    }
}

pub fn key_events() -> Result<std::sync::mpsc::Receiver<(i32, i32, i32)>> {
    #[cfg(target_os = "macos")]
    {
        macos::key_events()
    }
    #[cfg(target_os = "windows")]
    {
        windows::key_events()
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err("Native hotkeys are unavailable on this platform.".into())
    }
}
