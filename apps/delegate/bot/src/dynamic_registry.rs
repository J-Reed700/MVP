use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::context::{self, SkillHandler, SkillTool};
use crate::oauth::CredentialStore;
use crate::registry::{self, ActionTier, ToolScope};

/// Dynamic registry that wraps static tool definitions with runtime skill-defined tools.
pub struct DynamicRegistry {
    skill_tools: RwLock<Vec<SkillTool>>,
    /// Lookup by tool name for fast dispatch.
    tool_index: RwLock<HashMap<String, usize>>,
    credential_store: RwLock<Option<Arc<CredentialStore>>>,
}

impl DynamicRegistry {
    pub fn new() -> Self {
        Self {
            skill_tools: RwLock::new(Vec::new()),
            tool_index: RwLock::new(HashMap::new()),
            credential_store: RwLock::new(None),
        }
    }

    /// Set the credential store for OAuth-based env var resolution.
    pub async fn set_credential_store(&self, store: Arc<CredentialStore>) {
        *self.credential_store.write().await = Some(store);
    }

    /// Refresh skill-defined tools from the skills directory (no filtering).
    pub async fn refresh(&self, skills_dir: &Path) {
        let skills = context::load_skills(skills_dir, None).await;
        self.load_tools_from_skills(&skills).await;
    }

    /// Refresh skill-defined tools, filtering by connected/configured providers.
    pub async fn refresh_with_filter(&self, skills_dir: &Path, connected: Option<&HashSet<String>>) {
        let configured = {
            let store = self.credential_store.read().await;
            store.as_ref().map(|s| s.configured_providers())
        };
        let skills = context::load_skills_filtered(
            skills_dir,
            connected,
            configured.as_ref(),
        ).await;
        self.load_tools_from_skills(&skills).await;
    }

    async fn load_tools_from_skills(&self, skills: &[context::Skill]) {
        let mut all_tools = Vec::new();
        for skill in skills {
            all_tools.extend(skill.tools.clone());
        }

        let mut index = HashMap::new();
        for (i, tool) in all_tools.iter().enumerate() {
            index.insert(tool.name.clone(), i);
        }

        *self.skill_tools.write().await = all_tools;
        *self.tool_index.write().await = index;
    }

    /// Get tool schemas for a given scope, merging static + skill-defined tools.
    pub async fn tool_schemas(&self, scope: ToolScope) -> Vec<Value> {
        let mut schemas = match scope {
            ToolScope::Event => registry::event_tool_schemas(),
            ToolScope::Heartbeat => registry::heartbeat_tool_schemas(),
            ToolScope::Both => registry::event_tool_schemas(),
        };

        // Skill-defined tools are Event-scope only
        if matches!(scope, ToolScope::Event | ToolScope::Both) {
            let tools = self.skill_tools.read().await;
            for tool in tools.iter() {
                schemas.push(tool.schema.clone());
            }
        }

        schemas
    }

    /// Classify an action tier. Skill-defined tools always require approval.
    pub async fn classify_action(&self, tool_name: &str) -> ActionTier {
        let index = self.tool_index.read().await;
        if index.contains_key(tool_name) {
            ActionTier::RequiresApproval
        } else {
            registry::classify_action(tool_name)
        }
    }

    /// Check if a tool is an information tool. Skill-defined tools always are.
    pub async fn is_information_tool(&self, tool_name: &str) -> bool {
        let index = self.tool_index.read().await;
        if index.contains_key(tool_name) {
            true
        } else {
            registry::is_information_tool(tool_name)
        }
    }

    /// Get a reference to the credential store (if set).
    pub async fn get_credential_store(&self) -> Option<Arc<CredentialStore>> {
        self.credential_store.read().await.clone()
    }

    /// Look up a skill-defined tool by name for dispatch.
    pub async fn get_skill_tool(&self, name: &str) -> Option<SkillTool> {
        let index = self.tool_index.read().await;
        let i = index.get(name)?;
        let tools = self.skill_tools.read().await;
        tools.get(*i).cloned()
    }

