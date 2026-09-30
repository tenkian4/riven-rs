use super::*;
use riven_core::events::Artwork;

pub(crate) async fn get_library_sections(
    http: &riven_core::http::HttpClient,
    plex_url: &str,
    token: &str,
) -> anyhow::Result<Vec<PlexSection>> {
    let url = format!("{plex_url}/library/sections");
    tracing::debug!(target_url = %url, "fetching plex library sections");
    let resp: PlexSectionsResponse = http
        .get_json(PROFILE, url.clone(), |client| {
            client
                .get(&url)
                .header("x-plex-token", token)
                .header("accept", "application/json")
        })
        .await?;

    Ok(resp.media_container.directory)
}

pub(crate) async fn refresh_section(
    http: &riven_core::http::HttpClient,
    plex_url: &str,
    token: &str,
    section_key: &str,
    path: &str,
) -> anyhow::Result<()> {
    let encoded_path = urlencoding::encode(path);
    let url = format!("{plex_url}/library/sections/{section_key}/refresh?path={encoded_path}");
    tracing::debug!(target_url = %url, section_key, path, "refreshing plex library section");
    let response = http
        .send(PROFILE, |client| {
            client
                .post(&url)
                .header("x-plex-token", token)
                .header("accept", "application/json")
        })
        .await?;
    response.error_for_status()?;
    Ok(())
}

/// The largest artwork riven will relay. Plex thumbnails are tens of kilobytes;
/// this only exists so a misbehaving or hostile upstream cannot stream an
/// unbounded body through riven into a browser.
const MAX_ARTWORK_BYTES: usize = 8 * 1024 * 1024;

/// Fetch one artwork image, with the Plex token in a header rather than the URL.
///
/// `reference` arrives from the browser, so it is checked rather than trusted:
/// it must be the absolute, single-segment-rooted `thumb` path Plex itself
/// produced. Without that check a caller could steer this at any path on the
/// Plex server — the request carries an admin token, so it would read whatever
/// it was pointed at, and `..` would let it climb out of `/library` entirely.
pub(crate) async fn get_artwork(
    http: &riven_core::http::HttpClient,
    plex_url: &str,
    token: &str,
    reference: &str,
) -> anyhow::Result<Artwork> {
    anyhow::ensure!(
        reference.starts_with('/')
            && !reference.starts_with("//")
            && !reference.contains("..")
            && !reference.contains('?')
            && !reference.contains('#')
            && !reference.contains('\\'),
        "refusing to fetch artwork for a reference that is not a plain plex path"
    );

    let url = format!("{}{reference}", plex_url.trim_end_matches('/'));
    let response = http
        .send(PROFILE, |client| {
            client
                .get(&url)
                .header("x-plex-token", token)
                .header("accept", "image/*")
        })
        .await?
        .error_for_status()?;

    // Plex answers a bad path with an HTML error page rather than a 4xx in some
    // versions. Serving that back under riven's own origin would be an HTML
    // injection point, so anything that is not an image is refused here.
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string();
    anyhow::ensure!(
        content_type.starts_with("image/"),
        "plex returned {content_type:?} for artwork, which is not an image"
    );

    let bytes = response.bytes().await?;
    anyhow::ensure!(
        bytes.len() <= MAX_ARTWORK_BYTES,
        "plex artwork is {} bytes, over the {MAX_ARTWORK_BYTES} limit",
        bytes.len()
    );

    Ok(Artwork {
        content_type,
        bytes: bytes.to_vec(),
    })
}

