//! Deterministic monitoring-rule evaluation —
//! `Docs/graph/tasks/MON-002.md` Deliverables 2–4:
//!
//! - **Deterministic window/condition evaluation**: given a fixed rule, a
//!   fixed slice of already-ingested `crate::sample::TelemetrySample`s, a
//!   fixed `crate::freshness::FreshnessTracker` snapshot, and a fixed `now`,
//!   [`RuleEvaluator::evaluate`]'s outcome is a pure function of those
//!   inputs plus this evaluator's own explicit prior state (see "State and
//!   restart" below) — never real wall-clock time, never randomness.
//! - **Debounce vs. cooldown, kept distinct.** See "Debounce and cooldown"
//!   below for the exact state machine and why the two cannot be conflated.
//! - **Missing-data behavior**, driven by `crate::freshness::FreshnessTracker`
//!   (never guessed by this module from the sample slice alone — a signal
//!   that has gone stale/absent is a freshness question, not a condition
//!   question).
//!
//! # Debounce and cooldown
//!
//! Per rule+device, this evaluator tracks two independent facts:
//!
//! - `streak_started_at`: when the condition (or a forced missing-data
//!   substitute — see below) most recently transitioned from not-met to
//!   met. Cleared the instant the condition is not met.
//! - `last_fired_at`: the last time this rule+device pair actually fired.
//!   Independent of `streak_started_at` — it is **not** cleared when the
//!   condition stops being met, because cooldown must keep suppressing a
//!   rapid re-trigger even across a brief flap.
//!
//! On every `evaluate` call where the condition is currently met:
//!
//! 1. If this is a new streak (`streak_started_at` was `None`), start it at
//!    `now`.
//! 2. **Debounce** gates the *first* fire of a streak: while
//!    `now - streak_started_at < rule.debounce`, the outcome is
//!    [`EvaluationOutcome::Pending`], never a fire, no matter how many
//!    consecutive samples/evaluate calls have already confirmed the
//!    condition.
//! 3. Once debounce has elapsed for this streak, **cooldown** gates
//!    subsequent fires: if `last_fired_at` is set and
//!    `now - last_fired_at < rule.cooldown`, the outcome is
//!    [`EvaluationOutcome::Suppressed`] — the rule does not re-fire on every
//!    sample that keeps meeting its condition. Once cooldown has elapsed,
//!    the next `evaluate` call fires again *immediately* (it does not need
//!    to re-run debounce, because debounce's question — "has this streak
//!    genuinely persisted" — was already answered the first time and the
//!    streak never broke).
//!
//! This keeps the two concepts observably distinct and independently
//! testable: a rule with `debounce = 0s, cooldown = 5m` fires on the very
//! first met sample, then stays `Suppressed` for five minutes even though
//! the condition never stops being true; a rule with `debounce = 5m,
//! cooldown = 0s` never fires until the condition has held continuously for
//! five minutes, and then may fire again on the next sample once debounce
//! has been satisfied once (cooldown of zero never suppresses).
//!
//! # State and restart
//!
//! [`RuleEvaluator`]'s per-target state (`streak_started_at`/
//! `last_fired_at`) is **in-memory only** and resets on every new
//! `RuleEvaluator` (i.e. every process restart) — following exactly the
//! discipline `crate::pipeline::Pipeline`'s module doc establishes for
//! `MON-001`'s in-memory freshness/gap state: re-inventing "how long has
//! this streak already been running" across a restart would mean fabricating
//! state this process cannot actually verify (`AGENTS.md` rule 1). A rule
//! whose condition was mid-debounce at the moment of a restart honestly
//! starts a fresh streak after restart, not a fabricated continuation of the
//! pre-restart one. Durable rule *definitions* and *evaluation history* are
//! `crate::rule_store::RuleStore`'s responsibility, not this module's — see
//! that module's doc for what does and does not survive a restart.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use loki_connectors_sdk::ids::DeviceId;

