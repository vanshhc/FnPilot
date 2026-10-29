use fnpilot_core::{Result, context::AppInfo};
use objc2::{class, msg_send, rc::Retained, runtime::AnyObject};
use objc2_foundation::NSString;
use std::{
    collections::BTreeMap,
    ffi::CStr,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Objective-C references are confined to the native worker/main queue; only Rust values cross.
pub struct App {
    object: Retained<AnyObject>,
}
pub fn string(object: &AnyObject) -> Option<String> {
    unsafe {
        let is_string: bool = msg_send![object,isKindOfClass:class!(NSString)];
        if !is_string {
            return None;
        }
        let bytes: *const std::ffi::c_char = msg_send![object, UTF8String];
        (!bytes.is_null()).then(|| CStr::from_ptr(bytes).to_string_lossy().into_owned())
    }
}
fn text(value: Option<Retained<AnyObject>>) -> String {
    value.as_deref().and_then(string).unwrap_or_default()
}
impl App {
    pub fn foreground() -> Option<Self> {
        unsafe {
            let workspace: Retained<AnyObject> = msg_send![class!(NSWorkspace), sharedWorkspace];
            let object: Option<Retained<AnyObject>> = msg_send![&*workspace, frontmostApplication];
            object.map(|object| Self { object })
        }
    }
    pub fn for_pid(pid: u32) -> Option<Self> {
        unsafe {
            let object: Option<Retained<AnyObject>> = msg_send![class!(NSRunningApplication),runningApplicationWithProcessIdentifier:pid as i32];
            object.map(|object| Self { object })
        }
    }
    pub fn for_identity(pid: u32, start: u64) -> Option<Self> {
        Self::for_pid(pid).filter(|app| start != 0 && app.start() == start && !app.terminated())
    }
    pub fn running(id: &str) -> Option<Self> {
        unsafe {
            let array: Retained<AnyObject> = msg_send![class!(NSRunningApplication),runningApplicationsWithBundleIdentifier:&*NSString::from_str(id)];
            let count: usize = msg_send![&*array, count];
            if count == 0 {
                None
            } else {
                let object: Retained<AnyObject> = msg_send![&*array,objectAtIndex:0usize];
                Some(Self { object })
            }
        }
    }
    pub fn pid(&self) -> u32 {
        unsafe {
            let pid: i32 = msg_send![&*self.object, processIdentifier];
            pid as u32
        }
    }
    pub fn id(&self) -> String {
        unsafe { text(msg_send![&*self.object, bundleIdentifier]) }
    }
    pub fn name(&self) -> String {
        unsafe { text(msg_send![&*self.object, localizedName]) }
    }
    pub fn start(&self) -> u64 {
        unsafe {
            let date: Option<Retained<AnyObject>> = msg_send![&*self.object, launchDate];
            date.map(|date| {
                let value: f64 = msg_send![&*date, timeIntervalSince1970];
                (value * 1000.).max(0.) as u64
            })
            .unwrap_or(0)
        }
    }
    pub fn terminated(&self) -> bool {
        unsafe { msg_send![&*self.object, isTerminated] }
    }
    pub fn bundle_path(&self) -> Option<PathBuf> {
        unsafe {
            let url: Option<Retained<AnyObject>> = msg_send![&*self.object, bundleURL];
            let path: Option<Retained<AnyObject>> = msg_send![&*url?, path];
            path.as_deref().and_then(string).map(PathBuf::from)
        }
    }
    pub fn terminate(&self) -> bool {
        unsafe { msg_send![&*self.object, terminate] }
    }
}
pub struct Catalog {
    entries: BTreeMap<String, (AppInfo, PathBuf)>,
    refreshed: Instant,
    complete: bool,
}
impl Default for Catalog {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            refreshed: Instant::now() - Duration::from_secs(120),
            complete: false,
        }
    }
}
impl Catalog {
    pub fn refresh(&mut self) {
        if self.refreshed.elapsed() < Duration::from_secs(60) {
            return;
        }
        let mut roots = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/System/Applications"),
            PathBuf::from("/System/Library/CoreServices"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join("Applications"));
        }
        let mut entries = BTreeMap::new();
        let mut remaining = 5000;
        for root in roots {
            scan(&root, 0, &mut remaining, &mut entries);
        }
        self.complete = remaining > 0;
        self.entries = entries;
        self.refreshed = Instant::now();
    }
    pub fn infos(&mut self) -> Vec<AppInfo> {
        self.refresh();
        let mut result = self
            .entries
            .values()
            .map(|(i, _)| i.clone())
            .collect::<Vec<_>>();
        result.sort_by_key(|a| a.name.to_lowercase());
        result
    }
    pub fn resolve(&mut self, name: &str) -> Result<(AppInfo, PathBuf)> {
        self.refresh();
        if !self.complete {
            return Err(
                "The installed application scan reached its safety limit; app resolution stopped."
                    .into(),
            );
        }
        let query = name.trim().to_lowercase();
        let alias = match query.as_str() {
            "chrome" => "com.google.Chrome",
            "notes" => "com.apple.Notes",
            "safari" => "com.apple.Safari",
            "finder" => "com.apple.finder",
            "vs code" | "vscode" => "com.microsoft.VSCode",
            "system settings" => "com.apple.systempreferences",
            _ => "",
        };
        if let Some(entry) = self.entries.get(alias) {
            return Ok(entry.clone());
        }
        let exact = self
            .entries
            .values()
            .filter(|(i, _)| {
                i.name.eq_ignore_ascii_case(name) || i.bundle_id.eq_ignore_ascii_case(name)
            })
            .collect::<Vec<_>>();
        if exact.len() == 1 {
            return Ok(exact[0].clone());
        }
        let partial = self
            .entries
            .values()
            .filter(|(i, _)| i.name.to_lowercase().contains(&query))
            .collect::<Vec<_>>();
        if exact.is_empty() && partial.len() == 1 {
            return Ok(partial[0].clone());
        }
        Err(if exact.len() > 1 || partial.len() > 1 {
            "Several apps match. Use a full application name or bundle identifier."
        } else {
            "No installed app matches that name."
        }
        .into())
    }
}
fn scan(
    root: &Path,
    depth: usize,
    remaining: &mut usize,
    entries: &mut BTreeMap<String, (AppInfo, PathBuf)>,
) {
    if depth > 6 || *remaining == 0 {
        return;
    }
    let Ok(walker) = std::fs::read_dir(root) else {
        return;
    };
    for item in walker.flatten() {
        if *remaining == 0 {
            return;
        }
        *remaining -= 1;
        let path = item.path();
        if item.file_name().to_string_lossy().starts_with('.')
            || item
                .file_type()
                .map_or(true, |t| t.is_symlink() || !t.is_dir())
        {
            continue;
        }
        if path.extension().is_some_and(|e| e == "app") {
            if let Some(info) = bundle_info(&path) {
                entries
                    .entry(info.bundle_id.clone())
                    .or_insert((info, path));
            }
        } else {
            scan(&path, depth + 1, remaining, entries);
        }
    }
}
fn bundle_info(path: &Path) -> Option<AppInfo> {
    unsafe {
        let bundle: Option<Retained<AnyObject>> =
            msg_send![class!(NSBundle),bundleWithPath:&*NSString::from_str(path.to_str()?)];
        let bundle = bundle?;
        let id = text(msg_send![&*bundle, bundleIdentifier]);
        if id.is_empty() {
            return None;
        }
        let mut name = String::new();
        for key in ["CFBundleDisplayName", "CFBundleName"] {
            name = text(msg_send![&*bundle,objectForInfoDictionaryKey:&*NSString::from_str(key)]);
            if !name.is_empty() {
                break;
            }
        }
        if name.is_empty() {
            name = path.file_stem()?.to_string_lossy().into_owned();
        }
        Some(AppInfo {
            name,
            bundle_id: id,
        })
    }
}
pub fn launch_or_activate(
    id: String,
    path: PathBuf,
    launch: bool,
    deadline: Instant,
    epoch: u64,
) -> Result<()> {
    super::on_main(deadline, epoch, move || unsafe {
        let path = if let Some(app) = App::running(&id) {
            if App::foreground()
                .is_some_and(|front| front.pid() == app.pid() && front.start() == app.start())
            {
                return Ok(());
            }
            app.bundle_path()
                .ok_or("The running app's bundle path is unavailable.")?
        } else if launch {
            path
        } else {
            return Err("That app is not running. Say open to launch it.".into());
        };
        let workspace: Retained<AnyObject> = msg_send![class!(NSWorkspace), sharedWorkspace];
        let url: Retained<AnyObject> =
            msg_send![class!(NSURL),fileURLWithPath:&*NSString::from_str(&path.to_string_lossy())];
        let config: Retained<AnyObject> = msg_send![class!(NSWorkspaceOpenConfiguration), new];
        let _: () = msg_send![&*config,setActivates:true];
        let _: () = msg_send![&*config,setCreatesNewApplicationInstance:false];
        let completed = block2::RcBlock::new(|_app: *mut AnyObject, _error: *mut AnyObject| {});
        let _: () = msg_send![&*workspace,openApplicationAtURL:&*url, configuration:&*config, completionHandler:&*completed];
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn system_application_catalog_and_process_metadata_are_read_only() {
        objc2::rc::autoreleasepool(|_| {
            let mut catalog = Catalog::default();
            assert!(!catalog.infos().is_empty());
            let (calculator, path) = catalog.resolve("com.apple.calculator").unwrap();
            assert_eq!(calculator.bundle_id, "com.apple.calculator");
            assert!(path.is_dir());
            if let Some(app) = App::for_pid(std::process::id()) {
                assert_eq!(app.pid(), std::process::id());
                assert!(!app.terminated());
                let _ = (app.name(), app.id(), app.start());
                assert!(App::for_identity(app.pid(), 0).is_none());
                assert!(App::for_identity(app.pid(), app.start().wrapping_add(1)).is_none());
                if app.start() != 0 {
                    assert!(App::for_identity(app.pid(), app.start()).is_some());
                }
            }
        });
    }
}
