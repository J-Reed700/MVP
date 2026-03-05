use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use tokio::sync::RwLock;

use crate::context::{self, SkillHandler, SkillTool};
use crate::registry::{self, ActionTier, ToolScope};

/// Dynamic registry that wraps static tool definitions with runtime skill-defined tools.
pub struct DynamicRegistry {
    skill_tools: RwLock<Vec<SkillTool>>,
    /// Lookup by tool name for fast dispatch.
    tool_index: RwLock<HashMap<String, usize>>,
}

impl DynamicRegistry {
    pub fn new() -> Self {
        Self {
            skill_tools: RwLock::new(Vec::new()),
            tool_index: RwLock::new(HashMap::new()),
        }
    }

    /// Refresh skill-defined tools from the skills directory.
    pub async fn refresh(&self, skills_dir: &Path) {
        let skills = context::load_skills(skills_dir).await;
        let mut all_tools = Vec::new();
        for skill in &skills {
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
) -> String {
    match &skill_tool.handler {
        SkillHandler::Script { language, script_path } => {
            execute_script_handler(language, script_path, args, workspace).await
        }
        SkillHandler::Http { method, url_template, headers, body_template } => {
            execute_http_handler(method, url_template, headers, body_template.as_deref(), args).await
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
) -> String {
    // Substitute {{arg}} templates
    let url = substitute_template(url_template, args);

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
        builder = builder.header(key.as_str(), substitute_template(val, args));
    }

    if let Some(bt) = body_template {
        builder = builder.body(substitute_template(bt, args));
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

/// Replace `{{key}}` placeholders with values from args.
fn substitute_template(template: &str, args: &Value) -> String {
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
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_template_replaces_args() {
        let args = serde_json::json!({"repo": "owner/repo", "count": 5});
        let result = substitute_template("https://api.github.com/repos/{{repo}}/pulls?per_page={{count}}", &args);
        assert_eq!(result, "https://api.github.com/repos/owner/repo/pulls?per_page=5");
    }

    #[test]
    fn substitute_template_no_args() {
        let args = serde_json::json!({});
        let result = substitute_template("https://example.com", &args);
        assert_eq!(result, "https://example.com");
    }

    #[tokio::test]
    async fn dynamic_registry_empty() {
        let reg = DynamicRegistry::new();
        assert!(!reg.is_skill_tool("anything").await);
        assert_eq!(reg.classify_action("react").await, ActionTier::Autonomous);
    }
}
