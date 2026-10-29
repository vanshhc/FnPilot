use fnpilot_core::Result;
use serde_json::Value;
use std::{
    collections::HashMap,
    ffi::{CStr, CString, c_char},
    sync::{
        LazyLock, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

type Pending = HashMap<u64, mpsc::Sender<Result<Value>>>;
static PENDING: LazyLock<Mutex<Pending>> = LazyLock::new(|| Mutex::new(HashMap::new()));
static NEXT: AtomicU64 = AtomicU64::new(1);
unsafe extern "C" {
    fn fnpilot_speech_request(
        json: *const c_char,
        id: u64,
        callback: extern "C" fn(u64, *const c_char),
    );
}
extern "C" fn reply(id: u64, json: *const c_char) {
    if json.is_null() {
        return;
    }
    // Swift owns this string for the duration of the callback. Copy it before returning.
    let data = unsafe { CStr::from_ptr(json) }.to_bytes();
    let result = serde_json::from_slice::<Value>(data)
        .map_err(|_| "Invalid speech binding JSON.".into())
        .and_then(|v| {
            if let Some(error) = v["error"].as_str() {
                Err(error.into())
            } else {
                v.get("ok").cloned().ok_or("Missing speech result.".into())
            }
        });
    if let Some(sender) = PENDING.lock().unwrap().remove(&id) {
        let _ = sender.send(result);
    }
}
pub fn request(value: Value) -> Result<Value> {
    let mut value = value;
    let timeout = match value["operation"].as_str() {
        Some("transcribeWhisperKit") => 120,
        Some("unloadSpeech") => 8,
        _ => return Err("Unknown speech operation.".into()),
    };
    let epoch = value
        .get("cancellationEpoch")
        .and_then(Value::as_u64)
        .unwrap_or_else(super::current_epoch);
    // Only decoder inputs can cross into Swift; never forward actions or app context.
    value = match value["operation"].as_str() {
        Some("transcribeWhisperKit") => {
            serde_json::json!({"operation":"transcribeWhisperKit","epoch":epoch,"folder":value["folder"],"tokenizerBase":value["tokenizerBase"],"samples":value["samples"]})
        }
        Some("unloadSpeech") => serde_json::json!({"operation":"unloadSpeech","epoch":epoch}),
        _ => unreachable!(),
    };
    let json = CString::new(value.to_string()).map_err(|_| "Invalid native request.")?;
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = mpsc::channel();
    PENDING.lock().unwrap().insert(id, tx);
    unsafe {
        fnpilot_speech_request(json.as_ptr(), id, reply);
    }
    let deadline = Instant::now() + Duration::from_secs(timeout);
    let result = loop {
        if super::current_epoch() != epoch {
            break Err("Native operation was cancelled.".to_string());
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(value) => break Ok(value),
            Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() < deadline => continue,
            Err(_) => {
                // An old timeout must never invalidate a newer recording/session.
                super::expire(epoch);
                break Err("Native operation timed out. Session stopped.".to_string());
            }
        }
    };
    PENDING.lock().unwrap().remove(&id);
    result?
}