    /// Check if a tool name belongs to a skill-defined tool.
    #[allow(dead_code)]
    pub async fn is_skill_tool(&self, name: &str) -> bool {
        self.tool_index.read().await.contains_key(name)
    }
}

/// Execute a skill-defined tool based on its handler type.
pub async fn execute_skill_tool(
    skill_tool: &SkillTool,
    args: &Value,
    workspace: &Path,
    cred_store: Option<&CredentialStore>,
) -> String {
    match &skill_tool.handler {
        SkillHandler::Script { language, script_path } => {
            execute_script_handler(language, script_path, args, workspace).await
        }
        SkillHandler::Http { method, url_template, headers, body_template } => {
            execute_http_handler(method, url_template, headers, body_template.as_deref(), args, cred_store).await
        }
    }
}

async fn execute_script_handler(
    language: &str,
    script_path: &Path,
    args: &Value,
    workspace: &Path,
) -> String {
    let code = match tokio::fs::read_to_string(script_path).await {
        Ok(c) => c,
        Err(e) => return format!("Failed to read handler script: {e}"),
    };

    let (program, flag) = match language.as_ref() {
        "python" => ("python3", None),
        "shell" | _ => {
            if cfg!(windows) {
                ("cmd", Some("/C"))
            } else {
                ("sh", Some("-c"))
            }
        }
    };

    // Write to a temp script file
    let scripts_dir = workspace.join(".scripts");
    let _ = tokio::fs::create_dir_all(&scripts_dir).await;

    let ext = if language == "python" { "py" } else if cfg!(windows) { "bat" } else { "sh" };
    let tmp_path = scripts_dir.join(format!("_skill_run.{ext}"));
    if let Err(e) = tokio::fs::write(&tmp_path, &code).await {
        return format!("Failed to write temp script: {e}");
    }

    let mut cmd = tokio::process::Command::new(program);
    if let Some(f) = flag {
        cmd.arg(f);
    }
    cmd.arg(&tmp_path);
    cmd.current_dir(workspace);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // Pass args via TOOL_ARGS env var and stdin
    let args_json = serde_json::to_string(args).unwrap_or_default();
    cmd.env("TOOL_ARGS", &args_json);

    // For stdin piping we need to spawn manually
    cmd.stdin(std::process::Stdio::piped());

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        async {
            let mut child = cmd.spawn()?;
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(args_json.as_bytes()).await;
                drop(stdin);
            }
            child.wait_with_output().await
        },
    )
    .await;

    let _ = tokio::fs::remove_file(&tmp_path).await;

    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let status = output.status;
            let mut result = format!("Exit code: {status}\n");
            if !stdout.is_empty() {
                let truncated = crate::tools::truncate_str(&stdout, 8192);
                result.push_str(&format!("\nstdout:\n{truncated}"));
            }
            if !stderr.is_empty() {
                let truncated = crate::tools::truncate_str(&stderr, 2048);
                result.push_str(&format!("\nstderr:\n{truncated}"));
            }
            result
        }
        Ok(Err(e)) => format!("Failed to execute handler: {e}"),
        Err(_) => "Handler timed out after 30 seconds".to_string(),
    }
}

async fn execute_http_handler(
    method: &str,
    url_template: &str,
    headers: &HashMap<String, String>,
    body_template: Option<&str>,
    args: &Value,
    cred_store: Option<&CredentialStore>,
) -> String {
    // Substitute {{arg}} templates
    let url = substitute_template(url_template, args, cred_store).await;

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(c) => c,
        Err(e) => return format!("Failed to create HTTP client: {e}"),
    };

    let mut builder = match method.to_uppercase().as_str() {
        "GET" => client.get(&url),
        "POST" => client.post(&url),
        "PUT" => client.put(&url),
        "PATCH" => client.patch(&url),
        "DELETE" => client.delete(&url),
        other => return format!("Unsupported method: {other}"),
    };

    for (key, val) in headers {
        builder = builder.header(key.as_str(), substitute_template(val, args, cred_store).await);
    }

    if let Some(bt) = body_template {
        builder = builder.body(substitute_template(bt, args, cred_store).await);
    }

    match builder.send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            let truncated = crate::tools::truncate_str(&body, 8192);
            if truncated.len() < body.len() {
                format!("HTTP {status}\n\n{truncated}\n\n[truncated, {} bytes total]", body.len())
            } else {
                format!("HTTP {status}\n\n{body}")
            }
        }
        Err(e) => format!("HTTP request failed: {e}"),
    }
}