/// Find an item's `ratingKey` by title and release year, searching every
/// configured library at once.
///
/// Not a `guid` lookup: Plex's `/library/all?guid=` filter only matches an
/// item's own *primary* guid, which on every modern ("new agent") Plex
/// library is Plex's own opaque identifier (`plex://movie/...`), not the
/// external id — those live in a separate `Guid[]` array the filter does not
/// search at all. Confirmed live against a real Plex 1.43 server: a movie
/// with a correct `tmdb://...` entry in its `Guid[]` still returns zero
/// results from `?guid=tmdb://...`. Title+year is Plex's own fallback for
/// exactly this case (its "Match" UI works the same way) — fast, but not a
/// guaranteed-exact match, since a same-titled remake/reboot with the same
/// release year would collide.
pub(crate) async fn find_rating_key_by_title_year(
    http: &riven_core::http::HttpClient,
    plex_url: &str,
    token: &str,
    title: &str,
    year: Option<i32>,
    plex_type: u8,
) -> anyhow::Result<Option<String>> {
    let url = format!("{plex_url}/library/all");
    tracing::debug!(target_url = %url, title, ?year, "searching plex library by title/year");
    let year_str = year.map(|y| y.to_string());
    let mut query = vec![
        ("title", title),
        ("type", if plex_type == 1 { "1" } else { "2" }),
    ];
    if let Some(ref y) = year_str {
        query.push(("year", y.as_str()));
    }
    let resp: PlexSectionsAllResponse = http
        .get_json(PROFILE, url.clone(), |client| {
            client
                .get(&url)
                .query(&query)
                .header("x-plex-token", token)
                .header("accept", "application/json")
        })
        .await?;
    Ok(resp
        .media_container
        .metadata
        .into_iter()
        .next()
        .map(|item| item.rating_key))
}

/// The server's own persistent identity, needed to build a deep link — Plex's
/// web app addresses a server by this id, not by its (possibly-changing) URL.
pub(crate) async fn get_machine_identifier(
    http: &riven_core::http::HttpClient,
    plex_url: &str,
    token: &str,
) -> anyhow::Result<String> {
    let url = format!("{plex_url}/identity");
    tracing::debug!(target_url = %url, "fetching plex server identity");
    let resp: PlexIdentityResponse = http
        .get_json(PROFILE, url.clone(), |client| {
            client
                .get(&url)
                .header("x-plex-token", token)
                .header("accept", "application/json")
        })
        .await?;
    Ok(resp.media_container.machine_identifier)
}

pub(crate) async fn get_active_sessions(
    http: &riven_core::http::HttpClient,
    plex_url: &str,
    token: &str,
) -> anyhow::Result<Vec<ActivePlaybackSession>> {
    let url = format!("{plex_url}/status/sessions");
    tracing::debug!(target_url = %url, "fetching plex active sessions");
    let resp: PlexSessionsResponse = http
        .get_json(PROFILE, url.clone(), |client| {
            client
                .get(&url)
                .header("x-plex-token", token)
                .header("accept", "application/json")
        })
        .await?;

    Ok(resp
        .media_container
        .metadata
        .into_iter()
        .map(|item| {
            let PlexSessionMetadata {
                item_type,
                title,
                grandparent_title,
                duration,
                parent_index,
                index,
                thumb,
                view_offset,
                transcode_session,
                player,
                user,
            } = item;
            let (device_name, client_name, playback_state) = match player {
                Some(player) => (
                    player.title,
                    player.product,
                    map_playback_state(player.state.as_deref()),
                ),
                None => (None, None, PlaybackState::Unknown),
            };
            let playback_method = map_playback_method(transcode_session.is_some());

            ActivePlaybackSession {
                server: "plex".to_string(),
                user_name: user.and_then(|user| user.title),
                parent_title: grandparent_title.clone(),
                item_title: title
                    .or(grandparent_title)
                    .unwrap_or_else(|| "Unknown item".to_string()),
                item_type,
                season_number: parent_index,
                episode_number: index,
                playback_state,
                playback_method,
                position_seconds: view_offset.and_then(|v| u64::try_from(v / 1000).ok()),
                duration_seconds: duration.and_then(|v| u64::try_from(v / 1000).ok()),
                device_name,
                client_name,
                // A riven-relative path, not a Plex URL. Embedding
                // `?X-Plex-Token=` here handed the Plex server's admin token to
                // every caller of `activePlaybackSessions` — a query any
                // authenticated user can run — and then to the DOM, the browser
                // cache and the `Referer` of whatever the dashboard loaded next.
                // riven proxies the image instead and keeps the token server-side.
                image_url: thumb.as_ref().map(|thumb| artwork_path("plex", thumb)),
            }
        })
        .collect())
}

