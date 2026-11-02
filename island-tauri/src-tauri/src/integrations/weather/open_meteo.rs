//! Open-Meteo transport (SPEC §3.5): bounded, timed GETs. All URL shaping and response
//! parsing is the audited `policy` core; this file only moves bytes.

use super::policy::MAX_BODY_BYTES;
use std::time::Duration;

pub const FORECAST_BASE: &str = "https://api.open-meteo.com";
pub const GEOCODE_BASE: &str = "https://geocoding-api.open-meteo.com";
pub const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, PartialEq, Eq)]
pub enum FetchError {
    Timeout,
    Network,
    Status(u16),
    TooLarge,
}

impl FetchError {
    /// User-presentable, secret-free reason for `ServiceStatus.detail`.
    pub fn detail(&self) -> String {
        match self {
            Self::Timeout => "timeout".into(),
            Self::Network => "network".into(),
            Self::Status(code) => format!("http-{code}"),
            Self::TooLarge => "response-too-large".into(),
        }
    }
}

pub fn client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(TIMEOUT)
        .connect_timeout(TIMEOUT)
        // No redirects: an injected or hijacked base must not bounce us elsewhere.
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(concat!("hermes-island/", env!("CARGO_PKG_VERSION")))
        .build()
}

/// GET `url`, reading at most [`MAX_BODY_BYTES`] before giving up.
pub async fn get(client: &reqwest::Client, url: &str) -> Result<Vec<u8>, FetchError> {
    let mut response = client.get(url).send().await.map_err(classify)?;
    let status = response.status();
    if !status.is_success() {
        return Err(FetchError::Status(status.as_u16()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(classify)? {
        if body.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(FetchError::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn classify(error: reqwest::Error) -> FetchError {
    if error.is_timeout() {
        FetchError::Timeout
    } else {
        FetchError::Network
    }
}
