#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Dictate,
    Translate { target: String },
    Ask,
    Rewrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Preparing,
    Recording,
    Finalizing,
    Transcribing,
    Transforming,
    ValidatingTarget,
    Inserting,
    Completed,
    ResultCard,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Start,
    MicReady,
    Stop,
    AudioFinalized,
    AsrDone { needs_llm: bool },
    LlmDone,
    TargetOk,
    TargetChanged,
    Delivered,
    DeliveryFailed,
    Dismiss,
    Cancel,
    Reset,
}

pub fn next(phase: Phase, event: Event) -> Option<Phase> {
    use {Event::*, Phase::*};
    match (phase, event) {
        (Idle, Start) => Some(Preparing),
        (Preparing, MicReady) => Some(Recording),
        (Recording, Stop) => Some(Finalizing),
        (Finalizing, AudioFinalized) => Some(Transcribing),
        (Transcribing, AsrDone { needs_llm: true }) => Some(Transforming),
        (Transcribing, AsrDone { needs_llm: false }) => Some(ValidatingTarget),
        (Transforming, LlmDone) => Some(ValidatingTarget),
        (ValidatingTarget, TargetOk) => Some(Inserting),
        (ValidatingTarget, TargetChanged) => Some(ResultCard),
        (Inserting, Delivered) => Some(Completed),
        (Inserting, DeliveryFailed) => Some(ResultCard),
        (Completed, Reset) => Some(Idle),
        (ResultCard, Dismiss) => Some(Idle),
        (Cancelled, Reset) => Some(Idle),
        (Preparing, Cancel) => Some(Cancelled),
        (Recording, Cancel) => Some(Cancelled),
        (Finalizing, Cancel) => Some(Cancelled),
        (Transcribing, Cancel) => Some(Cancelled),
        (Transforming, Cancel) => Some(Cancelled),
        _ => None,
    }
}

