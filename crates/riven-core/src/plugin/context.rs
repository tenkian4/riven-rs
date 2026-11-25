use crate::settings::PluginSettings;

pub struct PluginContext {
    pub settings: PluginSettings,
    pub http: crate::http::HttpClient,
    pub redis: redis::aio::ConnectionManager,
    pub vfs_mount_path: String,
}

impl PluginContext {
    pub fn require_setting(&self, key: &str) -> anyhow::Result<&str> {
        self.settings
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("{key} not configured"))
    }
}

pub async fn validate_api_key(
    http: &crate::http::HttpClient,
    settings: &PluginSettings,
    key_name: &str,
    url: &str,
    header: &str,
) -> anyhow::Result<bool> {
    let api_key = match settings.get(key_name) {
        Some(k) => k,
        None => return Ok(false),
    };
    let resp = http
        .send(
            crate::http::HttpServiceProfile::new("plugin_validation"),
            |client| client.get(url).header(header, api_key),
        )
        .await?;
    // Only a rejected key invalidates the setting; any other status (a 5xx, a
    // redirect) says nothing about the key itself.
    Ok(!matches!(
        resp.status(),
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    ))
}
