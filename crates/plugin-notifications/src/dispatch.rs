use super::*;

/// Sends the notification to every configured target. Returns `true` only if
/// every one accepted it — `on_download_success` ignores this (a completed
/// download must not fail over a flaky webhook), but the test-notification
/// path uses it to report an actual failure instead of a false "sent".
pub(crate) async fn dispatch_webhooks(
    ctx: &PluginContext,
    urls: &[String],
    payload: &NotificationPayload,
    detailed: bool,
    custom_title: Option<&str>,
    custom_body: Option<&str>,
) -> bool {
    let mut all_ok = !urls.is_empty();
    for url_str in urls {
        match parse_notification_url(url_str) {
            Some(NotificationService::Discord {
                webhook_id,
                webhook_token,
            }) => {
                if let Err(error) = send_discord(
                    &ctx.http,
                    &webhook_id,
                    &webhook_token,
                    payload,
                    detailed,
                    custom_title,
                    custom_body,
                )
                .await
                {
                    all_ok = false;
                    // Never log `url_str` here — it's the full discord.com
                    // webhook URL, which is itself a bearer credential.
                    tracing::error!(error = %error, service = "discord", "failed to send discord notification");
                }
            }
            Some(NotificationService::Pushbullet { access_token }) => {
                if let Err(error) =
                    send_pushbullet(&ctx.http, &access_token, payload, custom_title, custom_body)
                        .await
                {
                    all_ok = false;
                    // Never log `url_str` here — it embeds the Pushbullet
                    // access token, which grants full account access.
                    tracing::error!(error = %error, service = "pushbullet", "failed to send pushbullet notification");
                }
            }
            Some(NotificationService::Json { url }) => {
                if let Err(error) = send_json_webhook(&ctx.http, &url, payload).await {
                    all_ok = false;
                    // Generic webhook URLs commonly embed a secret path
                    // segment too, so this omits `url_str`/`url` as well.
                    tracing::error!(error = %error, service = "json", "failed to send json notification");
                }
            }
            Some(NotificationService::Ntfy {
                base_url,
                topic,
                auth,
                priority,
                tags,
            }) => {
                if let Err(error) = send_ntfy(
                    &ctx.http,
                    &base_url,
                    &topic,
                    &auth,
                    priority.as_deref(),
                    tags.as_deref(),
                    payload,
                )
                .await
                {
                    // Never log `url_str` here — a basic-auth password or
                    // bearer token in the URL would end up in the logs.
                    tracing::error!(error = %error, endpoint = %base_url, "failed to send ntfy notification");
                }
            }
            None => {
                all_ok = false;
                // Never log `url_str` here — a rejected pbul:// URL still
                // embeds the Pushbullet access token even though parsing
                // failed.
                tracing::warn!("unsupported notification URL scheme");
            }
        }
    }
    all_ok
}

pub(crate) enum NotificationService {
    Discord {
        webhook_id: String,
        webhook_token: String,
    },
    Pushbullet {
        access_token: String,
    },
    Json {
        url: String,
    },
    Ntfy {
        base_url: String,
        topic: String,
        auth: NtfyAuth,
        priority: Option<String>,
        tags: Option<String>,
    },
}

pub(crate) enum NtfyAuth {
    None,
    Basic { user: String, password: String },
    Token(String),
}