impl Phase {
    pub fn is_active(self) -> bool {
        matches!(
            self,
            Phase::Preparing
                | Phase::Recording
                | Phase::Finalizing
                | Phase::Transcribing
                | Phase::Transforming
                | Phase::ValidatingTarget
                | Phase::Inserting
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: u64,
    pub generation: u64,
    pub mode: Mode,
    pub phase: Phase,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoordError {
    #[error("a session is already active")]
    Busy,
    #[error("no session")]
    NoSession,
    #[error("stale generation")]
    Stale,
    #[error("invalid transition from {from:?} on {event:?}")]
    InvalidTransition { from: Phase, event: Event },
}

#[derive(Debug, Default)]
pub struct Coordinator {
    next_id: u64,
    generation: u64,
    current: Option<Session>,
}

impl Coordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> Option<&Session> {
        self.current.as_ref()
    }

    pub fn start(&mut self, mode: Mode) -> Result<Session, CoordError> {
        if let Some(session) = &self.current
            && session.phase.is_active()
        {
            return Err(CoordError::Busy);
        }
        self.next_id += 1;
        self.generation += 1;
        let session = Session {
            id: self.next_id,
            generation: self.generation,
            mode,
            phase: Phase::Preparing,
        };
        self.current = Some(session.clone());
        Ok(session)
    }

    pub fn apply(&mut self, generation: u64, event: Event) -> Result<Phase, CoordError> {
        let session = self.current.as_mut().ok_or(CoordError::NoSession)?;
        if generation != session.generation {
            return Err(CoordError::Stale);
        }
        let new_phase = next(session.phase, event).ok_or(CoordError::InvalidTransition {
            from: session.phase,
            event,
        })?;
        session.phase = new_phase;
        if event == Event::Cancel {
            self.generation += 1;
            session.generation = self.generation;
        }
        Ok(new_phase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_llm_ends_completed() {
        let mut c = Coordinator::new();
        let s = c.start(Mode::Dictate).unwrap();
        let g = s.generation;
        assert_eq!(c.apply(g, Event::MicReady).unwrap(), Phase::Recording);
        assert_eq!(c.apply(g, Event::Stop).unwrap(), Phase::Finalizing);
        assert_eq!(
            c.apply(g, Event::AudioFinalized).unwrap(),
            Phase::Transcribing
        );
        assert_eq!(
            c.apply(g, Event::AsrDone { needs_llm: true }).unwrap(),
            Phase::Transforming
        );
        assert_eq!(c.apply(g, Event::LlmDone).unwrap(), Phase::ValidatingTarget);
        assert_eq!(c.apply(g, Event::TargetOk).unwrap(), Phase::Inserting);
        assert_eq!(c.apply(g, Event::Delivered).unwrap(), Phase::Completed);
        assert_eq!(c.apply(g, Event::Reset).unwrap(), Phase::Idle);
    }

    #[test]
    fn raw_path_skips_transforming() {
        let mut c = Coordinator::new();
        let s = c.start(Mode::Dictate).unwrap();
        let g = s.generation;
        c.apply(g, Event::MicReady).unwrap();
        c.apply(g, Event::Stop).unwrap();
        c.apply(g, Event::AudioFinalized).unwrap();
        assert_eq!(
            c.apply(g, Event::AsrDone { needs_llm: false }).unwrap(),
            Phase::ValidatingTarget
        );
    }

    #[test]
    fn target_changed_goes_to_result_card() {
        let mut c = Coordinator::new();
        let s = c.start(Mode::Dictate).unwrap();
        let g = s.generation;
        c.apply(g, Event::MicReady).unwrap();
        c.apply(g, Event::Stop).unwrap();
        c.apply(g, Event::AudioFinalized).unwrap();
        c.apply(g, Event::AsrDone { needs_llm: false }).unwrap();
        assert_eq!(c.apply(g, Event::TargetChanged).unwrap(), Phase::ResultCard);
        assert_eq!(c.apply(g, Event::Dismiss).unwrap(), Phase::Idle);
    }

    #[test]
    fn stop_while_idle_is_invalid() {
        let mut c = Coordinator::new();
        assert_eq!(c.apply(1, Event::Start).unwrap_err(), CoordError::NoSession);
        let s = c.start(Mode::Dictate).unwrap();
        let g = s.generation;
        c.apply(g, Event::MicReady).unwrap();
        c.apply(g, Event::Stop).unwrap();
        c.apply(g, Event::AudioFinalized).unwrap();
        c.apply(g, Event::AsrDone { needs_llm: false }).unwrap();
        c.apply(g, Event::TargetOk).unwrap();
        c.apply(g, Event::Delivered).unwrap();
        c.apply(g, Event::Reset).unwrap();
        assert_eq!(c.current().unwrap().phase, Phase::Idle);
        assert_eq!(
            c.apply(g, Event::Stop).unwrap_err(),
            CoordError::InvalidTransition {
                from: Phase::Idle,
                event: Event::Stop
            }
        );
    }

    #[test]
    fn start_while_recording_is_busy() {
        let mut c = Coordinator::new();
        let s = c.start(Mode::Dictate).unwrap();
        let g = s.generation;
        c.apply(g, Event::MicReady).unwrap();
        assert_eq!(c.current().unwrap().phase, Phase::Recording);
        assert_eq!(c.start(Mode::Ask).unwrap_err(), CoordError::Busy);
    }

    #[test]
    fn cancel_then_old_gen_is_stale() {
        let mut c = Coordinator::new();
        let s = c.start(Mode::Dictate).unwrap();
        let g = s.generation;
        c.apply(g, Event::MicReady).unwrap();
        c.apply(g, Event::Stop).unwrap();
        c.apply(g, Event::AudioFinalized).unwrap();
        assert_eq!(c.apply(g, Event::Cancel).unwrap(), Phase::Cancelled);
        let new_gen = c.current().unwrap().generation;
        assert_ne!(new_gen, g);
        assert_eq!(
            c.apply(g, Event::AsrDone { needs_llm: false }).unwrap_err(),
            CoordError::Stale
        );
        assert_eq!(c.current().unwrap().phase, Phase::Cancelled);
    }

    #[test]
    fn reset_after_cancel_allows_new_start() {
        let mut c = Coordinator::new();
        let s = c.start(Mode::Dictate).unwrap();
        let g = s.generation;
        c.apply(g, Event::MicReady).unwrap();
        c.apply(g, Event::Stop).unwrap();
        c.apply(g, Event::AudioFinalized).unwrap();
        c.apply(g, Event::Cancel).unwrap();
        let g2 = c.current().unwrap().generation;
        assert_eq!(c.apply(g2, Event::Reset).unwrap(), Phase::Idle);
        let s2 = c.start(Mode::Ask).unwrap();
        assert_eq!(s2.phase, Phase::Preparing);
        assert_ne!(s2.generation, g);
    }
}
