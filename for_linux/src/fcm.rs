//! Push Notification Service — Sends push notifications to offline peers.
//!
//! Supports two backends:
//! - **FCM v1 API** (Android): Firebase Cloud Messaging via service account JWT.
//! - **APNs** (iOS): Apple Push Notification service via P8 key JWT.
//!
//! Configuration (environment variables):
//!   FIREBASE_SERVICE_ACCOUNT_PATH  — path to Firebase service account JSON
//!   APNS_KEY_PATH                  — path to Apple P8 key file
//!   APNS_KEY_ID                    — Apple Push Key ID
//!   APNS_TEAM_ID                   — Apple Developer Team ID
//!   APNS_BUNDLE_ID                 — iOS app bundle ID (default: chat.introvert.app)
//!   APNS_USE_PRODUCTION            — "true" for production, "false" for sandbox (default: true)

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

// ── FCM Types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct ServiceAccount {
    #[serde(rename = "type")]
    account_type: String,
    project_id: String,
    private_key_id: String,
    private_key: String,
    client_email: String,
    client_id: String,
    auth_uri: String,
    token_uri: String,
}

#[derive(Debug, Serialize)]
struct FcmJwtClaims {
    iss: String,
    scope: String,
    aud: String,
    exp: usize,
    iat: usize,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    token_type: String,
}

#[derive(Debug, Serialize)]
struct FcmMessage {
    message: FcmPayload,
    #[serde(skip_serializing_if = "Option::is_none")]
    validate_only: Option<bool>,
}

#[derive(Debug, Serialize)]
struct FcmPayload {
    token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    notification: Option<FcmNotification>,
    data: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct FcmNotification {
    title: String,
    body: String,
}

// ── APNs Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ApnsJwtHeader {
    alg: String,
    kid: String,
}

#[derive(Debug, Serialize)]
struct ApnsJwtClaims {
    iss: String,
    iat: usize,
}

#[derive(Debug, Serialize)]
struct ApnsPayload {
    aps: ApnsAps,
    #[serde(flatten)]
    data: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ApnsAps {
    alert: ApnsAlert,
    sound: String,
    #[serde(rename = "content-available")]
    content_available: u8,
}

#[derive(Debug, Serialize)]
struct ApnsAlert {
    title: String,
    body: String,
}

// ── Cached Token ─────────────────────────────────────────────────────────────

struct CachedToken {
    access_token: String,
    expires_at: std::time::Instant,
}

// ── APNs Config ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ApnsConfig {
    key_path: String,
    key_id: String,
    team_id: String,
    bundle_id: String,
    use_production: bool,
}

// ── Push Service ─────────────────────────────────────────────────────────────

pub struct FcmPushService {
    service_account: Option<ServiceAccount>,
    cached_fcm_token: Mutex<Option<CachedToken>>,
    apns_config: Option<ApnsConfig>,
    cached_apns_token: Mutex<Option<CachedToken>>,
}

const MAX_RETRIES: u32 = 3;
const INITIAL_RETRY_DELAY_MS: u64 = 500;

impl FcmPushService {
    pub fn new() -> Self {
        let sa = Self::load_service_account();
        match &sa {
            Some(_) => println!("[Push] ✅ Firebase service account loaded"),
            None => println!("[Push] ⚠️ No Firebase service account — Android push disabled"),
        }

        let apns = Self::load_apns_config();
        match &apns {
            Some(_) => println!("[Push] ✅ APNs configuration loaded"),
            None => println!("[Push] ⚠️ No APNs config — iOS push disabled"),
        }

        Self {
            service_account: sa,
            cached_fcm_token: Mutex::new(None),
            apns_config: apns,
            cached_apns_token: Mutex::new(None),
        }
    }

