use async_graphql::{Context, SimpleObject, Subscription};
use futures::stream::{Stream, StreamExt};
use std::sync::Arc;
use tokio::sync::broadcast;

use super::broadcast_stream;

#[derive(SimpleObject, Clone)]
pub struct RivenNotification {
    pub event_type: String,
    pub title: Option<String>,
    pub full_title: Option<String>,
    /// For an episode or season: the show it belongs to. Its own `title` is
    /// only the episode or season name.
    pub show_title: Option<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub item_type: Option<String>,
    pub year: Option<i32>,
    pub imdb_id: Option<String>,
    pub tmdb_id: Option<String>,
    pub tvdb_id: Option<String>,
    pub duration_seconds: Option<f64>,
    pub id: Option<i64>,
    pub stream_count: Option<i64>,
    pub count: Option<i64>,
    pub new_items: Option<i64>,
    pub error: Option<String>,
}

fn notification_from_json(v: &serde_json::Value) -> Option<RivenNotification> {
    let s = |key: &str| v.get(key).and_then(|x| x.as_str()).map(str::to_string);
    let i = |key: &str| {
        v.get(key)
            .and_then(serde_json::Value::as_i64)
            .and_then(|x| i32::try_from(x).ok())
    };
    Some(RivenNotification {
        event_type: s("type")?,
        title: s("title"),
        full_title: s("full_title"),
        show_title: s("show_title"),
        season_number: i("season_number"),
        episode_number: i("episode_number"),
        item_type: s("item_type"),
        year: v
            .get("year")
            .and_then(serde_json::Value::as_i64)
            .map(|x| x as i32),
        imdb_id: s("imdb_id"),
        tmdb_id: s("tmdb_id"),
        tvdb_id: s("tvdb_id"),
        duration_seconds: v
            .get("duration_seconds")
            .and_then(serde_json::Value::as_f64),
        id: v.get("id").and_then(serde_json::Value::as_i64),
        stream_count: v.get("stream_count").and_then(serde_json::Value::as_i64),
        count: v.get("count").and_then(serde_json::Value::as_i64),
        new_items: v.get("new_items").and_then(serde_json::Value::as_i64),
        error: s("error"),
    })
}

#[derive(Default)]
pub struct NotificationsSubscription;

#[Subscription]
impl NotificationsSubscription {
    /// Stream of all UI-notable Riven events. Replaces the `/notifications/stream` SSE endpoint.
    async fn notifications(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<impl Stream<Item = async_graphql::Result<RivenNotification>>> {
        let queue = ctx.data::<Arc<riven_queue::JobQueue>>()?;
        let rx = queue.notification_tx.subscribe();
        Ok(broadcast_stream(rx).filter_map(|json| async move {
            let v: serde_json::Value = serde_json::from_str(&json).ok()?;
            notification_from_json(&v).map(Ok)
        }))
    }

    /// Stream of live log lines. Replaces the `/logs/stream` SSE endpoint.
    /// Each item is a JSON string matching `{ timestamp, level, message, target }`.
    async fn log_lines(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<impl Stream<Item = async_graphql::Result<String>>> {
        crate::schema::auth::require_settings_access(ctx)?;
        let log_tx = ctx.data::<broadcast::Sender<String>>()?;
        let rx = log_tx.subscribe();
        Ok(broadcast_stream(rx).map(Ok))
    }
}

#[cfg(test)]
mod tests {
    use super::notification_from_json;

    #[test]
    fn an_episode_notification_carries_its_show() {
        let n = notification_from_json(&serde_json::json!({
            "type": "riven.media-item.download.error",
            "id": 7,
            "title": "The Garden",
            "item_type": "Episode",
            "show_title": "Bob's Burgers",
            "season_number": 3,
            "episode_number": 14,
        }))
        .expect("a typed event is a notification");
        assert_eq!(n.title.as_deref(), Some("The Garden"));
        assert_eq!(n.show_title.as_deref(), Some("Bob's Burgers"));
        assert_eq!((n.season_number, n.episode_number), (Some(3), Some(14)));
    }

    #[test]
    fn a_movie_notification_has_no_show() {
        let n = notification_from_json(&serde_json::json!({
            "type": "riven.media-item.download.error",
            "title": "Nobody 2",
            "item_type": "Movie",
        }))
        .expect("a typed event is a notification");
        assert_eq!(n.show_title, None);
        assert_eq!((n.season_number, n.episode_number), (None, None));
    }
}
