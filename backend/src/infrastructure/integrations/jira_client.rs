use anyhow::{bail, Context, Result};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct JiraClient {
    base_url: String,
    email: String,
    api_token: String,
    http: Client,
}

#[derive(Debug, Clone)]
pub struct JiraIssue {
    pub key: String,
    pub summary: String,
    pub description: Option<String>,
    pub labels: Vec<String>,
    pub updated: Option<String>,
    pub status: Option<String>,
    pub project_key: Option<String>,
}

impl JiraClient {
    pub fn from_env() -> Result<Option<Self>> {
        let base_url = std::env::var("JIRA_BASE_URL").ok();
        let email = std::env::var("JIRA_USER_EMAIL").ok();
        let api_token = std::env::var("JIRA_API_TOKEN").ok();

        if base_url.is_none() && email.is_none() && api_token.is_none() {
            return Ok(None);
        }

        let base_url = base_url
            .context("JIRA_BASE_URL is required when Jira integration is configured")?
            .trim_end_matches('/')
            .to_string();
        let email =
            email.context("JIRA_USER_EMAIL is required when Jira integration is configured")?;
        let api_token =
            api_token.context("JIRA_API_TOKEN is required when Jira integration is configured")?;

        Ok(Some(Self {
            base_url,
            email,
            api_token,
            http: Client::new(),
        }))
    }

    pub async fn search_issues(&self, jql: &str, max_results: usize) -> Result<Vec<JiraIssue>> {
        let clamped = max_results.clamp(1, 1000);

        let primary = self
            .search_with_path("/rest/api/3/search/jql", jql, clamped)
            .await;
        match primary {
            Ok(issues) => Ok(issues),
            Err(primary_err) => {
                tracing::warn!(
                    error = %primary_err,
                    "jira /search/jql request failed, attempting /search fallback"
                );
                self.search_with_path("/rest/api/3/search", jql, clamped)
                    .await
                    .context("jira search fallback failed")
            }
        }
    }

    async fn search_with_path(
        &self,
        path: &str,
        jql: &str,
        max_results: usize,
    ) -> Result<Vec<JiraIssue>> {
        let mut issues = Vec::new();
        let mut start_at = 0u32;

        while issues.len() < max_results {
            let page_size = (max_results - issues.len()).min(100) as u32;
            let mut page = self
                .search_page(path, jql, page_size, start_at)
                .await
                .context("jira page request failed")?;

            if page.is_empty() {
                break;
            }

            let page_count = page.len();
            issues.append(&mut page);
            start_at += page_count as u32;

            if page_count < page_size as usize {
                break;
            }
        }

        Ok(issues)
    }

    async fn search_page(
        &self,
        path: &str,
        jql: &str,
        max_results: u32,
        start_at: u32,
    ) -> Result<Vec<JiraIssue>> {
        let url = format!("{}{}", self.base_url, path);
        let request = JiraSearchRequest {
            jql,
            start_at,
            max_results,
            fields: vec![
                "summary",
                "description",
                "labels",
                "updated",
                "status",
                "project",
            ],
        };

        let response = self
            .http
            .post(url)
            .basic_auth(&self.email, Some(&self.api_token))
            .json(&request)
            .send()
            .await
            .context("jira request failed")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            if status == StatusCode::UNAUTHORIZED {
                bail!("jira authentication failed (401): check JIRA_USER_EMAIL and JIRA_API_TOKEN");
            }
            bail!("jira request failed ({status}): {body}");
        }

        let parsed: JiraSearchResponse = response
            .json()
            .await
            .context("failed to parse jira search response")?;

        Ok(parsed.issues.into_iter().map(JiraIssue::from).collect())
    }
}

#[derive(Serialize)]
struct JiraSearchRequest<'a> {
    jql: &'a str,
    #[serde(rename = "startAt")]
    start_at: u32,
    #[serde(rename = "maxResults")]
    max_results: u32,
    fields: Vec<&'a str>,
}

#[derive(Deserialize)]
struct JiraSearchResponse {
    issues: Vec<JiraIssueResponse>,
}

#[derive(Deserialize)]
struct JiraIssueResponse {
    key: String,
    fields: JiraIssueFields,
}

#[derive(Deserialize)]
struct JiraIssueFields {
    summary: Option<String>,
    description: Option<serde_json::Value>,
    labels: Option<Vec<String>>,
    updated: Option<String>,
    status: Option<JiraStatus>,
    project: Option<JiraProject>,
}

#[derive(Deserialize)]
struct JiraStatus {
    name: Option<String>,
}

#[derive(Deserialize)]
struct JiraProject {
    key: Option<String>,
}

impl From<JiraIssueResponse> for JiraIssue {
    fn from(value: JiraIssueResponse) -> Self {
        Self {
            key: value.key,
            summary: value
                .fields
                .summary
                .unwrap_or_else(|| "(no summary)".to_string()),
            description: value
                .fields
                .description
                .as_ref()
                .map(compact_json_text)
                .filter(|text| !text.trim().is_empty()),
            labels: value.fields.labels.unwrap_or_default(),
            updated: value.fields.updated,
            status: value.fields.status.and_then(|status| status.name),
            project_key: value
                .fields
                .project
                .as_ref()
                .and_then(|project| project.key.clone()),
        }
    }
}

fn compact_json_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}
