//! @docs ARCHITECTURE:Runner
//!
//! ### AI Assist Note
//! **External Tools**: Interfaces with public APIs and non-workspace services.
//! Includes **Discord Notifications**, **URL Fetching**, and **Financial Auditing**.
//! Implements **Privacy Guard** (SEC-04) and **Resource Quotas** for external
//! retrieval. Requires **Oversight** for all outbound traffic to prevent
//! data exfiltration.
//!
//! ### 🔍 Debugging & Observability
//! - **Telemetry Link**: Search `[external_tools]` in tracing logs.
//! - **Failure Path**: Webhook missing/invalid, external URL timeout, oversight
//!   rejection, or database error during financial log retrieval.
//! - **Trace Scope**: `server-rs::agent::runner::external_tools`

use super::{AgentRunner, RunContext};
use crate::error::AppError;
use crate::agent::runner::tools::error::ToolExecutionError;

impl AgentRunner {
    /// Handles `notify_discord`: sends a webhook notification after oversight.
    pub(crate) async fn handle_notify_discord(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        ) -> Result<String, ToolExecutionError> {
        let msg = fc
            .args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        tracing::info!(
            "🔔 [Surface] Agent {} requesting Discord notification...",
            ctx.agent_id
        );
        self.broadcast_agent(ctx, "🔔 Oversight: wants to notify Discord.", "warning");

        let approved = self
            .submit_oversight(
                crate::agent::types::ToolCallAudit {
                    id: uuid::Uuid::new_v4().to_string(),
                    agent_id: ctx.agent_id.clone(),
                    mission_id: Some(ctx.mission_id.clone()),
                    skill: "notify_discord".to_string(),
                    params: fc.args.clone(),
                    department: ctx.department.clone(),
                    description: "Sending an external notification via Discord.".to_string(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
                Some(ctx.mission_id.clone()),
            )
            .await?;

        if approved {
            if let Ok(webhook) = std::env::var("DISCORD_WEBHOOK") {
                let adapter = crate::adapter::discord::DiscordAdapter::new(webhook);
                adapter.notify(&ctx.name, msg).await.map_err(AppError::from)?;
                self.broadcast_agent(ctx, "🔔 Surface: sent Discord alert", "success");
                Ok("(Notified Discord)".to_string())
            } else {
                Ok("(Discord notification failed - no webhook)".to_string())
            }
        } else {
            Ok("(Discord notification REJECTED by Oversight)".to_string())
        }
    }

    /// Handles `fetch_url`: retrieves text content from a public URL.
    pub(crate) async fn handle_fetch_url(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        _usage: &mut Option<crate::agent::types::TokenUsage>,
    ) -> Result<String, ToolExecutionError> {
        let url = fc.args.get("url").and_then(|v| v.as_str()).unwrap_or("");
        tracing::info!("🌐 [Surface] Agent {} fetching URL: {}", ctx.agent_id, url);

        self.broadcast_agent(
            ctx,
            "🔒 Oversight: wants to fetch external URL. Review required.",
            "warning",
        );

        let approved = self
            .submit_oversight(
                crate::agent::types::ToolCallAudit {
                    id: uuid::Uuid::new_v4().to_string(),
                    agent_id: ctx.agent_id.clone(),
                    mission_id: Some(ctx.mission_id.clone()),
                    skill: "fetch_url".to_string(),
                    params: fc.args.clone(),
                    department: ctx.department.clone(),
                    description: format!("External retrieval from: {}", url),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
                Some(ctx.mission_id.clone()),
            )
            .await?;

        if !approved {
            Ok("(Fetch REJECTED by Oversight)".to_string())
        } else {
            self.broadcast_agent(ctx, &format!("🌐 Surface: researching {}...", url), "info");

            let parsed_url = match reqwest::Url::parse(url) {
                Ok(u) => u,
                Err(e) => {
                    let safe_url = url.replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;");
                    return Ok(format!("<untrusted_web_content url=\"{}\">\nFETCH FAILED: Invalid URL: {}\n</untrusted_web_content>", safe_url, e));
                }
            };

            let scheme = parsed_url.scheme();
            if scheme != "http" && scheme != "https" {
                let safe_url = url.replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;");
                return Ok(format!("<untrusted_web_content url=\"{}\">\nFETCH FAILED: Only http and https schemes are permitted\n</untrusted_web_content>", safe_url));
            }

            let host_str = parsed_url.host_str().unwrap_or("");
            let host_lower = host_str.to_ascii_lowercase();

            // 🛡️ [SSRF Defense] Block cloud metadata and dangerous internal addresses
            if host_lower == "169.254.169.254"
                || host_lower.starts_with("169.254.")
                || host_lower == "metadata.google.internal"
                || host_lower == "instance-data"
                || host_lower == "0.0.0.0"
            {
                let safe_url = url.replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;");
                return Ok(format!("<untrusted_web_content url=\"{}\">\nFETCH FAILED: Access to cloud metadata or unspecified addresses is prohibited\n</untrusted_web_content>", safe_url));
            }

            // Strict local check: must be EXACT local host, NOT a subdomain like localhost.attacker.com
            let is_strict_local = host_lower == "127.0.0.1" || host_lower == "localhost" || host_lower == "::1";

            // Dedicated client with no automatic redirect following to prevent SSRF redirect bounces
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_else(|_| (*self.state.resources.http_client).clone());

            let mut request = client.get(url);
            if is_strict_local {
                request = request.header(
                    reqwest::header::AUTHORIZATION,
                    format!("Bearer {}", self.state.security.deploy_token),
                );
            }

            match request.send().await {
                Ok(mut r) => {
                    // Stream response with a strict 1 MB cap to prevent OOM
                    const MAX_BYTES: usize = 1024 * 1024;
                    let mut bytes = Vec::new();
                    while let Ok(Some(chunk)) = r.chunk().await {
                        if bytes.len() + chunk.len() > MAX_BYTES {
                            let remaining = MAX_BYTES.saturating_sub(bytes.len());
                            bytes.extend_from_slice(&chunk[..remaining]);
                            break;
                        }
                        bytes.extend_from_slice(&chunk);
                    }
                    let text = String::from_utf8_lossy(&bytes).to_string();

                    // 🛡️ [M45: Prompt-Injection Defense] Neutralize boundary escapes and chat template tokens
                    let sanitized = text
                        .replace("</untrusted_web_content>", "&lt;/untrusted_web_content&gt;")
                        .replace("<untrusted_web_content", "&lt;untrusted_web_content")
                        .replace("<|im_start|>", "")
                        .replace("<|im_end|>", "")
                        .replace("<|endoftext|>", "");
                    let truncated = self.safe_truncate(&sanitized, 8000);
                    let safe_url = url.replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;");
                    Ok(format!(
                        "<untrusted_web_content url=\"{}\">\n{}\n</untrusted_web_content>",
                        safe_url, truncated
                    ))
                }
                Err(e) => {
                    let safe_url = url.replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;");
                    Ok(format!("<untrusted_web_content url=\"{}\">\nFETCH FAILED: {}\n</untrusted_web_content>", safe_url, e))
                }
            }
        }
    }

    /// Handles `query_financial_logs`: retrieves and analyzes mission cost history.
    pub(crate) async fn handle_query_financial_logs(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        _usage: &mut Option<crate::agent::types::TokenUsage>,
    ) -> Result<String, ToolExecutionError> {
        let raw_limit = fc.args.get("limit").and_then(|v| v.as_i64()).unwrap_or(10);
        let limit = raw_limit.clamp(1, 100);

        tracing::info!(
            "📊 [Governance] Agent {} querying financial history (limit: {})...",
            ctx.agent_id,
            limit
        );
        self.broadcast_agent(ctx, "📊 Audit: reviewing fiscal logs...", "info");

        let history = crate::agent::mission::get_recent_missions(&self.state.resources.pool, limit).await?;
        let history_json = serde_json::to_string_pretty(&history).unwrap_or_default();

        Ok(format!("MISSION HISTORY RETRIEVED:\n\n{}\n\nPlease analyze this history for cost anomalies, burn rates, or optimization opportunities.", history_json))
    }

    /// Handles `search_web`: performs a web search and returns snippets.
    pub(crate) async fn handle_search_web(
        &self,
        ctx: &RunContext,
        fc: &crate::agent::types::ToolCall,
        _usage: &mut Option<crate::agent::types::TokenUsage>,
    ) -> Result<String, ToolExecutionError> {
        let query = fc.args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        tracing::info!("🔍 [Surface] Agent {} searching web for: {}", ctx.agent_id, query);

        self.broadcast_agent(
            ctx,
            &format!("🔍 Oversight: wants to search the web for: {}. Review required.", query),
            "warning",
        );

        let approved = self
            .submit_oversight(
                crate::agent::types::ToolCallAudit {
                    id: uuid::Uuid::new_v4().to_string(),
                    agent_id: ctx.agent_id.clone(),
                    mission_id: Some(ctx.mission_id.clone()),
                    skill: "search_web".to_string(),
                    params: fc.args.clone(),
                    department: ctx.department.clone(),
                    description: format!("Web search query: {}", query),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                },
                Some(ctx.mission_id.clone()),
            )
            .await?;

        if !approved {
            return Ok("(Search REJECTED by Oversight)".to_string());
        }

        self.broadcast_agent(ctx, &format!("🔍 Surface: searching web for '{}'...", query), "info");

        // Simple DuckDuckGo HTML fallback for immediate value
        let search_url = format!("https://html.duckduckgo.com/html/?q={}", urlencoding::encode(query));
        
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_else(|_| (*self.state.resources.http_client).clone());

        match client.get(&search_url).send().await {
            Ok(mut r) => {
                const MAX_SEARCH_BYTES: usize = 512 * 1024;
                let mut bytes = Vec::new();
                while let Ok(Some(chunk)) = r.chunk().await {
                    if bytes.len() + chunk.len() > MAX_SEARCH_BYTES {
                        let remaining = MAX_SEARCH_BYTES.saturating_sub(bytes.len());
                        bytes.extend_from_slice(&chunk[..remaining]);
                        break;
                    }
                    bytes.extend_from_slice(&chunk);
                }
                let html = String::from_utf8_lossy(&bytes).to_string();
                // Simple regex to extract search result snippets from DDG HTML
                let re = regex::Regex::new(r#"class="result__snippet"[^>]*>([^<]+)</span>"#).unwrap();
                let mut snippets = Vec::new();
                for cap in re.captures_iter(&html) {
                    let snippet = cap[1].to_string()
                        .replace("</untrusted_web_content>", "&lt;/untrusted_web_content&gt;")
                        .replace("<untrusted_web_content", "&lt;untrusted_web_content");
                    snippets.push(snippet);
                    if snippets.len() >= 5 { break; }
                }

                if snippets.is_empty() {
                    Ok(format!("(SEARCH RESULTS FOR '{}'): No direct snippets found. Use 'fetch_url' to visit the search page directly: {}", query, search_url))
                } else {
                    let combined = snippets.join("\n\n---\n\n");
                    Ok(format!("(SEARCH RESULTS FOR '{}'):\n\n{}\n\nUse 'fetch_url' to visit specific sites for more detail.", query, combined))
                }
            }
            Err(e) => {
                Ok(format!("(SEARCH FAILED: {})", e))
            }
        }
    }
}





// Metadata: [external_tools]