/// Replace `{{key}}` placeholders with values from args,
/// then replace `{{env.VAR}}` placeholders with credential store or environment variables.
async fn substitute_template(
    template: &str,
    args: &Value,
    cred_store: Option<&CredentialStore>,
) -> String {
    let mut result = template.to_string();
    if let Some(obj) = args.as_object() {
        for (key, val) in obj {
            let placeholder = format!("{{{{{key}}}}}");
            let replacement = match val {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            result = result.replace(&placeholder, &replacement);
        }
    }
    // Clean up any unresolved arg placeholders (optional params the LLM didn't provide)
    while let Some(start) = result.find("{{") {
        // Don't touch {{env.}} placeholders — those are handled below
        if result[start..].starts_with("{{env.") {
            break;
        }
        if let Some(end) = result[start..].find("}}") {
            result = format!("{}{}", &result[..start], &result[start + end + 2..]);
        } else {
            break;
        }
    }

    // Replace {{env.VAR_NAME}} — try credential store first, fall back to env var
    while let Some(start) = result.find("{{env.") {
        if let Some(end) = result[start..].find("}}") {
            let var_name = &result[start + 6..start + end].to_string();
            let replacement = if let Some(store) = cred_store {
                if let Some(val) = store.resolve_env_var(var_name).await {
                    val
                } else {
                    std::env::var(var_name).unwrap_or_default()
                }
            } else {
                std::env::var(var_name).unwrap_or_default()
            };
            result = format!("{}{}{}", &result[..start], replacement, &result[start + end + 2..]);
        } else {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn substitute_template_replaces_args() {
        let args = serde_json::json!({"repo": "owner/repo", "count": 5});
        let result = substitute_template("https://api.github.com/repos/{{repo}}/pulls?per_page={{count}}", &args, None).await;
        assert_eq!(result, "https://api.github.com/repos/owner/repo/pulls?per_page=5");
    }

    #[tokio::test]
    async fn substitute_template_no_args() {
        let args = serde_json::json!({});
        let result = substitute_template("https://example.com", &args, None).await;
        assert_eq!(result, "https://example.com");
    }

    #[tokio::test]
    async fn substitute_template_env_vars() {
        std::env::set_var("TEST_SKILL_TOKEN", "secret123");
        let args = serde_json::json!({"project": "DEV"});
        let result = substitute_template(
            "https://api.example.com/{{project}}?token={{env.TEST_SKILL_TOKEN}}",
            &args,
            None,
        ).await;
        assert_eq!(result, "https://api.example.com/DEV?token=secret123");
        std::env::remove_var("TEST_SKILL_TOKEN");
    }

    #[tokio::test]
    async fn dynamic_registry_empty() {
        let reg = DynamicRegistry::new();
        assert!(!reg.is_skill_tool("anything").await);
        assert_eq!(reg.classify_action("react").await, ActionTier::Autonomous);
    }

    // ── Dry-run template tests for integration skills ──

    #[tokio::test]
    async fn jira_search_template_substitution() {
        std::env::set_var("JIRA_BASE_URL", "https://test.atlassian.net");
        std::env::set_var("JIRA_AUTH", "dXNlcjp0b2tlbg==");

        let args = serde_json::json!({"jql": "project = DEV", "max_results": 5});

        let url = substitute_template(
            "{{env.JIRA_BASE_URL}}/rest/api/3/search?jql={{jql}}&maxResults={{max_results}}&fields=summary,status,assignee,priority,updated",
            &args,
            None,
        ).await;
        assert_eq!(url, "https://test.atlassian.net/rest/api/3/search?jql=project = DEV&maxResults=5&fields=summary,status,assignee,priority,updated");

        let auth = substitute_template("Basic {{env.JIRA_AUTH}}", &args, None).await;
        assert_eq!(auth, "Basic dXNlcjp0b2tlbg==");

        std::env::remove_var("JIRA_BASE_URL");
        std::env::remove_var("JIRA_AUTH");
    }

    #[tokio::test]
    async fn jira_create_body_substitution() {
        let args = serde_json::json!({
            "project_key": "DEV",
            "summary": "New feature request",
            "description": "Build the thing",
            "issue_type": "Task"
        });

        let body = substitute_template(
            r#"{"fields":{"project":{"key":"{{project_key}}"},"summary":"{{summary}}","description":{"type":"doc","version":1,"content":[{"type":"paragraph","content":[{"type":"text","text":"{{description}}"}]}]},"issuetype":{"name":"{{issue_type}}"}}}"#,
            &args,
            None,
        ).await;
        assert!(body.contains(r#""key":"DEV""#));
        assert!(body.contains(r#""summary":"New feature request""#));
        assert!(body.contains(r#""text":"Build the thing""#));
        assert!(body.contains(r#""name":"Task""#));
    }

    #[tokio::test]
    async fn linear_graphql_template_substitution() {
        std::env::set_var("LINEAR_API_KEY", "lin_api_test123");

        let args = serde_json::json!({"query": "auth bug", "first": 5});
        let auth = substitute_template("{{env.LINEAR_API_KEY}}", &args, None).await;
        assert_eq!(auth, "lin_api_test123");

        let body = substitute_template(
            r#"{"query":"query { issueSearch(query: \"{{query}}\", first: {{first}}) { nodes { id identifier title } } }"}"#,
            &args,
            None,
        ).await;
        assert!(body.contains(r#"query: \"auth bug\""#));
        assert!(body.contains("first: 5"));

        std::env::remove_var("LINEAR_API_KEY");
    }

    #[tokio::test]
    async fn notion_search_template_substitution() {
        std::env::set_var("NOTION_API_KEY", "ntn_test_token");

        let args = serde_json::json!({"query": "deployment process"});
        let auth = substitute_template("Bearer {{env.NOTION_API_KEY}}", &args, None).await;
        assert_eq!(auth, "Bearer ntn_test_token");

        let body = substitute_template(r#"{"query":"{{query}}","page_size":10}"#, &args, None).await;
        assert!(body.contains(r#""query":"deployment process""#));

        std::env::remove_var("NOTION_API_KEY");
    }

    #[tokio::test]
    async fn gmail_send_template_substitution() {
        std::env::set_var("GOOGLE_ACCESS_TOKEN", "ya29.test_token");

        let args = serde_json::json!({"raw_message": "SGVsbG8gV29ybGQ="});
        let auth = substitute_template("Bearer {{env.GOOGLE_ACCESS_TOKEN}}", &args, None).await;
        assert_eq!(auth, "Bearer ya29.test_token");

        let body = substitute_template(r#"{"raw":"{{raw_message}}"}"#, &args, None).await;
        assert_eq!(body, r#"{"raw":"SGVsbG8gV29ybGQ="}"#);

        std::env::remove_var("GOOGLE_ACCESS_TOKEN");
    }

    #[tokio::test]
    async fn gcal_list_events_template_substitution() {
        std::env::set_var("GOOGLE_ACCESS_TOKEN", "ya29.test_token");

        let args = serde_json::json!({
            "calendar_id": "primary",
            "time_min": "2026-03-06T00:00:00Z",
            "time_max": "2026-03-07T23:59:59Z",
            "max_results": 10
        });

        let url = substitute_template(
            "https://www.googleapis.com/calendar/v3/calendars/{{calendar_id}}/events?timeMin={{time_min}}&timeMax={{time_max}}&maxResults={{max_results}}&singleEvents=true&orderBy=startTime",
            &args,
            None,
        ).await;
        assert!(url.contains("calendars/primary/events"));
        assert!(url.contains("timeMin=2026-03-06T00:00:00Z"));
        assert!(url.contains("maxResults=10"));

        std::env::remove_var("GOOGLE_ACCESS_TOKEN");
    }

    #[tokio::test]
    async fn confluence_create_body_substitution() {
        let args = serde_json::json!({
            "space_key": "DEV",
            "title": "Architecture Decision Records",
            "body_html": "<h1>ADR-001</h1><p>We chose Rust.</p>"
        });

        let body = substitute_template(
            r#"{"type":"page","title":"{{title}}","space":{"key":"{{space_key}}"},"body":{"storage":{"value":"{{body_html}}","representation":"storage"}}}"#,
            &args,
            None,
        ).await;
        assert!(body.contains(r#""title":"Architecture Decision Records""#));
        assert!(body.contains(r#""key":"DEV""#));
        assert!(body.contains("<h1>ADR-001</h1>"));
    }

    #[tokio::test]
    async fn missing_env_var_defaults_to_empty() {
        std::env::remove_var("NONEXISTENT_VAR_12345");
        let args = serde_json::json!({});
        let result = substitute_template("token={{env.NONEXISTENT_VAR_12345}}", &args, None).await;
        assert_eq!(result, "token=");
    }

    #[tokio::test]
    async fn substitute_template_prefers_credential_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::new(dir.path(), std::collections::HashMap::new());

        // Store a Linear credential
        let cred = crate::oauth::OAuthCredential {
            provider: "linear".to_string(),
            access_token: "oauth_lin_token".to_string(),
            refresh_token: None,
            expires_at: None,
            scopes: Vec::new(),
            extra: std::collections::HashMap::new(),
            connected_at: "2026-03-06T10:00:00Z".to_string(),
            connected_by: "U12345".to_string(),
        };
        store.store(&cred).await.unwrap();

        // Also set env var with different value
        std::env::set_var("LINEAR_API_KEY", "env_lin_token");

        let args = serde_json::json!({});
        let result = substitute_template("{{env.LINEAR_API_KEY}}", &args, Some(&store)).await;
        // Should prefer credential store over env var
        assert_eq!(result, "oauth_lin_token");

        std::env::remove_var("LINEAR_API_KEY");
    }

    #[tokio::test]
    async fn registry_loads_integration_skills() {
        let skills_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("workspace/skills");
        let reg = DynamicRegistry::new();
        reg.refresh(&skills_dir).await;

        // All integration tools should be found as skill tools
        let integration_tools = [
            "jira_search", "jira_get_issue", "jira_create_issue", "jira_assign_issue",
            "jira_get_sprint", "jira_get_boards", "jira_link_issues",
            "linear_search_issues", "linear_get_issue", "linear_create_issue",
            "linear_get_cycle", "linear_list_members", "linear_list_states",
            "notion_search", "notion_create_page", "notion_create_database_entry",
            "notion_update_page_properties",
            "confluence_search", "confluence_create_page", "confluence_add_comment",
            "confluence_get_children",
            "gcal_list_events", "gcal_get_event", "gcal_freebusy",
            "gmail_list_messages", "gmail_send_message", "gmail_get_thread", "gmail_reply",
        ];

        for tool_name in &integration_tools {
            assert!(reg.is_skill_tool(tool_name).await, "Tool '{}' not found in registry", tool_name);
            assert_eq!(
                reg.classify_action(tool_name).await,
                ActionTier::RequiresApproval,
                "Tool '{}' should require approval", tool_name
            );
        }
    }

    #[tokio::test]
    async fn substitute_template_cleans_missing_optional_args() {
        // Simulates jira_create_issue where description and issue_type are optional
        let args = serde_json::json!({"project_key": "DEV", "summary": "My issue"});
        let body = substitute_template(
            r#"{"fields":{"project":{"key":"{{project_key}}"},"summary":"{{summary}}","issuetype":{"name":"{{issue_type}}"}}}"#,
            &args,
            None,
        ).await;
        // Unresolved {{issue_type}} should be removed, not left as literal
        assert!(!body.contains("{{issue_type}}"));
        assert!(body.contains(r#""key":"DEV""#));
        assert!(body.contains(r#""summary":"My issue""#));
    }
}