use crate::freshness::{Freshness, FreshnessTracker};
use crate::rule::{MissingDataBehavior, MonitoringRule, RuleId};
use crate::sample::TelemetrySample;

/// Why a rule was, or was not, evaluated against real ingested data on a
/// given `evaluate` call. Reported on every non-short-circuited outcome so a
/// caller can always see *which* of a rule's declared missing-data paths
/// actually applied, even when the configured behavior does not itself
/// produce a dedicated alert.
#[derive(Debug, Clone, PartialEq)]
pub enum DataStatus {
    /// Every required signal was fresh as of `now`; the condition was
    /// evaluated against real samples.
    AllSignalsFresh,
    /// At least one required signal was stale/never-observed, and this
    /// rule's `MissingDataBehavior::TreatAsMet` applied — the condition was
    /// forced to "met" rather than actually evaluated.
    MissingTreatedAsMet { missing_signals: Vec<String> },
    /// As above, but `MissingDataBehavior::TreatAsNotMet` applied — the
    /// condition was forced to "not met".
    MissingTreatedAsNotMet { missing_signals: Vec<String> },
}

/// The full result of one [`RuleEvaluator::evaluate`] call.
#[derive(Debug, Clone, PartialEq)]
pub enum EvaluationOutcome {
    /// The rule's own `enabled` field is `false`. No state was touched; a
    /// disabled rule is inert, not silently evaluated anyway.
    Disabled,
    /// `device` is not matched by the rule's `scope`. No state was touched.
    OutOfScope,
    /// At least one required signal was stale/absent and this rule's
    /// `MissingDataBehavior::FireMissingDataAlert` applied. This is a
    /// distinct alert from a normal condition [`Self::Fired`] outcome — it
    /// bypasses debounce/cooldown entirely (data being missing is not a
    /// condition-met streak) and does not mutate this evaluator's streak
    /// state.
    MissingDataAlert {
        missing_signals: Vec<String>,
        at: SystemTime,
    },
    /// The condition (or a missing-data-forced substitute) was not met.
    /// Any prior streak was cleared.
    NotMet { data_status: DataStatus },
    /// The condition is met but this streak has not yet satisfied
    /// `rule.debounce` — has never fired.
    Pending {
        data_status: DataStatus,
        streak_started_at: SystemTime,
    },
    /// This call is the one that fired the rule.
    Fired {
        data_status: DataStatus,
        streak_started_at: SystemTime,
        fired_at: SystemTime,
    },
    /// The condition remains met and debounce has already been satisfied
    /// for this streak, but firing is suppressed because `rule.cooldown`
    /// has not yet elapsed since the last fire.
    Suppressed {
        data_status: DataStatus,
        last_fired_at: SystemTime,
        cooldown_remaining: Duration,
    },
}

impl EvaluationOutcome {
    pub fn fired(&self) -> bool {
        matches!(self, EvaluationOutcome::Fired { .. })
    }

    /// Stable wire/storage representation of which outcome variant this is
    /// — used by `crate::rule_store::RuleStore::record_evaluation` to
    /// persist an evaluation record without needing to serialize every
    /// variant's full payload.
    pub const fn kind_wire_str(&self) -> &'static str {
        match self {
            EvaluationOutcome::Disabled => "disabled",
            EvaluationOutcome::OutOfScope => "out_of_scope",
            EvaluationOutcome::MissingDataAlert { .. } => "missing_data_alert",
            EvaluationOutcome::NotMet { .. } => "not_met",
            EvaluationOutcome::Pending { .. } => "pending",
            EvaluationOutcome::Fired { .. } => "fired",
            EvaluationOutcome::Suppressed { .. } => "suppressed",
        }
    }

    /// The set of required signals this outcome reports as missing, if any
    /// (empty for every non-missing-data path).
    pub fn missing_signals(&self) -> Vec<String> {
        match self {
            EvaluationOutcome::MissingDataAlert {
                missing_signals, ..
            } => missing_signals.clone(),
            EvaluationOutcome::NotMet { data_status }
            | EvaluationOutcome::Pending { data_status, .. }
            | EvaluationOutcome::Fired { data_status, .. }
            | EvaluationOutcome::Suppressed { data_status, .. } => data_status.missing_signals(),
            EvaluationOutcome::Disabled | EvaluationOutcome::OutOfScope => Vec::new(),
        }
    }
}

