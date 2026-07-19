#![forbid(unsafe_code)]

//! Deterministic fault-injection vocabulary for C9 recovery tests.
//!
//! Production builds intentionally expose no runtime switch, environment
//! variable, or request-controlled hook. Until a caller is compiled as a test,
//! [`check`] is an unconditional no-op. Test code must hold a [`testing::Scenario`]
//! while arming failpoints so parallel tests cannot share counters accidentally.

use std::{fmt, io};

/// The small, static set of operating-system failure classes that tests may
/// attach to a boundary. This type is not configurable outside a test build;
/// keeping it here lets storage tests distinguish disk exhaustion and denied
/// writes without accepting an OS error, path, or message from a caller.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum InjectedFailureKind {
    Other,
    StorageFull,
    PermissionDenied,
}

impl InjectedFailureKind {
    pub(crate) const fn io_kind(self) -> io::ErrorKind {
        match self {
            Self::Other => io::ErrorKind::Other,
            Self::StorageFull => io::ErrorKind::StorageFull,
            Self::PermissionDenied => io::ErrorKind::PermissionDenied,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Other => "other",
            Self::StorageFull => "storage_full",
            Self::PermissionDenied => "permission_denied",
        }
    }
}

/// Named boundaries that must be exercised by Decision and storage recovery
/// tests. Names are static and deliberately contain no project ID, path,
/// credential, rationale, or provider content.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum Failpoint {
    DecisionJournalIntentBefore,
    DecisionJournalIntentAfter,
    DecisionObjectWriteBefore,
    DecisionObjectWriteAfter,
    DecisionCasPublishBefore,
    DecisionCasPublishAfter,
    DecisionSynapseCallBefore,
    DecisionSynapseCallAfter,
    DecisionReceiptPersistBefore,
    DecisionReceiptPersistAfter,
    DecisionReceiptQueryBefore,
    DecisionReceiptQueryAfter,
    DecisionAcceptedPointerBefore,
    DecisionAcceptedPointerAfter,
    DecisionMaterializeBefore,
    DecisionMaterializeAfter,
    DecisionJournalCompleteBefore,
    DecisionJournalCompleteAfter,
    DecisionResponseBefore,
    DecisionResponseAfter,
    StorageWriteBefore,
    StorageWriteAfter,
    StorageFsyncBefore,
    StorageFsyncAfter,
    StorageRenameBefore,
    StorageRenameAfter,
}

impl Failpoint {
    #[cfg(test)]
    pub(crate) const ALL: [Self; 26] = [
        Self::DecisionJournalIntentBefore,
        Self::DecisionJournalIntentAfter,
        Self::DecisionObjectWriteBefore,
        Self::DecisionObjectWriteAfter,
        Self::DecisionCasPublishBefore,
        Self::DecisionCasPublishAfter,
        Self::DecisionSynapseCallBefore,
        Self::DecisionSynapseCallAfter,
        Self::DecisionReceiptPersistBefore,
        Self::DecisionReceiptPersistAfter,
        Self::DecisionReceiptQueryBefore,
        Self::DecisionReceiptQueryAfter,
        Self::DecisionAcceptedPointerBefore,
        Self::DecisionAcceptedPointerAfter,
        Self::DecisionMaterializeBefore,
        Self::DecisionMaterializeAfter,
        Self::DecisionJournalCompleteBefore,
        Self::DecisionJournalCompleteAfter,
        Self::DecisionResponseBefore,
        Self::DecisionResponseAfter,
        Self::StorageWriteBefore,
        Self::StorageWriteAfter,
        Self::StorageFsyncBefore,
        Self::StorageFsyncAfter,
        Self::StorageRenameBefore,
        Self::StorageRenameAfter,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::DecisionJournalIntentBefore => "decision.journal.intent.before",
            Self::DecisionJournalIntentAfter => "decision.journal.intent.after",
            Self::DecisionObjectWriteBefore => "decision.object.write.before",
            Self::DecisionObjectWriteAfter => "decision.object.write.after",
            Self::DecisionCasPublishBefore => "decision.cas.publish.before",
            Self::DecisionCasPublishAfter => "decision.cas.publish.after",
            Self::DecisionSynapseCallBefore => "decision.synapse.call.before",
            Self::DecisionSynapseCallAfter => "decision.synapse.call.after",
            Self::DecisionReceiptPersistBefore => "decision.receipt.persist.before",
            Self::DecisionReceiptPersistAfter => "decision.receipt.persist.after",
            Self::DecisionReceiptQueryBefore => "decision.receipt.query.before",
            Self::DecisionReceiptQueryAfter => "decision.receipt.query.after",
            Self::DecisionAcceptedPointerBefore => "decision.accepted-pointer.before",
            Self::DecisionAcceptedPointerAfter => "decision.accepted-pointer.after",
            Self::DecisionMaterializeBefore => "decision.materialize.before",
            Self::DecisionMaterializeAfter => "decision.materialize.after",
            Self::DecisionJournalCompleteBefore => "decision.journal.complete.before",
            Self::DecisionJournalCompleteAfter => "decision.journal.complete.after",
            Self::DecisionResponseBefore => "decision.response.before",
            Self::DecisionResponseAfter => "decision.response.after",
            Self::StorageWriteBefore => "storage.write.before",
            Self::StorageWriteAfter => "storage.write.after",
            Self::StorageFsyncBefore => "storage.fsync.before",
            Self::StorageFsyncAfter => "storage.fsync.after",
            Self::StorageRenameBefore => "storage.rename.before",
            Self::StorageRenameAfter => "storage.rename.after",
        }
    }
}

