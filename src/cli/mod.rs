//! CLI module

pub mod args;
pub mod bootstrap;

mod session_list;
mod session_print;

pub use args::CliArgs;
use clap::Parser;

use crate::error::CliError;
use crate::agent::LspBackend;
use crate::tools::agent_tool::AgentTool;
use futures::future::BoxFuture;
use std::sync::Arc;

use crate::coordinator::agent::CoordinatorAgentBackend;
use crate::coordinator::Coordinator;

/// Build the full tool list: built-in tools + MCP tools, filtered by allow/deny lists
#[allow(clippy::too_many_arguments)]
async fn build_tools(
    global_config: &crate::config::GlobalConfig,
    project_config: &crate::config::ProjectConfig,
    extra_mcp_servers: Option<&std::collections::HashMap<String, crate::config::McpServerConfig>>,
    coordinator: Option<&Arc<Coordinator>>,
    api_client: Option<&Arc<crate::api::client::ApiClient>>,
    model: Option<&String>,
    working_dir: &std::path::Path,
    env: &std::collections::HashMap<String, String>,
) -> Result<Vec<Arc<dyn crate::types::Tool>>, CliError> {
    use std::collections::HashMap;
    use std::sync::Arc;
    use crate::config::McpServerConfig;
    use crate::mcp::load_mcp_tools;
    use crate::tools::ToolRegistry;
    use crate::types::Tool;
    use crate::plugins::PluginRegistry;

    // Load plugins
    let mut plugin_registry = PluginRegistry::new();
    if let Err(e) = plugin_registry.load_all().await {
        eprintln!("[Warning] Plugin loading failed: {e}");
    }
    for plugin in plugin_registry.list() {
        if plugin.hooks.is_empty() && plugin.tools.is_empty() && plugin.commands.is_empty() {
            continue;
        }
        eprintln!("[Plugin] Loaded: {} v{} (tools: {:?}, commands: {:?})",
            plugin.name, plugin.version, plugin.tools, plugin.commands);
    }

    // Collect allowed/denied tool lists
    let mut allowed = global_config.allowed_tools.clone();
    allowed.extend(project_config.allowed_tools.clone());
    let denied = &global_config.denied_tools;

    // Register built-in tools
    let registry = ToolRegistry::register_builtins();

    // Collect MCP servers (project overrides global for same-named servers)
    let mut servers: HashMap<String, McpServerConfig> = global_config.mcp_servers.clone();
    if let Some(ref project_servers) = project_config.mcp_servers {
        for (name, config) in project_servers {
            servers.insert(name.clone(), config.clone());
        }
    }
    // CLI --mcp-config overrides all
    if let Some(extra) = extra_mcp_servers {
        for (name, config) in extra {
            servers.insert(name.clone(), config.clone());
        }
    }

    // Load MCP tools
    let mcp_tools = load_mcp_tools(&servers).await.unwrap_or_else(|e| {
        eprintln!("[Warning] Failed to load some MCP servers: {e}");
        Vec::new()
    });

    // Load plugin tools
    let plugin_tools: Vec<Arc<dyn Tool>> = plugin_registry.list()
        .iter()
        .flat_map(|plugin| {
            plugin.tools.iter().map(|tool_name| {
                Arc::new(crate::plugins::tool::PluginTool::new(
                    tool_name.clone(),
                    format!("Tool '{}' from plugin '{}'", tool_name, plugin.name),
                    serde_json::json!({"type": "object", "properties": {}}),
                    plugin.path.clone(),
                )) as Arc<dyn Tool>
            })
        })
        .collect();

    // Combine all tools
    let builtin_tools = registry.get_all();
    let mut all_tools: Vec<Arc<dyn Tool>> = builtin_tools;
    all_tools.extend(mcp_tools);
    all_tools.extend(plugin_tools);

    // Add AgentTool if coordinator + api_client are available
    if let (Some(coord), Some(api), Some(model_str)) = (coordinator, api_client, model) {
        let backend = Arc::new(CoordinatorAgentBackend {
            coordinator: Arc::clone(coord),
            api_client: Arc::clone(api),
            model: model_str.clone(),
            tools: Arc::new(all_tools.clone()),
            working_directory: working_dir.to_path_buf(),
            env: env.clone(),
        });
        all_tools.push(Arc::new(AgentTool::new(backend)));
    }

    // Apply filter
    if allowed.is_empty() && denied.is_empty() {
        Ok(all_tools)
    } else {
        Ok(registry.filter(&allowed, denied))
    }
}

pub async fn run() -> Result<(), CliError> {
    let args = CliArgs::parse();

    if args.list_sessions {
        return session_list::run().await;
    }

    if let Some(ref session_id) = args.print_sessions {
        return session_print::run(session_id).await;
    }

    if args.print || args.output.is_some() {
        run_non_interactive(&args).await?;
    } else if args.resume.is_some() {
        run_resume(&args).await?;
    } else {
        run_interactive(&args).await?;
    }

    Ok(())
}

