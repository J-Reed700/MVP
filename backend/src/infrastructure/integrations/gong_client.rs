use anyhow::{bail, Context, Result};
use reqwest::{
    header::{HeaderName, HeaderValue},
    Client, RequestBuilder, StatusCode,
};
use serde_json::Value;

use crate::infrastructure::integrations::gong::{extract_events, GongEvent};

#[derive(Clone)]
pub struct GongClient {
    events_url: String,
    api_key: Option<String>,
    basic_auth_user: Option<String>,
    basic_auth_pass: Option<String>,
    auth_header_name: Option<HeaderName>,
    auth_header_value: Option<HeaderValue>,
    http: Client,
}

impl GongClient {
    pub fn from_env() -> Result<Option<Self>> {
        let events_url = std::env::var("GONG_EVENTS_URL")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                std::env::var("PENDO_EVENTS_URL")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            })
            .or_else(|| {
                std::env::var("GONG_BASE_URL")
                    .ok()
                    .map(|value| value.trim().trim_end_matches('/').to_string())
                    .filter(|value| !value.is_empty())
                    .map(|base| format!("{base}/events"))
            })
            .or_else(|| {
                std::env::var("PENDO_BASE_URL")
                    .ok()
                    .map(|value| value.trim().trim_end_matches('/').to_string())
                    .filter(|value| !value.is_empty())
                    .map(|base| format!("{base}/events"))
            });

        let Some(events_url) = events_url else {
            return Ok(None);
        };

        let api_key = std::env::var("GONG_API_KEY")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                std::env::var("PENDO_API_KEY")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            });

        let basic_auth_user = std::env::var("GONG_BASIC_AUTH_USER")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                std::env::var("PENDO_BASIC_AUTH_USER")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            });
        let basic_auth_pass = std::env::var("GONG_BASIC_AUTH_PASS")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                std::env::var("PENDO_BASIC_AUTH_PASS")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            });

        if basic_auth_user.is_some() ^ basic_auth_pass.is_some() {
            bail!("both GONG_BASIC_AUTH_USER and GONG_BASIC_AUTH_PASS must be set together");
        }

        let auth_header_name = std::env::var("GONG_AUTH_HEADER_NAME")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                std::env::var("PENDO_AUTH_HEADER_NAME")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            });
        let auth_header_value = std::env::var("GONG_AUTH_HEADER_VALUE")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .or_else(|| {
                std::env::var("PENDO_AUTH_HEADER_VALUE")
                    .ok()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            });

        let (auth_header_name, auth_header_value) = match (auth_header_name, auth_header_value) {
            (None, None) => (None, None),
            (Some(name), Some(value)) => {
                let parsed_name = name
                    .parse::<HeaderName>()
                    .with_context(|| format!("invalid GONG_AUTH_HEADER_NAME '{name}'"))?;
                let parsed_value = HeaderValue::from_str(&value)
                    .with_context(|| "invalid GONG_AUTH_HEADER_VALUE".to_string())?;
                (Some(parsed_name), Some(parsed_value))
            }
            _ => {
                bail!("both GONG_AUTH_HEADER_NAME and GONG_AUTH_HEADER_VALUE must be set together")
            }
        };

        let http = Client::builder()
            .build()
            .context("failed to build gong http client")?;

        Ok(Some(Self {
            events_url,
            api_key,
            basic_auth_user,
            basic_auth_pass,
            auth_header_name,
            auth_header_value,
            http,
        }))
    }

    pub async fn fetch_events(&self, limit: usize) -> Result<Vec<GongEvent>> {
        let response = self
            .apply_auth(self.http.get(&self.events_url))
            .send()
            .await
            .context("gong sync request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if status == StatusCode::UNAUTHORIZED {
                bail!("gong authentication failed (401): check GONG_API_KEY / auth header / basic auth");
            }
            bail!(
                "gong sync request failed ({status}): {}",
                summarize_error_body(&body)
            );
        }

        let payload: Value = response
            .json()
            .await
            .context("failed to parse gong sync payload as json")?;
        let mut events = extract_events(&payload);
        if limit > 0 && events.len() > limit {
            events.truncate(limit);
        }
        Ok(events)
    }

    fn apply_auth(&self, mut request: RequestBuilder) -> RequestBuilder {
        if let Some(api_key) = &self.api_key {
            request = request.bearer_auth(api_key);
        }
        if let (Some(user), Some(pass)) = (&self.basic_auth_user, &self.basic_auth_pass) {
            request = request.basic_auth(user, Some(pass));
        }
        if let (Some(name), Some(value)) = (&self.auth_header_name, &self.auth_header_value) {
            request = request.header(name, value.clone());
        }
        request
    }
}

fn summarize_error_body(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() > 300 {
        format!("{}…", &compact[..300])
    } else {
        compact
    }
}
