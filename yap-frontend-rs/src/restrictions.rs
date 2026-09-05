use crate::ChallengeRequirements;
use wasm_bindgen::prelude::*;

const RESTRICTION_DURATION_MS: f64 = 15.0 * 60.0 * 1000.0;

#[derive(Clone, Debug, serde::Serialize, tsify::Tsify)]
#[tsify(into_wasm_abi)]
pub struct ChallengeRestrictions {
    pub banned: Vec<ChallengeRequirements>,
    pub next_expiry_ms: Option<f64>,
}

/// Timestamps are device-local host state. Expiry is evaluated with the host's
/// clock; an active review should finish before the host applies expired bans.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn get_challenge_restrictions(
    listening_since: Option<f64>,
    speaking_since: Option<f64>,
    now_ms: f64,
) -> ChallengeRestrictions {
    let mut result = ChallengeRestrictions {
        banned: vec![],
        next_expiry_ms: None,
    };
    for (since, requirement) in [
        (listening_since, ChallengeRequirements::Listening),
        (speaking_since, ChallengeRequirements::Speaking),
    ] {
        if let Some(expiry) = since
            .filter(|t| t.is_finite())
            .map(|t| t + RESTRICTION_DURATION_MS)
            .filter(|expiry| *expiry > now_ms)
        {
            result.banned.push(requirement);
            result.next_expiry_ms = Some(
                result
                    .next_expiry_ms
                    .map_or(expiry, |previous| previous.min(expiry)),
            );
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restrictions_expire_independently_at_the_boundary() {
        let state = get_challenge_restrictions(Some(0.0), Some(100.0), 899_999.0);
        assert_eq!(
            state.banned,
            vec![
                ChallengeRequirements::Listening,
                ChallengeRequirements::Speaking
            ]
        );
        assert_eq!(state.next_expiry_ms, Some(900_000.0));
        let state = get_challenge_restrictions(Some(0.0), Some(100.0), 900_000.0);
        assert_eq!(state.banned, vec![ChallengeRequirements::Speaking]);
        assert_eq!(state.next_expiry_ms, Some(900_100.0));
        assert!(
            get_challenge_restrictions(Some(0.0), Some(100.0), 900_100.0)
                .banned
                .is_empty()
        );
    }
    #[test]
    fn malformed_storage_is_ignored_and_future_timestamps_preserve_clock_rollback_behavior() {
        assert!(
            get_challenge_restrictions(Some(f64::NAN), None, 0.0)
                .banned
                .is_empty()
        );
        assert!(
            get_challenge_restrictions(Some(f64::INFINITY), None, 0.0)
                .banned
                .is_empty()
        );
        assert_eq!(
            get_challenge_restrictions(None, Some(100.0), 0.0).next_expiry_ms,
            Some(900_100.0)
        );
    }
}
