use super::*;

fn payload() -> NotificationPayload {
    NotificationPayload {
        event: "riven.media-item.download.success".to_string(),
        title: "Movie".to_string(),
        full_title: "Movie".to_string(),
        item_type: MediaItemType::Movie,
        year: Some(2024),
        imdb_id: Some("tt123".to_string()),
        tmdb_id: Some("456".to_string()),
        tvdb_id: None,
        poster_path: Some("https://image.test/poster.jpg".to_string()),
        downloader: "stremthru".to_string(),
        provider: Some("realdebrid".to_string()),
        duration_seconds: 3661.2,
        timestamp: "2026-04-16T12:00:00Z".to_string(),
        is_anime: false,
        rating: Some(8.25),
        overview: Some("Short overview".to_string()),
        resolution: Some("1080p".to_string()),
        quality: Some("WEB-DL".to_string()),
        release_group: Some("GROUP".to_string()),
        season: None,
        episode: None,
        episode_title: None,
    }
}

#[test]
fn notification_url_parser_supports_discord_and_json_aliases() {
    match parse_notification_url("discord://id/token") {
        Some(NotificationService::Discord {
            webhook_id,
            webhook_token,
        }) => {
            assert_eq!(webhook_id, "id");
            assert_eq!(webhook_token, "token");
        }
        _ => panic!("expected discord URL"),
    }

    match parse_notification_url("jsons://hooks.example/path") {
        Some(NotificationService::Json { url }) => {
            assert_eq!(url, "https://hooks.example/path");
        }
        _ => panic!("expected json URL"),
    }

    assert!(parse_notification_url("ftp://example.test").is_none());
}

#[test]
fn notification_url_parser_supports_pushbullet() {
    match parse_notification_url("pbul://o.abc123token") {
        Some(NotificationService::Pushbullet { access_token }) => {
            assert_eq!(access_token, "o.abc123token");
        }
        _ => panic!("expected pushbullet URL"),
    }

    // A trailing slash with nothing after it is still a plain-token URL.
    match parse_notification_url("pbul://o.abc123token/") {
        Some(NotificationService::Pushbullet { access_token }) => {
            assert_eq!(access_token, "o.abc123token");
        }
        _ => panic!("expected pushbullet URL"),
    }
}

#[test]
fn notification_url_parser_rejects_pushbullet_urls_with_no_token() {
    assert!(parse_notification_url("pbul://").is_none());
    assert!(parse_notification_url("pbul:///some-device-id").is_none());
}

#[test]
fn notification_url_parser_rejects_pushbullet_urls_with_a_target() {
    // A device/channel/email target (Apprise's `pbul://token/#channel`,
    // `.../DEVICE_ID`, `.../email@address`) isn't supported. send_pushbullet
    // never sends a target parameter, and Pushbullet broadcasts to every
    // device on the account when none is set — so a target must be rejected
    // outright rather than silently dropped, which would otherwise leak the
    // notification to devices the user meant to exclude.
    assert!(parse_notification_url("pbul://o.abc123token/some-device-id").is_none());
    assert!(parse_notification_url("pbul://o.abc123token/#a-channel").is_none());
}

#[test]
fn pushbullet_body_condenses_the_core_fields_into_one_line() {
    let body = build_pushbullet_body(&payload());
    assert_eq!(
        body,
        "Movie • 2024 • via stremthru • realdebrid • in 1h 1m 1s"
    );
}

#[test]
fn duration_formatter_uses_human_units() {
    assert_eq!(format_duration(12.4), "12.4s");
    assert_eq!(format_duration(125.0), "2m 5s");
    assert_eq!(format_duration(3661.0), "1h 1m 1s");
    assert_eq!(format_duration(-1.0), "-1.0s");
}

#[test]
fn simple_embed_contains_core_download_fields() {
    let body = build_simple_embed(&payload());
    let embed = &body["embeds"][0];

    assert_eq!(embed["title"], "Downloaded: Movie");
    assert_eq!(embed["thumbnail"]["url"], "https://image.test/poster.jpg");
    assert!(
        embed["fields"]
            .as_array()
            .expect("fields array")
            .iter()
            .any(|field| field["name"] == "Provider" && field["value"] == "realdebrid")
    );
}

#[test]
fn ntfy_url_parser_defaults_to_the_public_server() {
    match parse_notification_url("ntfy://mytopic") {
        Some(NotificationService::Ntfy {
            base_url,
            topic,
            auth: NtfyAuth::None,
            priority: None,
            tags: None,
        }) => {
            assert_eq!(base_url, "https://ntfy.sh");
            assert_eq!(topic, "mytopic");
        }
        _ => panic!("expected a public-server ntfy URL"),
    }

    // `ntfys://` is still just the public server when no host is given.
    match parse_notification_url("ntfys://mytopic") {
        Some(NotificationService::Ntfy { base_url, .. }) => {
            assert_eq!(base_url, "https://ntfy.sh");
        }
        _ => panic!("expected a public-server ntfy URL"),
    }
}