/// A synthetic test failure. Its diagnostics are restricted to a static
/// failpoint name and a counter; accepting arbitrary context here is
/// intentionally impossible.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct InjectedFault {
    failpoint: Failpoint,
    occurrence: u64,
    kind: InjectedFailureKind,
}

#[cfg(test)]
impl InjectedFault {
    const fn new(failpoint: Failpoint, occurrence: u64, kind: InjectedFailureKind) -> Self {
        Self {
            failpoint,
            occurrence,
            kind,
        }
    }

    pub(crate) const fn failpoint(self) -> Failpoint {
        self.failpoint
    }

    pub(crate) const fn occurrence(self) -> u64 {
        self.occurrence
    }
}

impl InjectedFault {
    pub(crate) const fn io_kind(self) -> io::ErrorKind {
        self.kind.io_kind()
    }
}

impl fmt::Debug for InjectedFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("InjectedFault")
            .field("failpoint", &self.failpoint.name())
            .field("occurrence", &self.occurrence)
            .field("kind", &self.kind.name())
            .finish()
    }
}

impl fmt::Display for InjectedFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "injected {} fault at {} occurrence {}",
            self.kind.name(),
            self.failpoint.name(),
            self.occurrence
        )
    }
}

impl std::error::Error for InjectedFault {}

fn no_injection(_failpoint: Failpoint) -> Result<(), InjectedFault> {
    Ok(())
}

