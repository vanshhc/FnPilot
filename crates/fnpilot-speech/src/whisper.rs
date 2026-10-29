use crate::{
    backend::{SpeechBackend, validate_audio},
    model_store::ModelManifest,
};
use fnpilot_core::{Result, session::Session};
use std::path::Path;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

pub struct Whisper {
    context: WhisperContext,
}
struct Abort {
    session: Session,
    token: u64,
}
unsafe extern "C" fn abort(pointer: *mut std::ffi::c_void) -> bool {
    if pointer.is_null() {
        return true;
    }
    // This immutable state lives until synchronous full() returns; C does not retain it.
    let state = unsafe { &*pointer.cast::<Abort>() };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        state.session.check(state.token).is_err()
    }))
    .unwrap_or(true)
}
impl Whisper {
    pub fn load(path: &Path, manifest: &ModelManifest) -> Result<Self> {
        manifest.verify(path)?;
        let mut params = WhisperContextParameters::default();
        params.use_gpu(false);
        let context = WhisperContext::new_with_params(
            path.to_str().ok_or("Model path is not valid Unicode.")?,
            params,
        )
        .map_err(|_| "Could not load the selected whisper.cpp model.")?;
        Ok(Self { context })
    }
}
impl SpeechBackend for Whisper {
    fn transcribe(&mut self, samples: &[f32], session: &Session, token: u64) -> Result<String> {
        session.check(token)?;
        validate_audio(samples)?;
        let mut state = self
            .context
            .create_state()
            .map_err(|_| "Speech decoder initialization failed.")?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_n_threads(2);
        params.set_temperature(0.0);
        params.set_temperature_inc(0.0);
        params.set_no_context(true);
        params.set_no_timestamps(true);
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        let mut cancellation = Abort {
            session: session.clone(),
            token,
        };
        // Use an explicitly owned callback state. The pinned wrapper's closure helper
        // allocates erased callback data and does not retain it for destruction.
        unsafe {
            params.set_abort_callback(Some(abort));
            params.set_abort_callback_user_data((&mut cancellation as *mut Abort).cast());
        }
        state
            .full(params, samples)
            .map_err(|_| "Speech transcription failed or was cancelled.")?;
        session.check(token)?;
        let text = state
            .as_iter()
            .map(|segment| segment.to_string())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if text.is_empty() {
            Err("No speech was recognized.".into())
        } else {
            Ok(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abort_callback_tracks_generation_without_model_weights() {
        let session = Session::default();
        let token = session
            .begin(fnpilot_core::session::Phase::Transcribing)
            .unwrap();
        let mut state = Abort {
            session: session.clone(),
            token,
        };
        let pointer = (&mut state as *mut Abort).cast();
        assert!(!unsafe { abort(pointer) });
        session.cancel(false);
        assert!(unsafe { abort(pointer) });
    }
}