pub(crate) fn parse_notification_url(url: &str) -> Option<NotificationService> {
    if let Some(rest) = url
        .strip_prefix("discord://")
        .or_else(|| url.strip_prefix("https://discord.com/api/webhooks/"))
    {
        let (webhook_id, webhook_token) = rest.split_once('/')?;
        Some(NotificationService::Discord {
            webhook_id: webhook_id.to_string(),
            webhook_token: webhook_token.to_string(),
        })
    } else if let Some(rest) = url.strip_prefix("pbul://") {
        // Apprise's pushbullet scheme also allows `pbul://token/#channel`,
        // `.../DEVICE_ID`, `.../email@address` for targeted delivery — not
        // supported here. A target segment is rejected outright rather than
        // silently dropped: send_pushbullet never sends a target parameter,
        // and Pushbullet broadcasts to every device on the account when no
        // target is set, so silently dropping a configured target would leak
        // the notification to devices the user deliberately excluded.
        // A missing/empty token (`pbul://`, `pbul:///device-id`) is rejected
        // outright too, rather than reaching send_pushbullet with an empty
        // Access-Token header.
        let (token, target) = rest.split_once('/').unwrap_or((rest, ""));
        if token.is_empty() || !target.is_empty() {
            None
        } else {
            Some(NotificationService::Pushbullet {
                access_token: token.to_string(),
            })
        }
    } else if let Some(rest) = url.strip_prefix("json://") {
        Some(NotificationService::Json {
            url: format!("http://{rest}"),
        })
    } else if let Some(rest) = url.strip_prefix("jsons://") {
        Some(NotificationService::Json {
            url: format!("https://{rest}"),
        })
    } else if url.starts_with("ntfy://") || url.starts_with("ntfys://") {
        parse_ntfy_url(url)
    } else {
        None
    }
}

/// Apprise's ntfy scheme: `ntfy://topic` (public ntfy.sh), `ntfy://host/topic`
/// or `ntfys://host/topic` (self-hosted, http/https respectively), each
/// optionally prefixed with `user:pass@` (Basic auth) or `token@` (Bearer
/// auth). Only a single topic is supported — Apprise's `{topic1}/{topic2}`
/// multi-topic form is not, since without an explicit host there is no way
/// to tell a second topic segment apart from a hostname.
fn parse_ntfy_url(url: &str) -> Option<NotificationService> {
    let (secure_scheme, rest) = if let Some(rest) = url.strip_prefix("ntfys://") {
        (true, rest)
    } else {
        (false, url.strip_prefix("ntfy://")?)
    };

    let (rest, query) = match rest.split_once('?') {
        Some((path, q)) => (path, Some(q)),
        None => (rest, None),
    };

    let (auth_part, after_auth) = match rest.split_once('@') {
        Some((a, b)) => (Some(a), b),
        None => (None, rest),
    };

    let (base_url, topic) = match after_auth.split_once('/') {
        // `/` is not a valid character in an ntfy topic name, so a further
        // `/` inside `topic` means this is a multi-segment path (e.g.
        // `host/topic-one/topic-two`) — not the single-topic form this
        // supports.
        Some((host, topic)) if !host.is_empty() && !topic.is_empty() && !topic.contains('/') => {
            // ntfy.sh itself has no plaintext endpoint, so an explicit
            // `ntfy://ntfy.sh/topic` still needs to be forced to https.
            let scheme = if secure_scheme || host.eq_ignore_ascii_case("ntfy.sh") {
                "https"
            } else {
                "http"
            };
            (format!("{scheme}://{host}"), topic.to_string())
        }
        // A `/` is present but the host or topic on one side of it is empty
        // (e.g. `myhost/` or `/topic`), or the topic itself has a further
        // `/` — not a valid self-hosted target.
        Some(_) => return None,
        None if !after_auth.is_empty() => ("https://ntfy.sh".to_string(), after_auth.to_string()),
        None => return None,
    };

    let auth = match auth_part {
        None => NtfyAuth::None,
        Some(a) => match a.split_once(':') {
            Some((user, password)) => NtfyAuth::Basic {
                user: user.to_string(),
                password: password.to_string(),
            },
            None => NtfyAuth::Token(a.to_string()),
        },
    };

    let mut priority = None;
    let mut tags = None;
    for pair in query.into_iter().flat_map(|q| q.split('&')) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        match key {
            "priority" if matches!(value, "max" | "high" | "low" | "min") => {
                priority = Some(value.to_string());
            }
            "tags" if !value.is_empty() => {
                tags = Some(value.to_string());
            }
            _ => {}
        }
    }

    Some(NotificationService::Ntfy {
        base_url,
        topic,
        auth,
        priority,
        tags,
    })
}

