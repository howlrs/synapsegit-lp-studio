use std::collections::{HashMap, HashSet, VecDeque};

use tokio::sync::watch;

pub(super) const MAX_TERMINAL_ATTEMPTS: usize = 512;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct AttemptGeneration(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AttemptStatus {
    Queued,
    Running,
    Cancelled,
    TimedOut,
    ProviderFailed,
    ValidationFailed,
    ProposalReady,
}

impl AttemptStatus {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::ProviderFailed => "provider_failed",
            Self::ValidationFailed => "validation_failed",
            Self::ProposalReady => "proposal_ready",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ClaimError {
    ProjectBusy,
    AttemptAlreadyFinished,
    GenerationExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CancelResult {
    Cancelled,
    AlreadyCancelled,
    NotActive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CompleteResult {
    Recorded,
    Cancelled,
    NotActive,
}

#[derive(Debug)]
pub(super) struct Cancellation {
    receiver: watch::Receiver<bool>,
    generation: AttemptGeneration,
}

impl Cancellation {
    pub(super) const fn generation(&self) -> AttemptGeneration {
        self.generation
    }

    pub(super) fn is_cancelled(&self) -> bool {
        *self.receiver.borrow()
    }

    pub(super) async fn cancelled(&mut self) {
        if self.is_cancelled() {
            return;
        }
        while self.receiver.changed().await.is_ok() {
            if self.is_cancelled() {
                return;
            }
        }
    }
}

struct ActiveAttempt {
    attempt_id: String,
    generation: AttemptGeneration,
    cancellation: watch::Sender<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TerminalAttempt {
    project_id: String,
    attempt_id: String,
    status: AttemptStatus,
}

#[derive(Default)]
pub(super) struct AiAttempts {
    active_by_project: HashMap<String, ActiveAttempt>,
    queued: HashSet<(String, String)>,
    terminal: VecDeque<TerminalAttempt>,
    next_generation: u64,
}

impl AiAttempts {
    pub(super) fn queue(&mut self, project_id: &str, attempt_id: &str) -> Result<(), ClaimError> {
        if self.terminal_status(project_id, attempt_id).is_some() {
            return Err(ClaimError::AttemptAlreadyFinished);
        }
        self.queued
            .insert((project_id.to_owned(), attempt_id.to_owned()));
        Ok(())
    }

    pub(super) fn claim(
        &mut self,
        project_id: &str,
        attempt_id: &str,
    ) -> Result<Cancellation, ClaimError> {
        if self.terminal_status(project_id, attempt_id).is_some() {
            return Err(ClaimError::AttemptAlreadyFinished);
        }
        if self.active_by_project.contains_key(project_id) {
            return Err(ClaimError::ProjectBusy);
        }
        self.next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(ClaimError::GenerationExhausted)?;
        let generation = AttemptGeneration(self.next_generation);
        self.queued
            .remove(&(project_id.to_owned(), attempt_id.to_owned()));
        let (cancellation, receiver) = watch::channel(false);
        self.active_by_project.insert(
            project_id.to_owned(),
            ActiveAttempt {
                attempt_id: attempt_id.to_owned(),
                generation,
                cancellation,
            },
        );
        Ok(Cancellation {
            receiver,
            generation,
        })
    }

    pub(super) fn cancel(&mut self, project_id: &str, attempt_id: &str) -> CancelResult {
        if self.terminal_status(project_id, attempt_id) == Some(AttemptStatus::Cancelled) {
            return CancelResult::AlreadyCancelled;
        }
        if let Some(active) = self
            .active_by_project
            .get(project_id)
            .filter(|active| active.attempt_id == attempt_id)
        {
            let cancellation = active.cancellation.clone();
            let _ = cancellation.send(true);
            self.active_by_project.remove(project_id);
            self.record_terminal(project_id, attempt_id, AttemptStatus::Cancelled);
            return CancelResult::Cancelled;
        }
        if self
            .queued
            .remove(&(project_id.to_owned(), attempt_id.to_owned()))
        {
            self.record_terminal(project_id, attempt_id, AttemptStatus::Cancelled);
            return CancelResult::Cancelled;
        }
        CancelResult::NotActive
    }

    pub(super) fn complete(
        &mut self,
        project_id: &str,
        attempt_id: &str,
        generation: AttemptGeneration,
        status: AttemptStatus,
    ) -> CompleteResult {
        let Some(active) = self.active_by_project.get(project_id) else {
            return if self.terminal_status(project_id, attempt_id) == Some(AttemptStatus::Cancelled)
            {
                CompleteResult::Cancelled
            } else {
                CompleteResult::NotActive
            };
        };
        if active.attempt_id != attempt_id || active.generation != generation {
            return CompleteResult::NotActive;
        }
        let cancelled = *active.cancellation.borrow()
            || self.terminal_status(project_id, attempt_id) == Some(AttemptStatus::Cancelled);
        self.active_by_project.remove(project_id);
        if cancelled {
            self.record_terminal(project_id, attempt_id, AttemptStatus::Cancelled);
            CompleteResult::Cancelled
        } else {
            self.record_terminal(project_id, attempt_id, status);
            CompleteResult::Recorded
        }
    }

    /// Releases only the exact active attempt and intentionally records no
    /// semantic cancellation. A dropped HTTP request is not the explicit
    /// Creator cancel operation.
    pub(super) fn release(
        &mut self,
        project_id: &str,
        attempt_id: &str,
        generation: AttemptGeneration,
    ) -> bool {
        if self
            .active_by_project
            .get(project_id)
            .is_some_and(|active| {
                active.attempt_id == attempt_id && active.generation == generation
            })
        {
            self.active_by_project.remove(project_id);
            self.queued
                .insert((project_id.to_owned(), attempt_id.to_owned()));
            true
        } else {
            false
        }
    }

    pub(super) fn status(&self, project_id: &str, attempt_id: &str) -> Option<AttemptStatus> {
        if let Some(active) = self.active_by_project.get(project_id)
            && active.attempt_id == attempt_id
        {
            return Some(if *active.cancellation.borrow() {
                AttemptStatus::Cancelled
            } else {
                AttemptStatus::Running
            });
        }
        self.terminal_status(project_id, attempt_id).or_else(|| {
            self.queued
                .contains(&(project_id.to_owned(), attempt_id.to_owned()))
                .then_some(AttemptStatus::Queued)
        })
    }

    pub(super) fn is_active_generation(
        &self,
        project_id: &str,
        attempt_id: &str,
        generation: AttemptGeneration,
    ) -> bool {
        self.active_by_project
            .get(project_id)
            .is_some_and(|active| {
                active.attempt_id == attempt_id && active.generation == generation
            })
    }

    pub(super) fn contains_project(&self, project_id: &str) -> bool {
        self.active_by_project.contains_key(project_id)
    }

    pub(super) fn remove_project(&mut self, project_id: &str) {
        self.active_by_project.remove(project_id);
        self.queued.retain(|(owner, _)| owner != project_id);
        self.terminal
            .retain(|entry| entry.project_id.as_str() != project_id);
    }

    pub(super) fn remove_queued_project(&mut self, project_id: &str) {
        self.queued.retain(|(owner, _)| owner != project_id);
    }

    fn terminal_status(&self, project_id: &str, attempt_id: &str) -> Option<AttemptStatus> {
        self.terminal
            .iter()
            .rev()
            .find(|entry| entry.project_id == project_id && entry.attempt_id == attempt_id)
            .map(|entry| entry.status)
    }

    fn record_terminal(&mut self, project_id: &str, attempt_id: &str, status: AttemptStatus) {
        self.queued
            .remove(&(project_id.to_owned(), attempt_id.to_owned()));
        if let Some(existing) = self
            .terminal
            .iter_mut()
            .find(|entry| entry.project_id == project_id && entry.attempt_id == attempt_id)
        {
            existing.status = status;
            return;
        }
        if self.terminal.len() == MAX_TERMINAL_ATTEMPTS {
            self.terminal.pop_front();
        }
        self.terminal.push_back(TerminalAttempt {
            project_id: project_id.to_owned(),
            attempt_id: attempt_id.to_owned(),
            status,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn explicit_cancel_signals_and_tombstones_the_exact_attempt() {
        let mut attempts = AiAttempts::default();
        let mut cancellation = attempts.claim("prj_a", "att_a").unwrap();
        let generation = cancellation.generation();

        assert_eq!(attempts.cancel("prj_a", "att_a"), CancelResult::Cancelled);
        cancellation.cancelled().await;
        assert!(cancellation.is_cancelled());
        assert!(!attempts.contains_project("prj_a"));
        assert_eq!(
            attempts.status("prj_a", "att_a"),
            Some(AttemptStatus::Cancelled)
        );
        assert_eq!(
            attempts.complete("prj_a", "att_a", generation, AttemptStatus::ProposalReady),
            CompleteResult::Cancelled
        );
        assert_eq!(
            attempts.cancel("prj_a", "att_a"),
            CancelResult::AlreadyCancelled
        );
        assert!(matches!(
            attempts.claim("prj_a", "att_a"),
            Err(ClaimError::AttemptAlreadyFinished)
        ));
    }

    #[test]
    fn request_drop_release_is_not_semantic_cancel_and_allows_same_attempt_retry() {
        let mut attempts = AiAttempts::default();
        attempts.queue("prj_a", "att_a").unwrap();
        let cancellation = attempts.claim("prj_a", "att_a").unwrap();
        assert!(attempts.release("prj_a", "att_a", cancellation.generation()));
        assert_eq!(
            attempts.status("prj_a", "att_a"),
            Some(AttemptStatus::Queued)
        );
        assert!(attempts.claim("prj_a", "att_a").is_ok());
    }

    #[test]
    fn queued_cancel_tombstones_before_claim_and_is_idempotent() {
        let mut attempts = AiAttempts::default();
        attempts.queue("prj_a", "att_a").unwrap();
        assert_eq!(
            attempts.status("prj_a", "att_a"),
            Some(AttemptStatus::Queued)
        );
        assert_eq!(attempts.cancel("prj_a", "att_a"), CancelResult::Cancelled);
        assert_eq!(
            attempts.cancel("prj_a", "att_a"),
            CancelResult::AlreadyCancelled
        );
        assert!(matches!(
            attempts.claim("prj_a", "att_a"),
            Err(ClaimError::AttemptAlreadyFinished)
        ));
    }

    #[test]
    fn stale_release_and_cancel_cannot_touch_another_attempt() {
        let mut attempts = AiAttempts::default();
        let cancellation = attempts.claim("prj_a", "att_new").unwrap();

        assert!(!attempts.release("prj_a", "att_old", cancellation.generation()));
        assert_eq!(attempts.cancel("prj_a", "att_old"), CancelResult::NotActive);
        assert_eq!(
            attempts.status("prj_a", "att_new"),
            Some(AttemptStatus::Running)
        );
        attempts.queue("prj_a", "att_queued").unwrap();
        assert_eq!(
            attempts.cancel("prj_a", "att_queued"),
            CancelResult::Cancelled
        );
        assert_eq!(
            attempts.status("prj_a", "att_new"),
            Some(AttemptStatus::Running)
        );
        assert!(attempts.contains_project("prj_a"));
    }

    #[test]
    fn terminal_status_is_bounded_and_exact() {
        let mut attempts = AiAttempts::default();
        for index in 0..=MAX_TERMINAL_ATTEMPTS {
            let attempt_id = format!("att_{index}");
            let cancellation = attempts.claim("prj_a", &attempt_id).unwrap();
            assert_eq!(
                attempts.complete(
                    "prj_a",
                    &attempt_id,
                    cancellation.generation(),
                    AttemptStatus::ProviderFailed
                ),
                CompleteResult::Recorded
            );
        }
        assert_eq!(attempts.status("prj_a", "att_0"), None);
        assert_eq!(
            attempts.status("prj_a", &format!("att_{}", MAX_TERMINAL_ATTEMPTS)),
            Some(AttemptStatus::ProviderFailed)
        );

        attempts.remove_project("prj_a");
        assert!(!attempts.contains_project("prj_a"));
        assert_eq!(
            attempts.status("prj_a", &format!("att_{}", MAX_TERMINAL_ATTEMPTS)),
            None
        );
    }

    #[test]
    fn stale_generation_cannot_touch_a_reused_attempt_id_after_terminal_eviction() {
        let mut attempts = AiAttempts::default();
        let stale = attempts.claim("prj_a", "att_reused").unwrap();
        let stale_generation = stale.generation();
        assert_eq!(
            attempts.cancel("prj_a", "att_reused"),
            CancelResult::Cancelled
        );

        for index in 0..MAX_TERMINAL_ATTEMPTS {
            let attempt_id = format!("att_eviction_{index}");
            let filler = attempts.claim("prj_filler", &attempt_id).unwrap();
            assert_eq!(
                attempts.complete(
                    "prj_filler",
                    &attempt_id,
                    filler.generation(),
                    AttemptStatus::ProviderFailed
                ),
                CompleteResult::Recorded
            );
        }
        assert_eq!(attempts.status("prj_a", "att_reused"), None);

        let current = attempts.claim("prj_a", "att_reused").unwrap();
        let current_generation = current.generation();
        assert_ne!(stale_generation, current_generation);
        assert!(!attempts.is_active_generation("prj_a", "att_reused", stale_generation));
        assert!(attempts.is_active_generation("prj_a", "att_reused", current_generation));
        assert!(!attempts.release("prj_a", "att_reused", stale_generation));
        assert_eq!(
            attempts.complete(
                "prj_a",
                "att_reused",
                stale_generation,
                AttemptStatus::ProposalReady
            ),
            CompleteResult::NotActive
        );
        assert_eq!(
            attempts.status("prj_a", "att_reused"),
            Some(AttemptStatus::Running)
        );
        assert_eq!(
            attempts.complete(
                "prj_a",
                "att_reused",
                current_generation,
                AttemptStatus::ProposalReady
            ),
            CompleteResult::Recorded
        );
    }
}