/// Checks a durable boundary. Production builds always return `Ok(())`.
pub(crate) fn check(failpoint: Failpoint) -> Result<(), InjectedFault> {
    #[cfg(test)]
    {
        testing::check(failpoint)
    }
    #[cfg(not(test))]
    {
        no_injection(failpoint)
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::{Failpoint, InjectedFailureKind, InjectedFault};
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
    use std::thread::ThreadId;

    #[derive(Default)]
    struct Registry {
        owner: Option<ThreadId>,
        hits: BTreeMap<Failpoint, u64>,
        scheduled: BTreeMap<Failpoint, BTreeMap<u64, InjectedFailureKind>>,
        abort_scheduled: BTreeMap<Failpoint, BTreeSet<u64>>,
    }

    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    static SCENARIO_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    fn registry() -> &'static Mutex<Registry> {
        REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
    }

    fn lock_registry() -> MutexGuard<'static, Registry> {
        registry().lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Serializes one deterministic fault scenario and resets all counters on
    /// entry and exit. Keep this guard alive across the operation under test.
    #[must_use]
    pub(crate) struct Scenario {
        _serial: MutexGuard<'static, ()>,
    }

    impl Scenario {
        pub(crate) fn begin() -> Self {
            let serial = SCENARIO_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            *lock_registry() = Registry {
                owner: Some(std::thread::current().id()),
                ..Registry::default()
            };
            Self { _serial: serial }
        }
    }

    impl Drop for Scenario {
        fn drop(&mut self) {
            *lock_registry() = Registry::default();
        }
    }

    /// Arms one failpoint for an exact one-based occurrence. Re-arming the
    /// same occurrence is idempotent.
    pub(crate) fn fail_on(failpoint: Failpoint, occurrence: u64) {
        fail_on_kind(failpoint, occurrence, InjectedFailureKind::Other);
    }

    /// Arms one failpoint with a static I/O failure class. Re-arming an exact
    /// occurrence is allowed only with the same class so a scenario cannot
    /// silently change meaning.
    pub(crate) fn fail_on_kind(failpoint: Failpoint, occurrence: u64, kind: InjectedFailureKind) {
        assert!(occurrence > 0, "failpoint occurrence must be one-based");
        let previous = lock_registry()
            .scheduled
            .entry(failpoint)
            .or_default()
            .insert(occurrence, kind);
        assert!(
            previous.is_none() || previous == Some(kind),
            "an exact failpoint occurrence cannot change failure kind"
        );
    }

    /// Aborts the test process at an exact one-based occurrence. This API is
    /// compiled only into unit-test executables and is never present in the
    /// production binary.
    pub(crate) fn abort_on(failpoint: Failpoint, occurrence: u64) {
        assert!(occurrence > 0, "failpoint occurrence must be one-based");
        lock_registry()
            .abort_scheduled
            .entry(failpoint)
            .or_default()
            .insert(occurrence);
    }

    /// Arms the next occurrence using the current deterministic counter.
    pub(crate) fn fail_next(failpoint: Failpoint) -> u64 {
        let mut registry = lock_registry();
        let occurrence = registry
            .hits
            .get(&failpoint)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        registry
            .scheduled
            .entry(failpoint)
            .or_default()
            .insert(occurrence, InjectedFailureKind::Other);
        occurrence
    }

    pub(crate) fn hit_count(failpoint: Failpoint) -> u64 {
        lock_registry().hits.get(&failpoint).copied().unwrap_or(0)
    }

    pub(super) fn check(failpoint: Failpoint) -> Result<(), InjectedFault> {
        let mut registry = lock_registry();
        if registry.owner.as_ref() != Some(&std::thread::current().id()) {
            // Rust's test runner executes unrelated tests concurrently. A
            // scenario is authority only on the thread that created its
            // non-Send guard, so normal tests cannot consume its counters or
            // receive its synthetic failures. Explicit abort-child scenarios
            // arm and execute on their own child-process thread as before.
            return Ok(());
        }
        let occurrence = registry
            .hits
            .entry(failpoint)
            .and_modify(|value| *value = value.saturating_add(1))
            .or_insert(1)
            .to_owned();
        let failure_kind = registry
            .scheduled
            .get_mut(&failpoint)
            .and_then(|occurrences| occurrences.remove(&occurrence));
        let should_abort = registry
            .abort_scheduled
            .get_mut(&failpoint)
            .is_some_and(|occurrences| occurrences.remove(&occurrence));
        if registry
            .scheduled
            .get(&failpoint)
            .is_some_and(BTreeMap::is_empty)
        {
            registry.scheduled.remove(&failpoint);
        }
        if registry
            .abort_scheduled
            .get(&failpoint)
            .is_some_and(BTreeSet::is_empty)
        {
            registry.abort_scheduled.remove(&failpoint);
        }
        if should_abort {
            std::process::abort();
        }
        if let Some(kind) = failure_kind {
            Err(InjectedFault::new(failpoint, occurrence, kind))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Failpoint, InjectedFailureKind, check, no_injection, testing};
    use std::collections::BTreeSet;

    #[test]
    fn production_default_is_an_unconditional_noop() {
        for failpoint in Failpoint::ALL {
            assert_eq!(no_injection(failpoint), Ok(()));
        }
    }

    #[test]
    fn every_boundary_has_a_unique_static_safe_name() {
        let names = Failpoint::ALL
            .into_iter()
            .map(Failpoint::name)
            .collect::<BTreeSet<_>>();
        assert_eq!(names.len(), Failpoint::ALL.len());
        assert!(names.iter().all(|name| {
            !name.is_empty()
                && name.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'-')
                })
                && !name.contains('/')
                && !name.contains('\\')
        }));
    }

    #[test]
    fn exact_occurrences_fail_once_and_counters_remain_deterministic() {
        let _scenario = testing::Scenario::begin();
        let failpoint = Failpoint::DecisionReceiptPersistAfter;
        testing::fail_on(failpoint, 2);
        testing::fail_on(failpoint, 2);

        assert_eq!(check(failpoint), Ok(()));
        let fault = check(failpoint).unwrap_err();
        assert_eq!(fault.failpoint(), failpoint);
        assert_eq!(fault.occurrence(), 2);
        assert_eq!(check(failpoint), Ok(()));
        assert_eq!(testing::hit_count(failpoint), 3);
    }

    #[test]
    fn fail_next_and_independent_counters_are_predictable() {
        let _scenario = testing::Scenario::begin();
        let write = Failpoint::StorageWriteBefore;
        let rename = Failpoint::StorageRenameAfter;

        assert_eq!(check(write), Ok(()));
        assert_eq!(testing::fail_next(write), 2);
        assert_eq!(testing::fail_next(rename), 1);
        assert!(check(rename).is_err());
        assert!(check(write).is_err());
        assert_eq!(testing::hit_count(write), 2);
        assert_eq!(testing::hit_count(rename), 1);
    }

    #[test]
    fn scenario_drop_clears_plans_and_counters() {
        let failpoint = Failpoint::DecisionJournalIntentBefore;
        {
            let _scenario = testing::Scenario::begin();
            testing::fail_on(failpoint, 1);
            assert!(check(failpoint).is_err());
        }
        {
            let _scenario = testing::Scenario::begin();
            assert_eq!(testing::hit_count(failpoint), 0);
            assert_eq!(check(failpoint), Ok(()));
        }
    }

    #[test]
    fn scenario_faults_do_not_leak_to_parallel_non_owner_threads() {
        let _scenario = testing::Scenario::begin();
        let failpoint = Failpoint::StorageRenameBefore;
        testing::fail_on(failpoint, 1);

        let workers = (0..8)
            .map(|_| {
                std::thread::spawn(move || {
                    for _ in 0..128 {
                        assert_eq!(check(failpoint), Ok(()));
                    }
                })
            })
            .collect::<Vec<_>>();
        for worker in workers {
            worker.join().unwrap();
        }

        assert_eq!(testing::hit_count(failpoint), 0);
        let fault = check(failpoint).unwrap_err();
        assert_eq!(fault.failpoint(), failpoint);
        assert_eq!(fault.occurrence(), 1);
        assert_eq!(testing::hit_count(failpoint), 1);
    }

    #[test]
    fn diagnostics_cannot_contain_dynamic_secrets_or_paths() {
        let _scenario = testing::Scenario::begin();
        let failpoint = Failpoint::DecisionResponseBefore;
        testing::fail_on(failpoint, 1);
        let fault = check(failpoint).unwrap_err();
        let debug = format!("{fault:?}");
        let display = fault.to_string();

        assert_eq!(
            debug,
            "InjectedFault { failpoint: \"decision.response.before\", occurrence: 1, kind: \"other\" }"
        );
        assert_eq!(
            display,
            "injected other fault at decision.response.before occurrence 1"
        );
        for forbidden in ["/home/", "/tmp/", "sk-", "Bearer ", "rationale"] {
            assert!(!debug.contains(forbidden));
            assert!(!display.contains(forbidden));
        }
    }

    #[test]
    fn static_io_failure_classes_preserve_the_requested_error_kind() {
        let _scenario = testing::Scenario::begin();
        testing::fail_on_kind(
            Failpoint::StorageWriteBefore,
            1,
            InjectedFailureKind::StorageFull,
        );
        testing::fail_on_kind(
            Failpoint::StorageRenameBefore,
            1,
            InjectedFailureKind::PermissionDenied,
        );

        assert_eq!(
            check(Failpoint::StorageWriteBefore).unwrap_err().io_kind(),
            std::io::ErrorKind::StorageFull
        );
        assert_eq!(
            check(Failpoint::StorageRenameBefore).unwrap_err().io_kind(),
            std::io::ErrorKind::PermissionDenied
        );
    }
}