async fn send_discord(
    http: &riven_core::http::HttpClient,
    webhook_id: &str,
    webhook_token: &str,
    payload: &NotificationPayload,
    detailed: bool,
    custom_title: Option<&str>,
    custom_body: Option<&str>,
) -> anyhow::Result<()> {
    let url = format!("https://discord.com/api/webhooks/{webhook_id}/{webhook_token}");
    let body = if custom_title.is_some() || custom_body.is_some() {
        build_custom_embed(payload, custom_title, custom_body)
    } else if detailed {
        build_detailed_embed(payload)
    } else {
        build_simple_embed(payload)
    };
    tracing::debug!(
        webhook_id,
        title = %payload.full_title,
        "sending discord notification webhook"
    );
    http.send(profiles::DISCORD_WEBHOOK, |client| {
        client.post(&url).json(&body)
    })
    .await?
    .error_for_status()?;
    Ok(())
}

pub(crate) fn build_simple_embed(payload: &NotificationPayload) -> serde_json::Value {
    let duration_str = format_duration(payload.duration_seconds);

    let mut fields = vec![
        serde_json::json!({ "name": "Type", "value": format!("{:?}", payload.item_type), "inline": true }),
        serde_json::json!({ "name": "Downloader", "value": &payload.downloader, "inline": true }),
        serde_json::json!({ "name": "Duration", "value": duration_str, "inline": true }),
    ];

    if let Some(ref provider) = payload.provider {
        fields.push(serde_json::json!({ "name": "Provider", "value": provider, "inline": true }));
    }

    if let Some(year) = payload.year {
        fields
            .push(serde_json::json!({ "name": "Year", "value": year.to_string(), "inline": true }));
    }

    let mut embed = serde_json::json!({
        "title": format!("Downloaded: {}", payload.full_title),
        "color": 0x2ecc71,
        "fields": fields,
        "timestamp": &payload.timestamp,
    });

    if let Some(ref poster) = payload.poster_path {
        embed["thumbnail"] = serde_json::json!({ "url": poster });
    }

    serde_json::json!({ "embeds": [embed] })
}

/// Human label for the item type, factoring in the anime override — shared
/// between the detailed and custom embed builders.
fn media_label(payload: &NotificationPayload) -> &'static str {
    if payload.is_anime {
        "Anime"
    } else {
        match payload.item_type {
            MediaItemType::Movie => "Movie",
            MediaItemType::Show => "Show",
            MediaItemType::Season => "Season",
            MediaItemType::Episode => "Episode",
        }
    }
}

/// Embed accent color by item type, factoring in the anime override —
/// shared between the detailed and custom embed builders.
fn embed_color(payload: &NotificationPayload) -> u32 {
    if payload.is_anime {
        0x9B59B6
    } else {
        match payload.item_type {
            MediaItemType::Movie => 0xE67E22,
            MediaItemType::Show | MediaItemType::Season | MediaItemType::Episode => 0x3498DB,
        }
    }
}

/// A custom title/body template rendered into Discord's embed shape — the
/// visual chrome (color, poster thumbnail, timestamp) is kept, but the
/// structured fields/description of the default embeds are replaced
/// entirely by the rendered body, since a user writing their own template
/// is opting out of the auto-generated layout, not just its wording.
/// Discord's hard per-embed caps (<https://discord.com/safety/using-webhooks-and-embeds>):
/// title 256 chars, description 4096 chars. Exceeding either makes Discord
/// reject the whole webhook request with a 400, so a rendered custom
/// template — which can be arbitrarily long once `{{overview}}` or a verbose
/// body template is in the mix — is truncated before being sent.
const DISCORD_EMBED_TITLE_MAX_CHARS: usize = 256;
const DISCORD_EMBED_DESCRIPTION_MAX_CHARS: usize = 4096;

/// Truncates at a `char` boundary (never splits a multi-byte UTF-8
/// sequence) and appends an ellipsis when truncation actually happened.
pub(crate) fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    truncated.push('…');
    truncated
}