async fn run_interactive(args: &CliArgs) -> Result<(), CliError> {
    use crate::agent::context::AgentContext;
    use crate::agent::engine::{AgentEngine, AgentOutcome};
    use crate::api::client::ApiClient;
    use crate::types::{Session, SessionConfig};
    use crate::session::{SessionStorage, TranscriptManager};
    use crate::commands::CommandRegistry;
    use crate::coordinator::Coordinator;
    use crate::tui::event_loop::{run_repl, ReplState};

    let bootstrap = bootstrap::Bootstrap::new(
        args.model.clone(),
        args.permission_mode.clone(),
        args.dangerously_skip_permission.clone(),
        args.add_env.clone(),
        args.system_prompt.clone(),
        args.mcp_config.clone(),
        args.verbose,
    )
    .await?;

    // Start LSP servers from global config
    let workspace_root = std::env::current_dir().unwrap_or_default();
    let mut lsp_client = crate::lsp::LspClient::new(workspace_root.clone());
    for (lang, config) in &bootstrap.global_config.lsp_servers {
        let binary_config = crate::lsp::LspServerConfig {
            command: config.command.clone(),
            args: config.args.clone(),
            env: config.env.clone(),
        };
        if let Err(e) = lsp_client.start_server(lang, &binary_config).await {
            eprintln!("[Warning] Failed to start LSP server for {lang}: {e}");
        } else {
            eprintln!("[LSP] Started server for {lang}");
        }
    }

    // Wrap lsp_client for sharing via LspBackend trait
    let lsp_backend: Option<Arc<dyn LspBackend>> =
        Some(Arc::new(crate::lsp::LspBackendImpl(Arc::new(tokio::sync::Mutex::new(lsp_client)))));


    let api_client = ApiClient::new(
        bootstrap.provider,
        Some(bootstrap.api_key.as_str()),
        Some(bootstrap.base_url.as_str()),
    )
    .await?;

    let session_config = SessionConfig::default().with_permission_mode(bootstrap.permission_mode);
    let session_model = session_config.model.clone();
    let mut session = Session::new(
        uuid::Uuid::new_v4().to_string(),
        session_model.clone(),
    );
    session.system_prompt = bootstrap.system_prompt.clone()
        .or(Some(crate::types::DEFAULT_SYSTEM_PROMPT.to_string()));

    // Coordinator must be created before build_tools so AgentTool can use it
    let (coordinator, _coordinator_rx) = Coordinator::new();
    let coordinator = Arc::new(coordinator);

    let mut env: std::collections::HashMap<String, String> = std::env::vars().collect();
    env.extend(bootstrap.extra_env.clone());

    let working_dir = std::env::current_dir().unwrap_or_default();
    let session_model_clone = session_model.clone();

    let tools = build_tools(
        &bootstrap.global_config,
        &bootstrap.project_config,
        bootstrap.extra_mcp_servers.as_ref(),
        Some(&coordinator),
        Some(&Arc::new(api_client.clone())),
        Some(&session_model_clone),
        &working_dir,
        &env,
    )
    .await?;

    let custom_commands = bootstrap.project_config.custom_commands.clone();

    // Clone for ReplState (AgentContext takes ownership of tools and env)
    let tools_for_repl = tools.clone();
    let env_for_repl = env.clone();


    let context = AgentContext::new(
        session,
        session_config,
        tools,
        working_dir.clone(),
        bootstrap.global_config,
        bootstrap.project_config,
        env,
        lsp_backend,
    );

    let engine = AgentEngine::new(api_client.clone(), context);
    let storage = SessionStorage::default();

    let engine_arc = Arc::new(tokio::sync::Mutex::new(engine));
    let storage_arc = Arc::new(storage);
    let session_id = engine_arc.lock().await.session().id.clone();
    let transcript_arc = Arc::new(TranscriptManager::new(&session_id));

    let mut cmd_registry = CommandRegistry::new();
    cmd_registry.register_all(custom_commands);

    let session_arc = Arc::new(tokio::sync::Mutex::new(engine_arc.lock().await.session().clone()));

    let on_message = {
        let session_arc = session_arc.clone();
        move |prompt: String| -> BoxFuture<'static, bool> {
            let eng = engine_arc.clone();
            let stor = storage_arc.clone();
            let trans = transcript_arc.clone();
            let session_arc = session_arc.clone();
            Box::pin(async move {
                let mut eng = eng.lock().await;
                let outcome = eng.run(prompt).await;
                let session = eng.session().clone();
                *session_arc.lock().await = session.clone();
                let _ = stor.save(&session).await;
                if let Some(msg) = session.messages.last() {
                    let _ = trans.append(msg).await;
                }
                match outcome {
                    Ok(AgentOutcome::Completed) => true,
                    Ok(AgentOutcome::Error(msg)) => { eprintln!("[Error] {msg}"); true }
                    Ok(AgentOutcome::Interrupted) => { println!("[Interrupted]"); true }
                    Err(e) => { eprintln!("[Error] {e}"); true }
                }
            })
        }
    };

    let repl_state = ReplState {
        session: session_arc,
        provider: bootstrap.provider.to_string(),
        command_registry: Arc::new(cmd_registry),
        coordinator,
        api_client: Arc::new(api_client),
        tools: Arc::new(tools_for_repl),
        working_directory: working_dir.clone(),
        env: env_for_repl,
    };

    repl_state.wire_registry().await;

    run_repl(on_message, repl_state).await
        .map_err(|e| CliError::Other(format!("TUI error: {e}")))?;

    Ok(())
}

