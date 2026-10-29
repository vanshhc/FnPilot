use crate::Result;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Ready,
    Recording,
    Transcribing,
    Interpreting,
    Acting,
    Clarifying,
    Paused,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub version: u32,
    pub generation: u64,
    pub revision: u64,
    pub phase: Phase,
    pub message: String,
    pub action_count: usize,
}

#[derive(Clone)]
pub struct Session(Arc<Mutex<Snapshot>>);

impl Default for Session {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(Snapshot {
            version: 1,
            generation: 0,
            revision: 0,
            phase: Phase::Ready,
            message: "Ready".into(),
            action_count: 0,
        })))
    }
}
impl Session {
    pub fn snapshot(&self) -> Snapshot {
        self.0.lock().unwrap().clone()
    }
    pub fn begin(&self, phase: Phase) -> Result<u64> {
        let mut state = self.0.lock().unwrap();
        if state.phase == Phase::Paused {
            return Err("FnPilot is paused.".into());
        }
        state.generation = state
            .generation
            .checked_add(1)
            .expect("session generation exhausted");
        state.phase = phase;
        state.revision = state.revision.saturating_add(1);
        state.message = "Working".into();
        state.action_count = 0;
        Ok(state.generation)
    }
    pub fn check(&self, token: u64) -> Result<()> {
        let state = self.0.lock().unwrap();
        if state.generation != token
            || matches!(state.phase, Phase::Ready | Phase::Paused | Phase::Error)
        {
            Err("Session cancelled.".into())
        } else {
            Ok(())
        }
    }
    pub fn update(&self, token: u64, phase: Phase, message: impl Into<String>) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        if state.generation != token {
            return Err("Session cancelled.".into());
        }
        state.phase = phase;
        state.revision = state.revision.saturating_add(1);
        state.message = message.into();
        Ok(())
    }
    pub fn reserve_action(&self, token: u64) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        if state.generation != token || !matches!(state.phase, Phase::Acting | Phase::Interpreting)
        {
            return Err("Session cancelled.".into());
        }
        if state.action_count >= 20 {
            return Err("Stopped after 20 actions. Give a shorter instruction.".into());
        }
        state.action_count += 1;
        state.revision = state.revision.saturating_add(1);
        Ok(())
    }
    pub fn carry_budget(&self, token: u64, used: usize) -> Result<()> {
        let mut state = self.0.lock().unwrap();
        if state.generation != token || used > 20 {
            return Err("Invalid continuation budget.".into());
        }
        state.action_count = used;
        state.revision = state.revision.saturating_add(1);
        Ok(())
    }
    pub fn cancel(&self, pause: bool) {
        let mut state = self.0.lock().unwrap();
        let pause = pause || state.phase == Phase::Paused;
        state.generation = state
            .generation
            .checked_add(1)
            .expect("session generation exhausted");
        state.phase = if pause { Phase::Paused } else { Phase::Ready };
        state.revision = state.revision.saturating_add(1);
        state.message = if pause { "Paused" } else { "Cancelled" }.into();
    }
    pub fn resume(&self) {
        let mut state = self.0.lock().unwrap();
        if state.phase == Phase::Paused {
            state.phase = Phase::Ready;
            state.revision = state.revision.saturating_add(1);
            state.message = "Ready".into();
        }
    }
}