    fn load_service_account() -> Option<ServiceAccount> {
        let path = std::env::var("FIREBASE_SERVICE_ACCOUNT_PATH")
            .unwrap_or_else(|_| "/opt/introvert/config/firebase-service-account.json".to_string());

        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<ServiceAccount>(&content) {
                Ok(sa) => Some(sa),
                Err(e) => {
                    eprintln!("[Push] ❌ Failed to parse service account JSON: {}", e);
                    None
                }
            },
            Err(e) => {
                println!("[Push] ℹ️ No service account at {}: {}", path, e);
                None
            }
        }
    }

    fn load_apns_config() -> Option<ApnsConfig> {
        let key_path = std::env::var("APNS_KEY_PATH").ok()?;
        let key_id = std::env::var("APNS_KEY_ID").ok()?;
        let team_id = std::env::var("APNS_TEAM_ID").ok()?;
        let bundle_id = std::env::var("APNS_BUNDLE_ID")
            .unwrap_or_else(|_| "chat.introvert.app".to_string());
        let use_production = std::env::var("APNS_USE_PRODUCTION")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        if !std::path::Path::new(&key_path).exists() {
            eprintln!("[Push] ❌ APNs key file not found at: {}", key_path);
            return None;
        }

        Some(ApnsConfig {
            key_path,
            key_id,
            team_id,
            bundle_id,
            use_production,
        })
    }

    // ── FCM Access Token (async) ─────────────────────────────────────────────

    async fn get_fcm_access_token(&self) -> Option<String> {
        {
            let cached = self.cached_fcm_token.lock();
            if let Some(ref token) = *cached {
                if token.expires_at > std::time::Instant::now() {
                    return Some(token.access_token.clone());
                }
            }
        }

        let sa = self.service_account.as_ref()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;

        let claims = FcmJwtClaims {
            iss: sa.client_email.clone(),
            scope: "https://www.googleapis.com/auth/firebase.messaging".to_string(),
            aud: sa.token_uri.clone(),
            exp: now + 3600,
            iat: now,
        };

        let key = jsonwebtoken::EncodingKey::from_rsa_pem(sa.private_key.as_bytes()).ok()?;
        let header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        let jwt_token = jsonwebtoken::encode(&header, &claims, &key).ok()?;

        let client = reqwest::Client::new();
        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", jwt_token.as_str()),
        ];

        let resp = client
            .post(&sa.token_uri)
            .form(&params)
            .send()
            .await
            .ok()?;

        let token_response: TokenResponse = resp.json().await.ok()?;
        let expires_at = std::time::Instant::now()
            + std::time::Duration::from_secs(token_response.expires_in.saturating_sub(300));

        {
            let mut cached = self.cached_fcm_token.lock();
            *cached = Some(CachedToken {
                access_token: token_response.access_token.clone(),
                expires_at,
            });
        }

        Some(token_response.access_token)
    }

    // ── APNs JWT Token ───────────────────────────────────────────────────────

    fn get_apns_token(&self) -> Option<String> {
        {
            let cached = self.cached_apns_token.lock();
            if let Some(ref token) = *cached {
                if token.expires_at > std::time::Instant::now() {
                    return Some(token.access_token.clone());
                }
            }
        }

        let config = self.apns_config.as_ref()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as usize;

        let header = ApnsJwtHeader {
            alg: "ES256".to_string(),
            kid: config.key_id.clone(),
        };

        let claims = ApnsJwtClaims {
            iss: config.team_id.clone(),
            iat: now,
        };

        let key_bytes = std::fs::read(&config.key_path).ok()?;
        let key = jsonwebtoken::EncodingKey::from_ec_pem(&key_bytes).ok()?;

        let mut header = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::ES256);
        header.kid = Some(config.key_id.clone());

        let jwt_token = jsonwebtoken::encode(&header, &claims, &key).ok()?;

        let expires_at = std::time::Instant::now() + std::time::Duration::from_secs(30 * 60);

        {
            let mut cached = self.cached_apns_token.lock();
            *cached = Some(CachedToken {
                access_token: jwt_token.clone(),
                expires_at,
            });
        }

        Some(jwt_token)
    }

    // ── Send Push (dispatches to FCM or APNs) ────────────────────────────────

    pub async fn send_push(&self, device_type: &str, push_token: &str, sender_peer_id: &str) {
        match device_type {
            "android" => self.send_fcm_push(push_token, sender_peer_id).await,
            "ios" => self.send_apns_push(push_token, sender_peer_id).await,
            _ => eprintln!("[Push] ❌ Unknown device type: {}", device_type),
        }
    }

    // ── FCM Push (Android) ───────────────────────────────────────────────────

    async fn send_fcm_push(&self, push_token: &str, sender_peer_id: &str) {
        let Some(access_token) = self.get_fcm_access_token().await else {
            eprintln!("[FCM] ❌ Failed to get access token — skipping push");
            return;
        };

        let sa = match self.service_account.as_ref() {
            Some(sa) => sa,
            None => return,
        };

        let project_id = &sa.project_id;

        let mut data = std::collections::HashMap::new();
        data.insert("sender_peer_id".to_string(), sender_peer_id.to_string());
        data.insert("msg_type".to_string(), "chat".to_string());

        // DATA-ONLY message: no notification field.
        // If notification field is present, Firebase auto-displays it on the fallback
        // channel when app is backgrounded, bypassing our IntrovertFirebaseMessagingService
        // handler and its 3-minute cooldown. Data-only messages always route through
        // onMessageReceived() where we control display and cooldown.
        let message = FcmPayload {
            token: push_token.to_string(),
            notification: None,
            data,
        };

        let fcm_message = FcmMessage {
            message,
            validate_only: None,
        };

        let url = format!(
            "https://fcm.googleapis.com/v1/projects/{}/messages:send",
            project_id
        );

        for attempt in 0..MAX_RETRIES {
            let client = reqwest::Client::new();
            let response = client
                .post(&url)
                .bearer_auth(&access_token)
                .json(&fcm_message)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        println!(
                            "[FCM] ✅ Push sent (attempt {}), token: {}...",
                            attempt + 1,
                            &push_token[..20.min(push_token.len())]
                        );
                        return;
                    }

                    let body = resp.text().await.unwrap_or_default();

                    // Retry on transient errors
                    if status.as_u16() == 429 || status.as_u16() >= 500 {
                        eprintln!(
                            "[FCM] ⚠️ Transient error {} (attempt {}), retrying: {}",
                            status,
                            attempt + 1,
                            body
                        );
                        let delay = INITIAL_RETRY_DELAY_MS * 2u64.pow(attempt);
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        continue;
                    }

                    // Non-retryable error
                    eprintln!("[FCM] ❌ FCM returned {}: {}", status, body);
                    return;
                }
                Err(e) => {
                    eprintln!(
                        "[FCM] ❌ Request failed (attempt {}): {}",
                        attempt + 1,
                        e
                    );
                    if attempt < MAX_RETRIES - 1 {
                        let delay = INITIAL_RETRY_DELAY_MS * 2u64.pow(attempt);
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    }
                }
            }
        }

        eprintln!(
            "[FCM] ❌ All {} attempts failed for token {}...",
            MAX_RETRIES,
            &push_token[..20.min(push_token.len())]
        );
    }

    // ── APNs Push (iOS) ──────────────────────────────────────────────────────

    async fn send_apns_push(&self, push_token: &str, sender_peer_id: &str) {
        let Some(jwt_token) = self.get_apns_token() else {
            eprintln!("[APNs] ❌ Failed to get JWT token — skipping push");
            return;
        };

        let config = match self.apns_config.as_ref() {
            Some(c) => c,
            None => return,
        };

        let host = if config.use_production {
            "https://api.push.apple.com"
        } else {
            "https://api.development.push.apple.com"
        };

        let url = format!("{}/3/device/{}", host, push_token);

        let mut data = std::collections::HashMap::new();
        data.insert("sender_peer_id".to_string(), sender_peer_id.to_string());
        data.insert("msg_type".to_string(), "chat".to_string());

        let payload = ApnsPayload {
            aps: ApnsAps {
                alert: ApnsAlert {
                    title: "New Message".to_string(),
                    body: "You have a new message from Introvert".to_string(),
                },
                sound: "default".to_string(),
                content_available: 1,
            },
            data,
        };

        for attempt in 0..MAX_RETRIES {
            let client = reqwest::Client::new();

            let response = client
                .post(&url)
                .bearer_auth(&jwt_token)
                .header("apns-topic", &config.bundle_id)
                .header("apns-push-type", "alert")
                .header("apns-priority", "10")
                .json(&payload)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        println!(
                            "[APNs] ✅ Push sent (attempt {}), token: {}...",
                            attempt + 1,
                            &push_token[..20.min(push_token.len())]
                        );
                        return;
                    }

                    let body = resp.text().await.unwrap_or_default();

                    // APNs 429 = TooManyRequests, 5xx = server error
                    if status.as_u16() == 429 || status.as_u16() >= 500 {
                        eprintln!(
                            "[APNs] ⚠️ Transient error {} (attempt {}), retrying: {}",
                            status,
                            attempt + 1,
                            body
                        );
                        let delay = INITIAL_RETRY_DELAY_MS * 2u64.pow(attempt);
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        continue;
                    }

                    // Check for BadDeviceToken — remove from storage
                    if body.contains("BadDeviceToken") {
                        eprintln!(
                            "[APNs] ❌ Bad device token — should remove: {}...",
                            &push_token[..20.min(push_token.len())]
                        );
                        return;
                    }

                    eprintln!("[APNs] ❌ APNs returned {}: {}", status, body);
                    return;
                }
                Err(e) => {
                    eprintln!(
                        "[APNs] ❌ Request failed (attempt {}): {}",
                        attempt + 1,
                        e
                    );
                    if attempt < MAX_RETRIES - 1 {
                        let delay = INITIAL_RETRY_DELAY_MS * 2u64.pow(attempt);
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    }
                }
            }
        }

        eprintln!(
            "[APNs] ❌ All {} attempts failed for token {}...",
            MAX_RETRIES,
            &push_token[..20.min(push_token.len())]
        );
    }

    pub fn is_available(&self) -> bool {
        self.service_account.is_some() || self.apns_config.is_some()
    }

    pub fn has_fcm(&self) -> bool {
        self.service_account.is_some()
    }

    pub fn has_apns(&self) -> bool {
        self.apns_config.is_some()
    }
}