impl DataStatus {
    pub fn missing_signals(&self) -> Vec<String> {
        match self {
            DataStatus::AllSignalsFresh => Vec::new(),
            DataStatus::MissingTreatedAsMet { missing_signals }
            | DataStatus::MissingTreatedAsNotMet { missing_signals } => missing_signals.clone(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct TargetState {
    streak_started_at: Option<SystemTime>,
    last_fired_at: Option<SystemTime>,
}

/// Evaluates [`MonitoringRule`]s against already-ingested samples and
/// freshness state, tracking per rule+device debounce/cooldown state. See
/// this module's doc for the exact debounce/cooldown contract and restart
/// behavior.
#[derive(Debug, Default)]
pub struct RuleEvaluator {
    state: HashMap<(RuleId, DeviceId), TargetState>,
}

impl RuleEvaluator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Evaluates `rule` for `device` at `now`, using `samples` (any subset
    /// of already-ingested samples; only entries for `device` and this
    /// rule's signals are consulted) and `freshness` (this rule's
    /// `required_signals` are checked for staleness via
    /// `crate::freshness::FreshnessTracker::assess`, using `rule.window` as
    /// the staleness window — the same window that bounds condition
    /// evaluation, since `Docs/SPEC.md` §3.6 does not define a second,
    /// separate staleness duration).
    pub fn evaluate(
        &mut self,
        rule: &MonitoringRule,
        device: &DeviceId,
        samples: &[TelemetrySample],
        freshness: &FreshnessTracker,
        now: SystemTime,
    ) -> EvaluationOutcome {
        if !rule.enabled {
            return EvaluationOutcome::Disabled;
        }
        if !rule.scope.matches(device) {
            return EvaluationOutcome::OutOfScope;
        }

        let missing_signals = missing_required_signals(rule, device, freshness, now);

        if !missing_signals.is_empty() {
            match rule.missing_data_behavior {
                MissingDataBehavior::FireMissingDataAlert => {
                    // A distinct alert, not a condition-met streak: does not
                    // touch this evaluator's per-target state at all.
                    return EvaluationOutcome::MissingDataAlert {
                        missing_signals,
                        at: now,
                    };
                }
                MissingDataBehavior::TreatAsMet => {
                    let data_status = DataStatus::MissingTreatedAsMet { missing_signals };
                    return self.advance(rule, device, now, true, data_status);
                }
                MissingDataBehavior::TreatAsNotMet => {
                    let data_status = DataStatus::MissingTreatedAsNotMet { missing_signals };
                    return self.advance(rule, device, now, false, data_status);
                }
            }
        }

        let condition_met = condition_met(rule, device, samples, now);
        self.advance(
            rule,
            device,
            now,
            condition_met,
            DataStatus::AllSignalsFresh,
        )
    }

    fn advance(
        &mut self,
        rule: &MonitoringRule,
        device: &DeviceId,
        now: SystemTime,
        condition_met: bool,
        data_status: DataStatus,
    ) -> EvaluationOutcome {
        let key = (rule.rule_id.clone(), device.clone());
        let state = self.state.entry(key).or_default();

        if !condition_met {
            state.streak_started_at = None;
            return EvaluationOutcome::NotMet { data_status };
        }

        let streak_started_at = *state.streak_started_at.get_or_insert(now);

        let debounce_elapsed = elapsed_at_least(streak_started_at, now, rule.debounce);
        if !debounce_elapsed {
            return EvaluationOutcome::Pending {
                data_status,
                streak_started_at,
            };
        }

        if let Some(last_fired_at) = state.last_fired_at
            && let Some(cooldown_remaining) = remaining_cooldown(last_fired_at, now, rule.cooldown)
        {
            return EvaluationOutcome::Suppressed {
                data_status,
                last_fired_at,
                cooldown_remaining,
            };
        }

        state.last_fired_at = Some(now);
        EvaluationOutcome::Fired {
            data_status,
            streak_started_at,
            fired_at: now,
        }
    }
}

/// True if at least `duration` has elapsed between `start` and `now`. A
/// `now` before `start` (clock skew) is treated as "not yet elapsed" rather
/// than erroring — matches `crate::freshness::FreshnessTracker::assess`'s
/// documented clock-skew handling.
fn elapsed_at_least(start: SystemTime, now: SystemTime, duration: Duration) -> bool {
    match now.duration_since(start) {
        Ok(elapsed) => elapsed >= duration,
        Err(_) => false,
    }
}

/// `Some(remaining)` if `now` is still within `duration` of `last`;
/// `None` if the cooldown has fully elapsed (or `now` precedes `last`, which
/// this treats as "cooldown has not started counting down yet" — i.e. still
/// fully remaining — rather than as elapsed, so a clock-skew sample cannot
/// bypass cooldown).
fn remaining_cooldown(last: SystemTime, now: SystemTime, duration: Duration) -> Option<Duration> {
    if duration.is_zero() {
        return None;
    }
    match now.duration_since(last) {
        Ok(elapsed) if elapsed < duration => Some(duration - elapsed),
        Ok(_) => None,
        Err(_) => Some(duration),
    }
}

fn missing_required_signals(
    rule: &MonitoringRule,
    device: &DeviceId,
    freshness: &FreshnessTracker,
    now: SystemTime,
) -> Vec<String> {
    rule.required_signals
        .iter()
        .filter(|signal| {
            let key = MonitoringRule::signal_key_for(device, signal);
            !matches!(freshness.assess(&key, now, rule.window), Freshness::Fresh)
        })
        .cloned()
        .collect()
}

/// Evaluates `rule.condition` against the most recent (`receive_time`)
/// sample for `rule.condition.signal` on `device`, within `[now -
/// rule.window, now]`. No sample in window => not met (this is a distinct,
/// narrower question than "is the signal fresh" — see this module's doc for
/// why staleness is checked separately via `FreshnessTracker`).
fn condition_met(
    rule: &MonitoringRule,
    device: &DeviceId,
    samples: &[TelemetrySample],
    now: SystemTime,
) -> bool {
    let window_start = now.checked_sub(rule.window);

    let most_recent = samples
        .iter()
        .filter(|s| s.source_id == *device && s.metric_name == rule.condition.signal)
        .filter(|s| {
            s.receive_time <= now && window_start.is_none_or(|start| s.receive_time >= start)
        })
        .max_by_key(|s| s.receive_time);

    match most_recent {
        Some(sample) => rule
            .condition
            .operator
            .evaluate(sample.value.value, rule.condition.threshold),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{
        ComparisonOperator, Condition, MissingDataBehavior, RuleId, ScopeSelector, Severity,
    };
    use crate::sample::{RetentionClass, SampleQuality};

    fn device(id: &str) -> DeviceId {
        DeviceId::new(id).unwrap()
    }

    fn epoch(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn make_rule(
        debounce: Duration,
        cooldown: Duration,
        window: Duration,
        missing_data_behavior: MissingDataBehavior,
    ) -> MonitoringRule {
        MonitoringRule::new(
            RuleId::new("rule-1").unwrap(),
            1,
            "workspace-1",
            ScopeSelector::AllDevices,
            vec!["temperature_c".to_string()],
            Condition {
                signal: "temperature_c".to_string(),
                operator: ComparisonOperator::GreaterThan,
                threshold: 80.0,
            },
            window,
            debounce,
            cooldown,
            Severity::new("critical").unwrap(),
            true,
            missing_data_behavior,
            "user-1",
            SystemTime::UNIX_EPOCH,
            SystemTime::UNIX_EPOCH,
        )
        .unwrap()
    }

    fn sample(device_id: &str, value: f64, t: SystemTime) -> TelemetrySample {
        TelemetrySample::new(
            device(device_id),
            "temperature_c",
            value,
            "C",
            t,
            t,
            SampleQuality::Good,
            RetentionClass::new("standard").unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn disabled_rule_never_evaluates() {
        let mut rule = make_rule(
            Duration::ZERO,
            Duration::ZERO,
            Duration::from_secs(60),
            MissingDataBehavior::TreatAsNotMet,
        );
        rule.enabled = false;
        let mut evaluator = RuleEvaluator::new();
        let freshness = FreshnessTracker::new();
        let outcome = evaluator.evaluate(&rule, &device("dev-1"), &[], &freshness, epoch(100));
        assert_eq!(outcome, EvaluationOutcome::Disabled);
    }

    #[test]
    fn out_of_scope_device_never_evaluates() {
        let rule = MonitoringRule::new(
            RuleId::new("rule-1").unwrap(),
            1,
            "workspace-1",
            ScopeSelector::Devices(vec![device("dev-1")]),
            vec!["temperature_c".to_string()],
            Condition {
                signal: "temperature_c".to_string(),
                operator: ComparisonOperator::GreaterThan,
                threshold: 80.0,
            },
            Duration::from_secs(60),
            Duration::ZERO,
            Duration::ZERO,
            Severity::new("critical").unwrap(),
            true,
            MissingDataBehavior::TreatAsNotMet,
            "user-1",
            SystemTime::UNIX_EPOCH,
            SystemTime::UNIX_EPOCH,
        )
        .unwrap();
        let mut evaluator = RuleEvaluator::new();
        let freshness = FreshnessTracker::new();
        let outcome = evaluator.evaluate(&rule, &device("dev-2"), &[], &freshness, epoch(100));
        assert_eq!(outcome, EvaluationOutcome::OutOfScope);
    }

    #[test]
    fn deterministic_evaluation_same_inputs_same_outcome() {
        // Deliverable 2: fixed rule + fixed samples -> reproducible outcome.
        let rule = make_rule(
            Duration::ZERO,
            Duration::ZERO,
            Duration::from_secs(60),
            MissingDataBehavior::TreatAsNotMet,
        );
        let t0 = epoch(1000);
        let samples = vec![sample("dev-1", 95.0, t0)];
        let mut freshness = FreshnessTracker::new();
        freshness.observe("dev-1::temperature_c", t0);

        let mut evaluator_a = RuleEvaluator::new();
        let outcome_a = evaluator_a.evaluate(&rule, &device("dev-1"), &samples, &freshness, t0);

        let mut evaluator_b = RuleEvaluator::new();
        let outcome_b = evaluator_b.evaluate(&rule, &device("dev-1"), &samples, &freshness, t0);

        assert_eq!(outcome_a, outcome_b);
        assert!(outcome_a.fired());
    }

    #[test]
    fn condition_not_met_never_fires() {
        let rule = make_rule(
            Duration::ZERO,
            Duration::ZERO,
            Duration::from_secs(60),
            MissingDataBehavior::TreatAsNotMet,
        );
        let t0 = epoch(1000);
        let samples = vec![sample("dev-1", 10.0, t0)];
        let mut freshness = FreshnessTracker::new();
        freshness.observe("dev-1::temperature_c", t0);

        let mut evaluator = RuleEvaluator::new();
        let outcome = evaluator.evaluate(&rule, &device("dev-1"), &samples, &freshness, t0);
        assert!(matches!(outcome, EvaluationOutcome::NotMet { .. }));
    }

    #[test]
    fn debounce_delays_the_first_fire_but_not_forever() {
        let rule = make_rule(
            Duration::from_secs(30),
            Duration::ZERO,
            Duration::from_secs(120),
            MissingDataBehavior::TreatAsNotMet,
        );
        let t0 = epoch(1000);
        let mut freshness = FreshnessTracker::new();
        freshness.observe("dev-1::temperature_c", t0);
        let samples = vec![sample("dev-1", 95.0, t0)];

        let mut evaluator = RuleEvaluator::new();
        // At t0: condition just became met, debounce (30s) not satisfied.
        let outcome = evaluator.evaluate(&rule, &device("dev-1"), &samples, &freshness, t0);
        assert!(
            matches!(outcome, EvaluationOutcome::Pending { .. }),
            "expected Pending, got {outcome:?}"
        );

        // At t0+10s: still within debounce.
        let t1 = t0 + Duration::from_secs(10);
        let outcome = evaluator.evaluate(&rule, &device("dev-1"), &samples, &freshness, t1);
        assert!(
            matches!(outcome, EvaluationOutcome::Pending { .. }),
            "expected Pending, got {outcome:?}"
        );

        // At t0+30s: debounce satisfied, first fire.
        let t2 = t0 + Duration::from_secs(30);
        let outcome = evaluator.evaluate(&rule, &device("dev-1"), &samples, &freshness, t2);
        assert!(outcome.fired(), "expected Fired, got {outcome:?}");
    }

    #[test]
    fn cooldown_suppresses_repeated_fires_distinctly_from_debounce() {
        // Deliverable 3: debounce and cooldown are real, tested, distinct
        // behaviors. debounce = 0 (fires immediately once met); cooldown =
        // 60s (must not re-fire on every subsequent still-met sample).
        let rule = make_rule(
            Duration::ZERO,
            Duration::from_secs(60),
            Duration::from_secs(120),
            MissingDataBehavior::TreatAsNotMet,
        );
        let t0 = epoch(1000);
        let mut freshness = FreshnessTracker::new();
        freshness.observe("dev-1::temperature_c", t0);
        let samples = vec![sample("dev-1", 95.0, t0)];

        let mut evaluator = RuleEvaluator::new();
        let first = evaluator.evaluate(&rule, &device("dev-1"), &samples, &freshness, t0);
        assert!(first.fired(), "debounce=0 fires immediately: {first:?}");

        // 10s later, still met: must be suppressed by cooldown, not re-fire.
        let t1 = t0 + Duration::from_secs(10);
        let mut freshness1 = FreshnessTracker::new();
        freshness1.observe("dev-1::temperature_c", t1);
        let samples1 = vec![sample("dev-1", 95.0, t1)];
        let second = evaluator.evaluate(&rule, &device("dev-1"), &samples1, &freshness1, t1);
        assert!(
            matches!(second, EvaluationOutcome::Suppressed { .. }),
            "expected Suppressed during cooldown, got {second:?}"
        );

        // After cooldown (60s) elapses, still met: fires again immediately,
        // without needing to re-satisfy debounce (streak never broke).
        let t2 = t0 + Duration::from_secs(61);
        let mut freshness2 = FreshnessTracker::new();
        freshness2.observe("dev-1::temperature_c", t2);
        let samples2 = vec![sample("dev-1", 95.0, t2)];
        let third = evaluator.evaluate(&rule, &device("dev-1"), &samples2, &freshness2, t2);
        assert!(
            third.fired(),
            "expected Fired after cooldown elapses, got {third:?}"
        );
    }

    #[test]
    fn debounce_and_cooldown_are_independently_configurable_to_zero() {
        // debounce=0, cooldown=0: fires on the very first met sample AND
        // fires again on the next one — proves the two knobs are separate,
        // not one conflated "rate limit" concept.
        let rule = make_rule(
            Duration::ZERO,
            Duration::ZERO,
            Duration::from_secs(120),
            MissingDataBehavior::TreatAsNotMet,
        );
        let t0 = epoch(1000);
        let mut freshness = FreshnessTracker::new();
        freshness.observe("dev-1::temperature_c", t0);
        let samples = vec![sample("dev-1", 95.0, t0)];
        let mut evaluator = RuleEvaluator::new();
        assert!(
            evaluator
                .evaluate(&rule, &device("dev-1"), &samples, &freshness, t0)
                .fired()
        );

        let t1 = t0 + Duration::from_secs(1);
        let mut freshness1 = FreshnessTracker::new();
        freshness1.observe("dev-1::temperature_c", t1);
        let samples1 = vec![sample("dev-1", 95.0, t1)];
        assert!(
            evaluator
                .evaluate(&rule, &device("dev-1"), &samples1, &freshness1, t1)
                .fired()
        );
    }

    #[test]
    fn condition_going_false_resets_the_debounce_streak() {
        let rule = make_rule(
            Duration::from_secs(30),
            Duration::ZERO,
            Duration::from_secs(120),
            MissingDataBehavior::TreatAsNotMet,
        );
        let t0 = epoch(1000);
        let mut freshness = FreshnessTracker::new();
        freshness.observe("dev-1::temperature_c", t0);
        let met_samples = vec![sample("dev-1", 95.0, t0)];
        let mut evaluator = RuleEvaluator::new();
        let pending = evaluator.evaluate(&rule, &device("dev-1"), &met_samples, &freshness, t0);
        assert!(matches!(pending, EvaluationOutcome::Pending { .. }));

        // Condition drops below threshold at t0+20s (before debounce would
        // have elapsed) - streak must reset.
        let t1 = t0 + Duration::from_secs(20);
        let mut freshness1 = FreshnessTracker::new();
        freshness1.observe("dev-1::temperature_c", t1);
        let not_met_samples = vec![sample("dev-1", 10.0, t1)];
        let not_met =
            evaluator.evaluate(&rule, &device("dev-1"), &not_met_samples, &freshness1, t1);
        assert!(matches!(not_met, EvaluationOutcome::NotMet { .. }));

        // Condition met again at t1+10s: even though the *original* streak
        // started 30s ago, a fresh streak must wait out its own debounce
        // again since it was interrupted.
        let t2 = t1 + Duration::from_secs(10);
        let mut freshness2 = FreshnessTracker::new();
        freshness2.observe("dev-1::temperature_c", t2);
        let met_again = vec![sample("dev-1", 95.0, t2)];
        let outcome = evaluator.evaluate(&rule, &device("dev-1"), &met_again, &freshness2, t2);
        assert!(
            matches!(outcome, EvaluationOutcome::Pending { .. }),
            "expected a fresh Pending after the streak reset, got {outcome:?}"
        );
    }

    #[test]
    fn missing_data_treat_as_met_forces_a_fire_without_real_samples() {
        let rule = make_rule(
            Duration::ZERO,
            Duration::ZERO,
            Duration::from_secs(60),
            MissingDataBehavior::TreatAsMet,
        );
        let t0 = epoch(1000);
        // Freshness tracker has never observed this signal -> Unknown.
        let freshness = FreshnessTracker::new();
        let mut evaluator = RuleEvaluator::new();
        let outcome = evaluator.evaluate(&rule, &device("dev-1"), &[], &freshness, t0);
        match outcome {
            EvaluationOutcome::Fired { data_status, .. } => {
                assert!(matches!(
                    data_status,
                    DataStatus::MissingTreatedAsMet { .. }
                ));
            }
            other => panic!("expected Fired via TreatAsMet, got {other:?}"),
        }
    }

    #[test]
    fn missing_data_treat_as_not_met_never_fires() {
        let rule = make_rule(
            Duration::ZERO,
            Duration::ZERO,
            Duration::from_secs(60),
            MissingDataBehavior::TreatAsNotMet,
        );
        let t0 = epoch(1000);
        let freshness = FreshnessTracker::new();
        let mut evaluator = RuleEvaluator::new();
        let outcome = evaluator.evaluate(&rule, &device("dev-1"), &[], &freshness, t0);
        match outcome {
            EvaluationOutcome::NotMet { data_status } => {
                assert!(matches!(
                    data_status,
                    DataStatus::MissingTreatedAsNotMet { .. }
                ));
            }
            other => panic!("expected NotMet via TreatAsNotMet, got {other:?}"),
        }
    }

    #[test]
    fn missing_data_fire_missing_data_alert_bypasses_debounce_and_condition() {
        let rule = make_rule(
            Duration::from_secs(300), // would otherwise block any fire for 5m
            Duration::ZERO,
            Duration::from_secs(60),
            MissingDataBehavior::FireMissingDataAlert,
        );
        let t0 = epoch(1000);
        let freshness = FreshnessTracker::new();
        let mut evaluator = RuleEvaluator::new();
        let outcome = evaluator.evaluate(&rule, &device("dev-1"), &[], &freshness, t0);
        match outcome {
            EvaluationOutcome::MissingDataAlert {
                missing_signals, ..
            } => {
                assert_eq!(missing_signals, vec!["temperature_c".to_string()]);
            }
            other => panic!("expected MissingDataAlert, got {other:?}"),
        }
    }

    #[test]
    fn a_stale_signal_is_treated_as_missing_even_if_it_was_once_fresh() {
        let rule = make_rule(
            Duration::ZERO,
            Duration::ZERO,
            Duration::from_secs(30),
            MissingDataBehavior::TreatAsNotMet,
        );
        let t0 = epoch(1000);
        let mut freshness = FreshnessTracker::new();
        freshness.observe("dev-1::temperature_c", t0);
        // Sample was well within threshold, but by t0+3600s the signal is
        // far outside the rule's 30s window -> stale, not simply absent.
        let samples = vec![sample("dev-1", 95.0, t0)];
        let t1 = t0 + Duration::from_secs(3600);

        let mut evaluator = RuleEvaluator::new();
        let outcome = evaluator.evaluate(&rule, &device("dev-1"), &samples, &freshness, t1);
        match outcome {
            EvaluationOutcome::NotMet { data_status } => {
                assert!(matches!(
                    data_status,
                    DataStatus::MissingTreatedAsNotMet { .. }
                ));
            }
            other => panic!("expected NotMet via stale-signal TreatAsNotMet, got {other:?}"),
        }
    }

    #[test]
    fn a_restarted_evaluator_honestly_forgets_an_in_progress_streak() {
        // Deliverable 6/7 restart discipline: in-memory debounce state does
        // not survive a "restart" (a fresh RuleEvaluator).
        let rule = make_rule(
            Duration::from_secs(30),
            Duration::ZERO,
            Duration::from_secs(120),
            MissingDataBehavior::TreatAsNotMet,
        );
        let t0 = epoch(1000);
        let mut freshness = FreshnessTracker::new();
        freshness.observe("dev-1::temperature_c", t0);
        let samples = vec![sample("dev-1", 95.0, t0)];

        let mut evaluator = RuleEvaluator::new();
        let pending = evaluator.evaluate(&rule, &device("dev-1"), &samples, &freshness, t0);
        assert!(matches!(pending, EvaluationOutcome::Pending { .. }));

        // "Restart": a brand new evaluator, even though real time has moved
        // forward past the original debounce window.
        let mut restarted_evaluator = RuleEvaluator::new();
        let t2 = t0 + Duration::from_secs(30);
        let mut freshness2 = FreshnessTracker::new();
        freshness2.observe("dev-1::temperature_c", t2);
        let samples2 = vec![sample("dev-1", 95.0, t2)];
        let outcome =
            restarted_evaluator.evaluate(&rule, &device("dev-1"), &samples2, &freshness2, t2);
        assert!(
            matches!(outcome, EvaluationOutcome::Pending { .. }),
            "a restarted evaluator must not fabricate pre-restart debounce progress; got {outcome:?}"
        );
    }
}
