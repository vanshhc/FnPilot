use fnpilot_core::Result;
use objc2::{
    class, msg_send,
    rc::Retained,
    runtime::{AnyObject, Bool},
};
use objc2_foundation::NSString;
use serde_json::{Value, json};
use std::time::Instant;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    pub fn CGPreflightListenEventAccess() -> bool;
    pub fn CGRequestListenEventAccess() -> bool;
}
pub fn microphone_authorized() -> bool {
    unsafe {
        let status: isize = msg_send![class!(AVCaptureDevice),authorizationStatusForMediaType:&*NSString::from_str("soun")];
        status == 3
    }
}
pub fn report() -> Value {
    json!({"accessibility":super::ax::trusted(),"inputMonitoring":unsafe{CGPreflightListenEventAccess()},"microphone":microphone_authorized(),"screenRecording":unsafe{super::computer::CGPreflightScreenCaptureAccess()},"lastControlScan":super::LAST_SCAN.lock().unwrap().clone()})
}
pub fn request_microphone(deadline: Instant, epoch: u64) -> Result<Value> {
    let (tx, rx) = std::sync::mpsc::channel();
    super::on_main(deadline, epoch, move || unsafe {
        let block = block2::RcBlock::new(move |granted: Bool| {
            let _ = tx.send(granted.as_bool());
        });
        let _: () = msg_send![class!(AVCaptureDevice),requestAccessForMediaType:&*NSString::from_str("soun"), completionHandler:&*block];
        Ok(())
    })?;
    loop {
        if Instant::now() > deadline {
            return Err("Microphone approval is still pending. Complete the macOS prompt, then check permissions again.".into());
        }
        super::check(deadline, epoch)?;
        match rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(granted) => return Ok(json!(granted)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err("Microphone permission request failed.".into()),
        }
    }
}
pub fn request_accessibility(deadline: Instant, epoch: u64) -> Result<Value> {
    super::on_main(deadline, epoch, || {
        let trusted = super::ax::request_trust();
        let input = unsafe { CGRequestListenEventAccess() };
        Ok(
            json!({"accessibility":trusted,"inputMonitoring":input,"instructions":"Enable FnPilot in Accessibility and Input Monitoring. If a rebuilt preview still reports no access with the switches on, remove the old FnPilot Preview entries and add this app bundle again. The listener retries after permission changes."}),
        )
    })
}
pub fn autostart(enabled: bool, deadline: Instant, epoch: u64) -> Result<Value> {
    super::on_main(deadline, epoch, move || unsafe {
        let service: Retained<AnyObject> = msg_send![class!(SMAppService), mainAppService];
        let mut error: *mut AnyObject = std::ptr::null_mut();
        let ok: bool = if enabled {
            msg_send![&*service,registerAndReturnError:&mut error]
        } else {
            msg_send![&*service,unregisterAndReturnError:&mut error]
        };
        if !ok {
            return Err("macOS rejected launch-at-login registration. Review Login Items in System Settings.".into());
        }
        let status: isize = msg_send![&*service, status];
        Ok(json!({"enabled":status==1,"approvalRequired":status==2}))
    })
}
pub fn legacy_settings() -> Value {
    unsafe {
        let allocated: objc2::rc::Allocated<AnyObject> = msg_send![class!(NSUserDefaults), alloc];
        let defaults: Option<Retained<AnyObject>> =
            msg_send![allocated,initWithSuiteName:&*NSString::from_str("com.fnpilot.mac")];
        let mut result = serde_json::Map::new();
        if let Some(defaults) = defaults {
            for key in [
                "allowModelActions",
                "speechModelFolder",
                "useBundledRuntime",
            ] {
                let value: Option<Retained<AnyObject>> =
                    msg_send![&*defaults,objectForKey:&*NSString::from_str(key)];
                if let Some(value) = value {
                    if let Some(text) = super::apps::string(&value) {
                        result.insert(key.into(), json!(text));
                    } else {
                        let is_number: bool = msg_send![&*value,isKindOfClass:class!(NSNumber)];
                        if is_number {
                            let raw = (&*value as *const AnyObject).cast();
                            let is_bool = core_foundation::base::CFGetTypeID(raw)
                                == core_foundation::boolean::CFBoolean::type_id();
                            if is_bool {
                                let flag: bool = msg_send![&*value, boolValue];
                                result.insert(key.into(), json!(flag));
                            }
                        }
                    }
                }
            }
            let data: Option<Retained<AnyObject>> =
                msg_send![&*defaults,dataForKey:&*NSString::from_str("modelConfiguration")];
            if let Some(data) = data {
                let length: usize = msg_send![&*data, length];
                let pointer: *const std::ffi::c_void = msg_send![&*data, bytes];
                if !pointer.is_null()
                    && length <= 65536
                    && let Ok(config) = serde_json::from_slice::<Value>(std::slice::from_raw_parts(
                        pointer.cast::<u8>(),
                        length,
                    ))
                {
                    if let Some(model) = config["model"].as_str() {
                        let path: Option<Retained<AnyObject>> = msg_send![&*defaults,stringForKey:&*NSString::from_str(&format!("gguf:{model}"))];
                        if let Some(path) = path.as_deref().and_then(super::apps::string) {
                            result.insert("ggufPath".into(), json!(path));
                        }
                    }
                    result.insert("modelConfiguration".into(), config);
                }
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            let base =
                std::path::PathBuf::from(home).join("Library/Application Support/FnPilot/Models");
            if base
                .join("models/openai/whisper-small.en/tokenizer.json")
                .is_file()
            {
                result.insert("tokenizerCache".into(), json!(base));
            }
        }
        Value::Object(result)
    }
}
use core_foundation::base::TCFType;
