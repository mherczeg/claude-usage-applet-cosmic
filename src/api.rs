use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const BETA_HEADER: &str = "oauth-2025-04-20";

// ── Credentials file (~/.claude/.credentials.json) ──────────────────────

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    pub claude_ai_oauth: OAuthData,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct OAuthData {
    #[serde(rename = "accessToken")]
    pub access_token: String,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: u64,
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
}

// ── Token refresh response ──────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
    scope: Option<String>,
}

// ── Usage API response ──────────────────────────────────────────────────

#[derive(Deserialize, Clone, Debug, Default)]
pub struct UsageData {
    pub five_hour: Option<UsageLimit>,
    pub seven_day: Option<UsageLimit>,
    pub seven_day_sonnet: Option<UsageLimit>,
    pub seven_day_opus: Option<UsageLimit>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct UsageLimit {
    pub utilization: f64,
    pub resets_at: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn credentials_path() -> PathBuf {
    dirs::home_dir()
        .expect("no home directory")
        .join(".claude")
        .join(".credentials.json")
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn load_credentials() -> Result<Credentials, String> {
    let path = credentials_path();
    let data = std::fs::read_to_string(&path)
        .map_err(|e| format!("read credentials: {e}"))?;
    serde_json::from_str(&data).map_err(|e| format!("parse credentials: {e}"))
}

fn save_credentials(creds: &Credentials) -> Result<(), String> {
    let path = credentials_path();
    let data =
        serde_json::to_string_pretty(creds).map_err(|e| format!("serialize credentials: {e}"))?;
    std::fs::write(&path, data).map_err(|e| format!("write credentials: {e}"))
}

async fn refresh_token(creds: &mut Credentials) -> Result<(), String> {
    let oauth = &creds.claude_ai_oauth;
    let scopes = oauth
        .scopes
        .as_deref()
        .unwrap_or_default()
        .join(" ");

    let body = serde_json::json!({
        "grant_type": "refresh_token",
        "refresh_token": oauth.refresh_token,
        "client_id": CLIENT_ID,
        "scope": scopes,
    });

    let client = reqwest::Client::new();
    let resp: TokenResponse = client
        .post(TOKEN_URL)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("token request: {e}"))?
        .json()
        .await
        .map_err(|e| format!("token response: {e}"))?;

    let oauth = &mut creds.claude_ai_oauth;
    oauth.access_token = resp.access_token;
    if let Some(rt) = resp.refresh_token {
        oauth.refresh_token = rt;
    }
    oauth.expires_at = now_millis() + resp.expires_in * 1000;
    if let Some(scope) = resp.scope {
        oauth.scopes = Some(scope.split_whitespace().map(String::from).collect());
    }

    save_credentials(creds)?;
    Ok(())
}

async fn get_access_token() -> Result<String, String> {
    let mut creds = load_credentials()?;
    if now_millis() >= creds.claude_ai_oauth.expires_at {
        refresh_token(&mut creds).await?;
    }
    Ok(creds.claude_ai_oauth.access_token)
}

pub async fn fetch_usage() -> Result<UsageData, String> {
    let token = get_access_token().await?;

    let client = reqwest::Client::new();
    let resp = client
        .get(USAGE_URL)
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .header("User-Agent", "claude-usage-applet/1.0")
        .header("anthropic-beta", BETA_HEADER)
        .send()
        .await
        .map_err(|e| format!("usage request: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("usage API returned {}", resp.status()));
    }

    let text = resp.text().await.map_err(|e| format!("usage response: {e}"))?;
    if text.is_empty() || text.trim() == "null" {
        return Ok(UsageData::default());
    }
    serde_json::from_str(&text).map_err(|e| format!("usage parse: {e}"))
}

pub fn format_reset_time(iso_str: &str) -> String {
    let reset = chrono::DateTime::parse_from_rfc3339(iso_str)
        .or_else(|_| chrono::DateTime::parse_from_str(iso_str, "%Y-%m-%dT%H:%M:%S%.f%:z"))
        .unwrap_or_else(|_| chrono::Utc::now().fixed_offset());

    let now = chrono::Utc::now();
    let total_sec = (reset.signed_duration_since(now)).num_seconds();

    if total_sec <= 0 {
        return "now".to_string();
    }
    let hours = total_sec / 3600;
    let minutes = (total_sec % 3600) / 60;
    if hours > 24 {
        format!("{}d {}h", hours / 24, hours % 24)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    }
}