pub(crate) fn build_custom_embed(
    payload: &NotificationPayload,
    custom_title: Option<&str>,
    custom_body: Option<&str>,
) -> serde_json::Value {
    let title = custom_title
        .map(str::to_string)
        .unwrap_or_else(|| format!("Downloaded: {}", payload.full_title));
    let title = truncate_chars(&title, DISCORD_EMBED_TITLE_MAX_CHARS);

    let mut embed = serde_json::json!({
        "title": title,
        "color": embed_color(payload),
        "timestamp": &payload.timestamp,
    });

    if let Some(body) = custom_body {
        embed["description"] =
            serde_json::json!(truncate_chars(body, DISCORD_EMBED_DESCRIPTION_MAX_CHARS));
    }
    if let Some(ref poster) = payload.poster_path {
        embed["thumbnail"] = serde_json::json!({ "url": poster });
    }

    serde_json::json!({ "embeds": [embed] })
}

fn build_detailed_embed(payload: &NotificationPayload) -> serde_json::Value {
    let media_label = media_label(payload);
    let color: u32 = embed_color(payload);

    let title = match payload.year {
        Some(year) => format!("{} ({})", payload.full_title, year),
        None => payload.full_title.clone(),
    };

    let description = match &payload.overview {
        Some(overview) => {
            let truncated = if overview.len() > 500 {
                &overview[..500]
            } else {
                overview.as_str()
            };
            format!("**✅ {media_label} Completed Successfully**\n\n{truncated}")
        }
        None => format!("**✅ {media_label} Completed Successfully**"),
    };

    let mut fields = vec![];

    if let Some(rating) = payload.rating {
        fields.push(serde_json::json!({
            "name": "⭐ Rating",
            "value": format!("{:.2} / 10", rating),
            "inline": false,
        }));
    }

    fields.push(serde_json::json!({
        "name": "⏱ Completion Time",
        "value": format_duration(payload.duration_seconds),
        "inline": false,
    }));

    fields.push(serde_json::json!({
        "name": "Downloader",
        "value": &payload.downloader,
        "inline": true,
    }));

    if let Some(ref provider) = payload.provider {
        fields.push(serde_json::json!({
            "name": "Provider",
            "value": provider,
            "inline": true,
        }));
    }

    let mut links = vec![];
    if let Some(ref tmdb_id) = payload.tmdb_id {
        let path = if payload.item_type == MediaItemType::Movie {
            "movie"
        } else {
            "tv"
        };
        links.push(format!(
            "[TMDB](https://www.themoviedb.org/{path}/{tmdb_id})"
        ));
    }
    if let Some(ref imdb_id) = payload.imdb_id {
        links.push(format!("[IMDB](https://www.imdb.com/title/{imdb_id})"));
    }
    if let Some(ref tvdb_id) = payload.tvdb_id {
        let path = if payload.item_type == MediaItemType::Movie {
            "movie"
        } else {
            "series"
        };
        links.push(format!(
            "[TVDB](https://thetvdb.com/dereferrer/{path}/{tvdb_id})"
        ));
    }
    if !links.is_empty() {
        fields.push(serde_json::json!({
            "name": "Links",
            "value": links.join(" • "),
            "inline": false,
        }));
    }

    let mut embed = serde_json::json!({
        "title": title,
        "description": description,
        "color": color,
        "fields": fields,
        "timestamp": &payload.timestamp,
        "footer": { "text": "Riven" },
    });

    if let Some(ref poster) = payload.poster_path {
        embed["image"] = serde_json::json!({ "url": poster });
    }

    serde_json::json!({ "embeds": [embed] })
}

async fn send_pushbullet(
    http: &riven_core::http::HttpClient,
    access_token: &str,
    payload: &NotificationPayload,
    custom_title: Option<&str>,
    custom_body: Option<&str>,
) -> anyhow::Result<()> {
    let title = custom_title
        .map(str::to_string)
        .unwrap_or_else(|| format!("Downloaded: {}", payload.full_title));
    let body = custom_body
        .map(str::to_string)
        .unwrap_or_else(|| build_pushbullet_body(payload));
    tracing::debug!(
        title = %payload.full_title,
        "sending pushbullet notification"
    );
    http.send(profiles::PUSHBULLET, |client| {
        client
            .post("https://api.pushbullet.com/v2/pushes")
            .header("Access-Token", access_token)
            .json(&serde_json::json!({
                "type": "note",
                "title": title,
                "body": body,
            }))
    })
    .await?
    .error_for_status()?;
    Ok(())
}

