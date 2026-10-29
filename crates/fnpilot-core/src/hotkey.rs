#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Start,
    Finish,
    Cancel,
}

#[derive(Default)]
pub struct HotkeyState {
    down: bool,
    recording: bool,
}
impl HotkeyState {
    pub fn flags(&mut self, held: bool, other_modifier: bool) -> Option<Signal> {
        if held && !self.down {
            self.down = true;
            if !other_modifier {
                self.recording = true;
                return Some(Signal::Start);
            }
        } else if !held && self.down {
            self.down = false;
            if self.recording {
                self.recording = false;
                return Some(Signal::Finish);
            }
        }
        if held && other_modifier && self.recording {
            self.recording = false;
            return Some(Signal::Cancel);
        }
        None
    }
    pub fn key_down(&mut self, escape: bool) -> Option<Signal> {
        if self.recording {
            self.recording = false;
            return Some(Signal::Cancel);
        }
        if escape { Some(Signal::Cancel) } else { None }
    }
    pub fn reset(&mut self) {
        self.down = false;
        self.recording = false;
    }
}
