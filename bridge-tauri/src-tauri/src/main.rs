//! **RKStratum Bridge Desktop** — optional GUI to configure the bridge, then **Start** (same `runner::run` as
//! `stratum-bridge`). Launch with extra CLI arguments to skip the form and start immediately (scripting).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use clap::Parser;
use kaspa_alloc::init_allocator_with_default_settings;
use kaspa_stratum_bridge::cli::Cli;
use kaspa_stratum_bridge::{default_dashboard_iframe_url, request_bridge_shutdown, run};
use serde::Serialize;
use tauri::{CustomMenuItem, Manager, Menu, Submenu};

struct RunningBridge {
    join: std::thread::JoinHandle<()>,
    #[allow(dead_code)]
    cli: Cli,
}

#[derive(Default)]
struct AppState {
    bridge: Mutex<Option<RunningBridge>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartBridgeDto {
    config: Option<String>,
    #[serde(default)]
    testnet: bool,
    node_mode: Option<String>,
    appdir: Option<String>,
    coinbase_tag_suffix: Option<String>,
    #[serde(default)]
    kaspad_extra_args: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GuiDefaults {
    config_path: Option<String>,
    exe_directory: Option<String>,
    suggested_appdir: Option<String>,
}

fn spawn_running_bridge(cli: Cli) -> Result<RunningBridge, String> {
    std::env::set_var("RKSTRATUM_BRIDGE_EMBEDDED", "1");
    let cli_thread = cli.clone();
    let join = std::thread::Builder::new()
        .name("kaspa-stratum-bridge".into())
        .spawn(move || {
            init_allocator_with_default_settings();
            let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio runtime");
            if let Err(e) = rt.block_on(run(cli_thread)) {
                eprintln!("stratum bridge: {e:#}");
            }
        })
        .map_err(|e| format!("failed to spawn bridge thread: {e}"))?;
    Ok(RunningBridge { join, cli })
}

fn cli_from_start_dto(dto: StartBridgeDto) -> Result<Cli, String> {
    let argv0 = std::env::args().next().unwrap_or_else(|| "rkstratum-bridge-desktop".into());
    let mut args = vec![argv0];
    if let Some(c) = dto.config {
        let c = c.trim();
        if !c.is_empty() {
            args.push("--config".into());
            args.push(c.to_string());
        }
    }
    if dto.testnet {
        args.push("--testnet".into());
    }
    if let Some(ref nm) = dto.node_mode {
        let nm = nm.trim();
        if !nm.is_empty() {
            args.push("--node-mode".into());
            args.push(nm.to_string());
        }
    }
    if let Some(a) = dto.appdir {
        let a = a.trim();
        if !a.is_empty() {
            args.push("--appdir".into());
            args.push(a.to_string());
        }
    }
    if let Some(s) = dto.coinbase_tag_suffix {
        let s = s.trim();
        if !s.is_empty() {
            args.push("--coinbase-tag-suffix".into());
            args.push(s.to_string());
        }
    }
    if !dto.kaspad_extra_args.is_empty() {
        args.push("--".into());
        args.extend(dto.kaspad_extra_args);
    }
    Cli::try_parse_from(args).map_err(|e| e.to_string())
}

#[tauri::command]
fn is_cli_mode() -> bool {
    std::env::args().nth(1).is_some()
}

#[tauri::command]
fn gui_defaults() -> GuiDefaults {
    let exe = std::env::current_exe().ok();
    let exe_directory = exe.as_ref().and_then(|p| p.parent()).map(|p| p.to_string_lossy().into_owned());
    let beside = exe
        .as_ref()
        .and_then(|p| p.parent())
        .map(|d| d.join("config.yaml"))
        .filter(|p| p.is_file());
    let config_path = beside.as_ref().map(|p| p.to_string_lossy().into_owned());
    let suggested_appdir = exe_directory.as_ref().map(|d| format!("{d}\\kaspa-data"));
    GuiDefaults {
        config_path,
        exe_directory,
        suggested_appdir,
    }
}

#[tauri::command]
fn start_bridge(state: tauri::State<AppState>, dto: StartBridgeDto) -> Result<String, String> {
    let mut g = state.bridge.lock().map_err(|_| "bridge state lock poisoned".to_string())?;
    if g.is_some() {
        return Err("Bridge is already running. Use Bridge → Stop bridge first.".into());
    }
    let cli = cli_from_start_dto(dto)?;
    let url = default_dashboard_iframe_url(&cli);
    let running = spawn_running_bridge(cli)?;
    *g = Some(running);
    Ok(url)
}

#[tauri::command]
fn stop_bridge(state: tauri::State<AppState>) -> Result<(), String> {
    let mut g = state.bridge.lock().map_err(|_| "bridge state lock poisoned".to_string())?;
    let Some(running) = g.take() else {
        return Err("Bridge is not running.".into());
    };
    request_bridge_shutdown();
    running
        .join
        .join()
        .map_err(|_| "Bridge thread panicked while stopping.".to_string())?;
    Ok(())
}

#[tauri::command]
fn bridge_is_running(state: tauri::State<AppState>) -> bool {
    state.bridge.lock().map(|g| g.is_some()).unwrap_or(false)
}

#[tauri::command]
fn dashboard_default_url(state: tauri::State<AppState>) -> Result<String, String> {
    let g = state.bridge.lock().map_err(|_| "lock poisoned".to_string())?;
    let Some(r) = g.as_ref() else {
        return Err("Bridge is not running.".into());
    };
    Ok(default_dashboard_iframe_url(&r.cli))
}

/// Parse `http://127.0.0.1:3030/...` into a socket address for readiness checks.
fn dashboard_socket_addr(url: &str) -> Result<std::net::SocketAddr, String> {
    let u = url.trim();
    let rest = u
        .strip_prefix("http://")
        .or_else(|| u.strip_prefix("https://"))
        .ok_or("URL must start with http:// or https://")?;
    let authority = rest
        .split(&['/', '?', '#'][..])
        .next()
        .filter(|s| !s.is_empty())
        .ok_or("missing host in dashboard URL")?;
    authority.parse().map_err(|e| format!("invalid host:port in URL: {e}"))
}

/// Opens the repository README for setup (browser).
#[tauri::command]
fn open_bridge_documentation() -> Result<(), String> {
    const URL: &str = "https://github.com/kaspanet/rusty-kaspa/blob/master/bridge-tauri/README.md";
    open_os_url(URL)
}

/// Opens the folder containing the desktop executable (e.g. to edit `config.yaml`).
#[tauri::command]
fn reveal_exe_directory() -> Result<(), String> {
    let dir = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = dir.parent().ok_or_else(|| "executable has no parent directory".to_string())?;
    if cfg!(windows) {
        std::process::Command::new("explorer")
            .arg(dir.as_os_str())
            .spawn()
            .map_err(|e| e.to_string())?;
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(dir).spawn().map_err(|e| e.to_string())?;
    } else {
        std::process::Command::new("xdg-open").arg(dir).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn open_os_url(url: &str) -> Result<(), String> {
    if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(url)
            .spawn()
            .map_err(|e| e.to_string())?;
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(url).spawn().map_err(|e| e.to_string())?;
    } else {
        std::process::Command::new("xdg-open").arg(url).spawn().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn wait_for_dashboard_http(url: String) -> Result<(), String> {
    let addr = dashboard_socket_addr(&url)?;
    for attempt in 0u32..120 {
        match tokio::net::TcpStream::connect(addr).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                if attempt == 119 {
                    return Err(format!(
                        "Dashboard not reachable at {url} after 60s: {e}. Check web_dashboard_port and logs."
                    ));
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    }
    unreachable!("wait loop always returns Ok or Err")
}

fn try_start_from_cli(state: &AppState) {
    let args: Vec<String> = std::env::args().collect();
    if args.len() <= 1 {
        return;
    }
    let cli = match Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    let url = default_dashboard_iframe_url(&cli);
    match spawn_running_bridge(cli) {
        Ok(running) => {
            if let Ok(mut g) = state.bridge.lock() {
                *g = Some(running);
            }
            eprintln!("rkstratum-bridge-desktop: started bridge (CLI mode). Dashboard: {url}");
        }
        Err(e) => {
            eprintln!("rkstratum-bridge-desktop: failed to start bridge: {e}");
        }
    }
}

fn main() {
    let state = AppState::default();
    try_start_from_cli(&state);

    let menu = Menu::new().add_submenu(Submenu::new(
        "Bridge",
        Menu::new()
            .add_item(CustomMenuItem::new("stop_bridge", "Stop bridge"))
            .add_native_item(tauri::MenuItem::Quit),
    ));

    tauri::Builder::default()
        .menu(menu)
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            is_cli_mode,
            gui_defaults,
            start_bridge,
            stop_bridge,
            bridge_is_running,
            dashboard_default_url,
            wait_for_dashboard_http,
            open_bridge_documentation,
            reveal_exe_directory,
        ])
        .on_menu_event(|event| {
            if event.menu_item_id() == "stop_bridge" {
                let app = event.window().app_handle();
                if let Some(s) = app.try_state::<AppState>() {
                    if let Err(e) = stop_bridge(s) {
                        eprintln!("Stop bridge: {e}");
                    }
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                request_bridge_shutdown();
                if let Some(s) = app.try_state::<AppState>() {
                    if let Ok(mut g) = s.bridge.lock() {
                        if let Some(r) = g.take() {
                            let _ = r.join.join();
                        }
                    }
                }
            }
        });
}
