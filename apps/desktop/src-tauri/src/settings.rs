use fnpilot_core::Result;
use fnpilot_runtime::Configuration;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SpeechBackend {
    Portable,
    WhisperKit,
}
impl Default for SpeechBackend {
    fn default() -> Self {
        if cfg!(target_os = "macos") {
            Self::WhisperKit
        } else {
            Self::Portable
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Settings {
    pub schema_version: u32,
    pub allow_model_actions: bool,
    pub allow_computer_use: bool,
    pub computer_use_model: String,
    pub configuration: Configuration,
    pub gguf_path: Option<PathBuf>,
    pub speech_model_path: Option<PathBuf>,
    pub speech_backend: SpeechBackend,
    pub tokenizer_cache: Option<PathBuf>,
    pub talk_key: String,
    pub legacy_speech_cache: Option<PathBuf>,
    pub launch_at_login_intent: bool,
    pub prefer_bundled_runtime: bool,
    pub import_version: u32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: 1,
            allow_model_actions: false,
            allow_computer_use: false,
            computer_use_model: String::new(),
            configuration: Configuration::default(),
            gguf_path: None,
            speech_model_path: None,
            speech_backend: SpeechBackend::default(),
            tokenizer_cache: None,
            talk_key: if cfg!(target_os = "macos") {
                "fn"
            } else {
                "rightCtrl"
            }
            .into(),
            legacy_speech_cache: None,
            launch_at_login_intent: false,
            prefer_bundled_runtime: false,
            import_version: 0,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1 || self.import_version > 1 {
            return Err("Unsupported settings schema.".into());
        }
        self.configuration.validate()?;
        if self.allow_computer_use {
            if self.computer_use_model.trim().is_empty() {
                return Err("Select a locally installed vision model for computer use.".into());
            }
            let mut vision = self.configuration.clone();
            vision.model = self.computer_use_model.clone();
            vision.validate()?;
        }
        if !["fn", "rightCtrl", "rightAlt", "f8", "f9"].contains(&self.talk_key.as_str())
            || cfg!(target_os = "macos") && self.talk_key != "fn"
            || cfg!(target_os = "windows") && self.talk_key == "fn"
        {
            return Err("Choose a supported hold-to-talk key for this platform.".into());
        }
        if cfg!(target_os = "windows") && self.speech_backend == SpeechBackend::WhisperKit {
            return Err("WhisperKit is available only on macOS.".into());
        }
        for path in [&self.gguf_path, &self.speech_model_path]
            .into_iter()
            .flatten()
        {
            if !path.is_absolute() || !path.is_file() {
                return Err(
                    "Selected model file is missing. Choose an existing absolute file path.".into(),
                );
            }
        }
        for path in [&self.legacy_speech_cache, &self.tokenizer_cache]
            .into_iter()
            .flatten()
        {
            if !path.is_absolute() || !path.is_dir() {
                return Err("Selected WhisperKit model or tokenizer directory is missing.".into());
            }
        }
        Ok(())
    }
}
pub fn load(path: &Path) -> Result<Settings> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let bytes = fs::read(path).map_err(|_| "Could not read settings.")?;
    let settings: Settings = serde_json::from_slice(&bytes)
        .map_err(|_| "Settings are invalid. Restore the backup before saving.")?;
    if settings.schema_version != 1 || settings.import_version > 1 {
        return Err("Unsupported settings schema.".into());
    }
    Ok(settings)
}
pub fn save(path: &Path, settings: &Settings) -> Result<()> {
    settings.validate()?;
    let parent = path.parent().ok_or("Invalid settings location.")?;
    fs::create_dir_all(parent).map_err(|_| "Could not create settings directory.")?;
    let temporary = path.with_extension("partial");
    let mut file = File::options()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| "Settings staging file exists. Recover it before saving.")?;
    let result = (|| {
        file.write_all(
            &serde_json::to_vec_pretty(settings).map_err(|_| "Could not encode settings.")?,
        )
        .map_err(|_| "Could not write settings.")?;
        file.sync_all().map_err(|_| "Could not flush settings.")?;
        drop(file);
        if path.exists() {
            fs::copy(path, path.with_extension("backup.json"))
                .map_err(|_| "Could not back up existing settings.")?;
        }
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows::Win32::Storage::FileSystem::{
                MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            };
            use windows::core::PCWSTR;
            let src: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
            let dst: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            unsafe {
                MoveFileExW(
                    PCWSTR(src.as_ptr()),
                    PCWSTR(dst.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
            .map_err(|_| "Could not atomically replace settings.")?;
        }
        #[cfg(not(target_os = "windows"))]
        fs::rename(&temporary, path).map_err(|_| "Could not atomically replace settings.")?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