/// Pushbullet notes are plain text — no embed support — so this condenses
/// the same core fields `build_simple_embed` shows into one line.
pub(crate) fn build_pushbullet_body(payload: &NotificationPayload) -> String {
    let mut parts = vec![format!("{:?}", payload.item_type)];
    if let Some(year) = payload.year {
        parts.push(year.to_string());
    }
    parts.push(format!("via {}", payload.downloader));
    if let Some(ref provider) = payload.provider {
        parts.push(provider.clone());
    }
    parts.push(format!("in {}", format_duration(payload.duration_seconds)));
    parts.join(" • ")
}

async fn send_json_webhook(
    http: &riven_core::http::HttpClient,
    url: &str,
    payload: &NotificationPayload,
) -> anyhow::Result<()> {
    tracing::debug!(
        target_url = %url,
        title = %payload.full_title,
        "sending json notification webhook"
    );
    http.send(profiles::WEBHOOK_JSON, |client| {
        client.post(url).json(payload)
    })
    .await?
    .error_for_status()?;
    Ok(())
}

async fn send_ntfy(
    http: &riven_core::http::HttpClient,
    base_url: &str,
    topic: &str,
    auth: &NtfyAuth,
    priority: Option<&str>,
    tags: Option<&str>,
    payload: &NotificationPayload,
) -> anyhow::Result<()> {
    // ntfy only parses a JSON publish body at the root endpoint — POSTing
    // JSON straight to `/{topic}` (as this used to) gets treated as a plain
    // publish and the raw JSON text ends up as the literal message body.
    // The root form also keeps title/message/tags fully UTF-8 safe: the
    // header-based per-topic form would need every non-ASCII byte (accented
    // titles, the "•" separator below) RFC 2047-encoded, since HTTP header
    // values are ASCII-only.
    let body = build_ntfy_body(topic, priority, tags, payload);
    tracing::debug!(
        topic,
        title = %payload.full_title,
        "sending ntfy notification"
    );
    http.send(profiles::NTFY, |client| {
        let request = client.post(base_url).json(&body);
        match auth {
            NtfyAuth::None => request,
            NtfyAuth::Basic { user, password } => request.basic_auth(user, Some(password)),
            NtfyAuth::Token(token) => request.bearer_auth(token),
        }
    })
    .await?
    .error_for_status()?;
    Ok(())
}

pub(crate) fn build_ntfy_body(
    topic: &str,
    priority: Option<&str>,
    tags: Option<&str>,
    payload: &NotificationPayload,
) -> serde_json::Value {
    let mut body = serde_json::json!({
        "topic": topic,
        "title": format!("Downloaded: {}", payload.full_title),
        "message": build_ntfy_message(payload),
    });
    if let Some(priority) = priority {
        body["priority"] = serde_json::json!(priority);
    }
    if let Some(tags) = tags {
        body["tags"] = serde_json::json!(tags.split(',').map(str::trim).collect::<Vec<_>>());
    }
    if let Some(ref poster) = payload.poster_path {
        body["attach"] = serde_json::json!(poster);
    }
    body
}

/// ntfy's `message` field is plain text — no embed support — so this
/// condenses the same core fields `build_simple_embed` shows into one line.
fn build_ntfy_message(payload: &NotificationPayload) -> String {
    let mut parts = vec![format!("{:?}", payload.item_type)];
    if let Some(year) = payload.year {
        parts.push(year.to_string());
    }
    parts.push(format!("via {}", payload.downloader));
    if let Some(ref provider) = payload.provider {
        parts.push(provider.clone());
    }
    parts.push(format!("in {}", format_duration(payload.duration_seconds)));
    parts.join(" • ")
}

pub(crate) fn format_duration(seconds: f64) -> String {
    let total = u64::try_from(seconds.round().max(0.0) as i64).unwrap_or(0);
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let secs = total % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m {secs}s")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{seconds:.1}s")
    }
}
