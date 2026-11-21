pub mod agent_return;
mod commands;
pub mod files;
pub mod models;
pub mod office;
pub mod path_validator;
pub mod storage;

pub fn run() {
    use clap::{Parser, Subcommand};
    use tauri::{Emitter, Manager};

    #[derive(Debug, Parser)]
    #[command(name = "viewera", version, about = "Local file viewer")]
    struct Cli {
        #[command(subcommand)]
        command: Option<CliCommand>,
    }

    #[derive(Debug, Subcommand)]
    enum CliCommand {
        AgentReturn {
            #[arg(long)]
            payload: std::path::PathBuf,
            #[arg(long)]
            receipt: Option<std::path::PathBuf>,
            #[arg(long)]
            quiet: bool,
        },
    }

    fn handle_arguments(
        app: &tauri::AppHandle,
        mut arguments: Vec<String>,
        parse_cli: impl FnOnce(Vec<String>) -> Result<Option<CliCommand>, String>,
    ) {
        use tauri::Manager;
        if arguments
            .first()
            .and_then(|argument| std::path::Path::new(argument).file_stem())
            .is_none_or(|name| name != "viewera")
        {
            arguments.insert(0, "viewera".to_owned());
        }
        let command = match parse_cli(arguments) {
            Ok(Some(command)) => command,
            Ok(None) => return,
            Err(error) => {
                tracing::error!("{error}");
                let _ = app.emit("agent-return-error", error);
                return;
            }
        };
        match command {
            CliCommand::AgentReturn {
                payload,
                receipt,
                quiet,
            } => {
                let result = (|| {
                    let state = app.state::<agent_return::AgentReturnState>();
                    let receipt = state.receive_payload(&payload, receipt.as_deref())?;
                    let roots = app.state::<storage::AppState>().roots()?;
                    let results = state.results(&roots)?;
                    app.emit("agent-return-received", results)?;
                    Ok::<_, anyhow::Error>(receipt)
                })();
                match result {
                    Ok(receipt) => {
                        if !quiet {
                            tracing::info!(
                                request_id = receipt.request_id,
                                status = receipt.status,
                                "Agent Return accepted"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::error!(%error, "Agent Return was rejected");
                        let _ = app.emit("agent-return-error", error.to_string());
                    }
                }
            }
        }
    }

    let parse_cli = |arguments: Vec<String>| {
        Cli::try_parse_from(arguments)
            .map(|cli| cli.command)
            .map_err(|error| error.to_string())
    };
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info"))
        .init();
    let launch_arguments = std::env::args().collect::<Vec<_>>();
    if launch_arguments
        .get(1)
        .is_some_and(|argument| argument == "--version")
    {
        println!("viewera {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    if launch_arguments
        .get(1)
        .is_some_and(|argument| argument == "--help" || argument == "-h")
    {
        println!(
            "Usage: viewera [--version] [agent-return --payload PATH [--receipt PATH] [--quiet]]"
        );
        return;
    }
    if launch_arguments
        .get(1)
        .is_some_and(|argument| argument == "agent-return")
        && launch_arguments
            .iter()
            .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("Usage: viewera agent-return --payload PATH [--receipt PATH] [--quiet]");
        return;
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(
            move |app, args, _cwd| {
                handle_arguments(app, args, parse_cli);
                if let Some(window) = app.get_webview_window("main") {
                    if let Err(error) = window.show() {
                        tracing::error!(%error, "Could not show the main window");
                    }
                    if let Err(error) = window.set_focus() {
                        tracing::error!(%error, "Could not focus the main window");
                    }
                }
            },
        ))
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let data_dir = app.path().app_data_dir()?;
            let state = storage::AppState::load(&data_dir)?;
            let agent_returns = agent_return::AgentReturnState::load(&data_dir)?;
            for root in state.roots()? {
                app.asset_protocol_scope()
                    .allow_directory(std::path::Path::new(&root.path), true)?;
            }
            app.manage(state);
            app.manage(agent_returns);
            handle_arguments(app.handle(), launch_arguments, parse_cli);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_roots,
            commands::select_root_folder,
            commands::remove_root,
            commands::list_directory,
            commands::get_file_metadata,
            commands::list_zip_entries,
            commands::search_file_contents,
            commands::cancel_content_search,
            commands::get_favorites,
            commands::get_recent_files,
            commands::set_file_favorite,
            commands::record_recent_file,
            commands::get_preview_descriptor,
            commands::read_text_preview,
            commands::read_csv_preview,
            commands::read_workbook,
            commands::read_worksheet_range,
            commands::get_settings,
            commands::save_settings,
            commands::get_agent_results,
            commands::dismiss_agent_result,
            commands::confirm_agent_result_root,
            commands::open_agent_artifact,
            commands::open_agent_artifact_folder,
            commands::mark_agent_artifact_opened,
            commands::mark_agent_artifact_failed
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ViewerA");
}