pub(crate) fn map_playback_state(state: Option<&str>) -> PlaybackState {
    match state.unwrap_or_default().to_ascii_lowercase().as_str() {
        "playing" => PlaybackState::Playing,
        "paused" => PlaybackState::Paused,
        "buffering" => PlaybackState::Buffering,
        "stopped" => PlaybackState::Idle,
        _ => PlaybackState::Unknown,
    }
}

pub(crate) fn map_playback_method(is_transcoding: bool) -> PlaybackMethod {
    if is_transcoding {
        PlaybackMethod::Transcode
    } else {
        PlaybackMethod::DirectPlay
    }
}

#[derive(Deserialize)]
struct PlexSectionsResponse {
    #[serde(rename = "MediaContainer")]
    media_container: PlexMediaContainer,
}

#[derive(Deserialize)]
struct PlexSectionsAllResponse {
    #[serde(rename = "MediaContainer")]
    media_container: PlexAllMediaContainer,
}

#[derive(Deserialize)]
struct PlexAllMediaContainer {
    #[serde(rename = "Metadata", default)]
    metadata: Vec<PlexAllMetadata>,
}

#[derive(Deserialize)]
struct PlexAllMetadata {
    #[serde(rename = "ratingKey")]
    rating_key: String,
}

#[derive(Deserialize)]
struct PlexIdentityResponse {
    #[serde(rename = "MediaContainer")]
    media_container: PlexIdentityContainer,
}

#[derive(Deserialize)]
struct PlexIdentityContainer {
    #[serde(rename = "machineIdentifier")]
    machine_identifier: String,
}

#[derive(Deserialize)]
struct PlexMediaContainer {
    #[serde(rename = "Directory", default)]
    directory: Vec<PlexSection>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct PlexSection {
    pub(crate) key: String,
    #[serde(rename = "Location", default)]
    pub(crate) locations: Vec<PlexLocation>,
}

#[derive(Deserialize, Clone)]
pub(crate) struct PlexLocation {
    pub(crate) path: String,
}

#[derive(Deserialize)]
struct PlexSessionsResponse {
    #[serde(rename = "MediaContainer")]
    media_container: PlexSessionsContainer,
}

#[derive(Deserialize)]
struct PlexSessionsContainer {
    #[serde(rename = "Metadata", default)]
    metadata: Vec<PlexSessionMetadata>,
}

#[derive(Deserialize)]
struct PlexSessionMetadata {
    #[serde(rename = "type")]
    item_type: Option<String>,
    title: Option<String>,
    #[serde(rename = "grandparentTitle")]
    grandparent_title: Option<String>,
    duration: Option<i64>,
    #[serde(rename = "parentIndex")]
    parent_index: Option<i32>,
    index: Option<i32>,
    thumb: Option<String>,
    #[serde(rename = "ViewOffset")]
    view_offset: Option<i64>,
    #[serde(rename = "TranscodeSession")]
    transcode_session: Option<serde_json::Value>,
    #[serde(rename = "Player")]
    player: Option<PlexPlayer>,
    #[serde(rename = "User")]
    user: Option<PlexUser>,
}

#[derive(Deserialize)]
struct PlexPlayer {
    title: Option<String>,
    product: Option<String>,
    state: Option<String>,
}

#[derive(Deserialize)]
struct PlexUser {
    title: Option<String>,
}

pub(crate) mod urlencoding {
    /// Path-segment encoding: `/` is left alone since it is a separator, not
    /// data, in every caller of this function (all of them build a `?path=`
    /// query value that is itself a filesystem path Plex echoes back as-is).
    pub(crate) fn encode(s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' | '/' => result.push(c),
                _ => {
                    for b in c.to_string().as_bytes() {
                        result.push_str(&format!("%{b:02X}"));
                    }
                }
            }
        }
        result
    }

    /// Full query-value encoding, `/` included — for a value that is not
    /// itself a path being handed straight to a `path=`-style param, but
    /// arbitrary data embedded in a query string (Plex's web-app deep link
    /// embeds a `/library/metadata/...` path as the *value* of `key=`, where
    /// an unescaped `/` would be read as part of the URL's own structure).
    pub(crate) fn encode_query_value(s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
                _ => {
                    for b in c.to_string().as_bytes() {
                        result.push_str(&format!("%{b:02X}"));
                    }
                }
            }
        }
        result
    }
}
