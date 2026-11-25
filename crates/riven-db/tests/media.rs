use riven_db::repo::media::cooldown_for_failed_attempts;

// Must mirror `FAILED_ATTEMPTS_COOLDOWN_SQL`'s tiers exactly — this is the
// job-level retry's only backoff signal, since it doesn't go through
// `get_pending_items_for_retry`'s SQL filter.

#[test]
fn base_tier_under_two_failures() {
    assert_eq!(
        cooldown_for_failed_attempts(0),
        chrono::Duration::minutes(30)
    );
    assert_eq!(
        cooldown_for_failed_attempts(1),
        chrono::Duration::minutes(30)
    );
}

#[test]
fn escalates_at_two_failures() {
    assert_eq!(cooldown_for_failed_attempts(2), chrono::Duration::hours(2));
    assert_eq!(cooldown_for_failed_attempts(4), chrono::Duration::hours(2));
}

#[test]
fn escalates_at_five_failures() {
    assert_eq!(cooldown_for_failed_attempts(5), chrono::Duration::hours(6));
    assert_eq!(cooldown_for_failed_attempts(9), chrono::Duration::hours(6));
}

#[test]
fn escalates_at_ten_failures() {
    assert_eq!(
        cooldown_for_failed_attempts(10),
        chrono::Duration::hours(24)
    );
    assert_eq!(
        cooldown_for_failed_attempts(1000),
        chrono::Duration::hours(24)
    );
}

mod cooldown {
    use chrono::{Duration, TimeZone, Utc};
    use riven_db::repo::media::is_in_retry_cooldown;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 25, 12, 0, 0).unwrap()
    }

    /// Fresh work must never be held back: an item that has not failed has no
    /// backoff to wait out, whatever its last attempt says.
    #[test]
    fn an_item_that_never_failed_is_never_in_cooldown() {
        assert!(!is_in_retry_cooldown(0, None, now()));
        assert!(!is_in_retry_cooldown(
            0,
            Some(now() - Duration::minutes(1)),
            now()
        ));
    }

    #[test]
    fn a_failed_item_with_no_recorded_attempt_is_not_in_cooldown() {
        assert!(!is_in_retry_cooldown(5, None, now()));
    }

    /// The bug: a show's fan-out pushed an episode with 57 failed attempts that
    /// was last tried ten minutes ago, every ten minutes.
    #[test]
    fn a_much_failed_item_tried_recently_is_in_cooldown() {
        assert!(is_in_retry_cooldown(
            57,
            Some(now() - Duration::minutes(10)),
            now()
        ));
        assert!(is_in_retry_cooldown(
            57,
            Some(now() - Duration::hours(23)),
            now()
        ));
    }

    #[test]
    fn the_cooldown_lapses_on_each_tier_boundary() {
        // (failed attempts, that tier's wait): 1 -> 30 min, 2 -> 2 h, 5 -> 6 h, 10 -> 24 h.
        for (failed, wait) in [
            (1, Duration::minutes(30)),
            (2, Duration::hours(2)),
            (5, Duration::hours(6)),
            (10, Duration::hours(24)),
        ] {
            let just_inside = now() - (wait - Duration::seconds(1));
            let just_outside = now() - (wait + Duration::seconds(1));
            assert!(
                is_in_retry_cooldown(failed, Some(just_inside), now()),
                "{failed}: inside"
            );
            assert!(
                !is_in_retry_cooldown(failed, Some(just_outside), now()),
                "{failed}: outside"
            );
        }
    }
}