#[test]
fn ntfy_url_parser_supports_self_hosted_servers() {
    match parse_notification_url("ntfy://myhost.local/mytopic") {
        Some(NotificationService::Ntfy {
            base_url, topic, ..
        }) => {
            assert_eq!(base_url, "http://myhost.local");
            assert_eq!(topic, "mytopic");
        }
        _ => panic!("expected a self-hosted ntfy URL"),
    }

    match parse_notification_url("ntfys://myhost.local:8080/mytopic") {
        Some(NotificationService::Ntfy { base_url, .. }) => {
            assert_eq!(base_url, "https://myhost.local:8080");
        }
        _ => panic!("expected a secure self-hosted ntfy URL"),
    }

    // The public server has no plaintext endpoint, so an explicit host of
    // ntfy.sh is forced to https even under the plain `ntfy://` scheme.
    match parse_notification_url("ntfy://ntfy.sh/mytopic") {
        Some(NotificationService::Ntfy { base_url, .. }) => {
            assert_eq!(base_url, "https://ntfy.sh");
        }
        _ => panic!("expected ntfy.sh to be forced to https"),
    }
}

#[test]
fn ntfy_url_parser_supports_basic_and_token_auth() {
    match parse_notification_url("ntfy://user:pass@myhost/mytopic") {
        Some(NotificationService::Ntfy {
            auth: NtfyAuth::Basic { user, password },
            ..
        }) => {
            assert_eq!(user, "user");
            assert_eq!(password, "pass");
        }
        _ => panic!("expected basic auth"),
    }

    match parse_notification_url("ntfy://tk_abc123@ntfy.sh/mytopic") {
        Some(NotificationService::Ntfy {
            auth: NtfyAuth::Token(token),
            ..
        }) => {
            assert_eq!(token, "tk_abc123");
        }
        _ => panic!("expected token auth"),
    }
}

#[test]
fn ntfy_url_parser_reads_priority_and_tags() {
    match parse_notification_url("ntfy://mytopic?priority=high&tags=warning,skull") {
        Some(NotificationService::Ntfy { priority, tags, .. }) => {
            assert_eq!(priority.as_deref(), Some("high"));
            assert_eq!(tags.as_deref(), Some("warning,skull"));
        }
        _ => panic!("expected priority and tags to be parsed"),
    }

    // An unrecognized priority is dropped rather than passed through blindly.
    match parse_notification_url("ntfy://mytopic?priority=bogus") {
        Some(NotificationService::Ntfy { priority, .. }) => assert_eq!(priority, None),
        _ => panic!("expected an ntfy URL"),
    }
}

#[test]
fn ntfy_url_parser_rejects_a_missing_topic() {
    assert!(parse_notification_url("ntfy://").is_none());
    assert!(parse_notification_url("ntfy://myhost/").is_none());
}

#[test]
fn ntfy_url_parser_rejects_a_multi_segment_topic() {
    // `/` isn't a valid character in an ntfy topic name, so a second
    // segment here reads as a topic path rather than the supported
    // single-topic form.
    assert!(parse_notification_url("ntfy://host/topic-one/topic-two").is_none());
}

#[test]
fn ntfy_body_condenses_the_core_fields_and_attaches_the_poster() {
    let body = build_ntfy_body("mytopic", None, None, &payload());

    assert_eq!(body["topic"], "mytopic");
    assert_eq!(body["title"], "Downloaded: Movie");
    assert_eq!(
        body["message"],
        "Movie • 2024 • via stremthru • realdebrid • in 1h 1m 1s"
    );
    assert_eq!(body["attach"], "https://image.test/poster.jpg");
    assert!(body.get("priority").is_none());
    assert!(body.get("tags").is_none());
}

#[test]
fn ntfy_body_includes_priority_and_tags_as_a_json_array() {
    let body = build_ntfy_body("mytopic", Some("high"), Some("warning, skull"), &payload());

    assert_eq!(body["priority"], "high");
    assert_eq!(body["tags"], serde_json::json!(["warning", "skull"]));
}

#[test]
fn truncate_chars_is_a_no_op_at_or_under_the_limit() {
    assert_eq!(truncate_chars("hello", 5), "hello");
    assert_eq!(truncate_chars("hello", 10), "hello");
}

#[test]
fn truncate_chars_shortens_and_marks_anything_over_the_limit() {
    let truncated = truncate_chars("hello world", 5);
    assert_eq!(truncated.chars().count(), 5);
    assert!(truncated.starts_with("hell"));
    assert!(truncated.ends_with('…'));
}

#[test]
fn truncate_chars_counts_chars_not_bytes_so_multi_byte_text_is_not_split_mid_character() {
    // Each "é" is 2 UTF-8 bytes; a byte-index truncation at 5 would split one
    // in half and produce invalid UTF-8 (or panic). char-based truncation
    // must not.
    let truncated = truncate_chars("ééééé", 3);
    assert_eq!(truncated.chars().count(), 3);
    assert!(truncated.ends_with('…'));
}

#[test]
fn custom_embed_truncates_title_and_description_to_discords_limits() {
    let long_title = "T".repeat(300);
    let long_body = "B".repeat(5000);
    let embed =
        build_custom_embed(&payload(), Some(&long_title), Some(&long_body))["embeds"][0].clone();

    assert_eq!(embed["title"].as_str().unwrap().chars().count(), 256);
    assert_eq!(embed["description"].as_str().unwrap().chars().count(), 4096);
}