async fn run_non_interactive(args: &CliArgs) -> Result<(), CliError> {
    use crate::agent::context::AgentContext;
    use crate::agent::engine::{AgentEngine, AgentOutcome};
    use crate::api::client::ApiClient;
    use crate::types::{Session, SessionConfig};

    let bootstrap = bootstrap::Bootstrap::new(
        args.model.clone(),
        args.permission_mode.clone(),
        args.dangerously_skip_permission.clone(),
        args.add_env.clone(),
        args.system_prompt.clone(),
        args.mcp_config.clone(),
        args.verbose,
    )
    .await?;

    let prompt = args.combined_prompt().unwrap_or_default();

    let api_client = ApiClient::new(
        bootstrap.provider,
        Some(bootstrap.api_key.as_str()),
        Some(bootstrap.base_url.as_str()),
    )
    .await?;

    let session_config = SessionConfig::default().with_permission_mode(bootstrap.permission_mode);
    let session = Session::new(
        uuid::Uuid::new_v4().to_string(),
        session_config.model.clone(),
    );

    let tools = build_tools(
        &bootstrap.global_config,
        &bootstrap.project_config,
        bootstrap.extra_mcp_servers.as_ref(),
        None,
        None,
        None,
        &std::env::current_dir().unwrap_or_default(),
        &std::collections::HashMap::<String, String>::new(),
    )
    .await?;

    let mut env: std::collections::HashMap<String, String> = std::env::vars().collect();
    env.extend(bootstrap.extra_env.clone());

    let context = AgentContext::new(
        session,
        session_config,
        tools,
        std::env::current_dir().unwrap_or_default(),
        bootstrap.global_config,
        bootstrap.project_config,
        env,
        None,
    );

    let mut engine = AgentEngine::new(api_client, context);

    match engine.run(prompt).await {
        Ok(AgentOutcome::Completed) => {}
        Ok(AgentOutcome::Error(msg)) => {
            eprintln!("Agent error: {msg}");
            return Err(CliError::Other(msg));
        }
        Ok(AgentOutcome::Interrupted) => {}
        Err(e) => return Err(e),
    }

    Ok(())
}

async fn run_resume(args: &CliArgs) -> Result<(), CliError> {
    use crate::agent::context::AgentContext;
    use crate::agent::engine::{AgentEngine, AgentOutcome};
    use crate::api::client::ApiClient;
    use crate::types::SessionConfig;

    let session_id = args.resume.as_ref().unwrap();

    let bootstrap = bootstrap::Bootstrap::new(
        args.model.clone(),
        args.permission_mode.clone(),
        args.dangerously_skip_permission.clone(),
        args.add_env.clone(),
        args.system_prompt.clone(),
        args.mcp_config.clone(),
        args.verbose,
    )
    .await?;

    let session = bootstrap.session_storage.load(session_id).await
        .map_err(|e| CliError::Session(format!("failed to load session: {e}")))?;

    let api_client = ApiClient::new(
        bootstrap.provider,
        Some(bootstrap.api_key.as_str()),
        Some(bootstrap.base_url.as_str()),
    )
    .await?;

    let session_config = SessionConfig::default().with_permission_mode(bootstrap.permission_mode);
    let tools = build_tools(
        &bootstrap.global_config,
        &bootstrap.project_config,
        bootstrap.extra_mcp_servers.as_ref(),
        None,
        None,
        None,
        &std::env::current_dir().unwrap_or_default(),
        &std::collections::HashMap::<String, String>::new(),
    )
    .await?;

    let mut env: std::collections::HashMap<String, String> = std::env::vars().collect();
    env.extend(bootstrap.extra_env.clone());

    let context = AgentContext::new(
        session,
        session_config,
        tools,
        std::env::current_dir().unwrap_or_default(),
        bootstrap.global_config,
        bootstrap.project_config,
        env,
        None,
    );

    let mut engine = AgentEngine::new(api_client, context);
    match engine.run_resume().await {
        Ok(AgentOutcome::Completed) => {}
        Ok(AgentOutcome::Error(msg)) => {
            eprintln!("Agent error: {msg}");
            return Err(CliError::Other(msg));
        }
        Ok(AgentOutcome::Interrupted) => {}
        Err(e) => return Err(e),
    }

    // Save session after resume
    let session = engine.session().clone();
    if let Err(e) = bootstrap.session_storage.save(&session).await {
        eprintln!("[Warning] Failed to save session: {e}");
    }

    Ok(())
}
