# RKStratum Bridge — Tauri 1.x desktop app

This package builds **rkstratum-bridge-desktop**: a Windows shell that runs **kaspa-stratum-bridge in-process** (same code path as the `stratum-bridge` CLI), including optional **in-process kaspad**, stratum listeners, health checks, and the **web dashboard** on `web_dashboard_port`.

### You do not run `stratum-bridge.exe` separately

**`rkstratum-bridge-desktop.exe` is a full replacement for the CLI binary in normal use.** It calls the same `kaspa_stratum_bridge::runner::run` as `stratum-bridge` (same config, sync, Stratum ports, mining). Do not start a second copy of the bridge unless you intentionally want two independent processes with different configs.

The WebView is only the dashboard UI; it is not a thin client that requires another `stratum-bridge` process.

**GUI mode (double-click, no arguments):** the app opens a **setup** screen where you choose network options, config path, app directory, and optional kaspad arguments, then click **Start bridge**. Only then does the embedded bridge run. The shell waits until the bridge HTTP port accepts TCP (`wait_for_dashboard_http`), then loads the **vendored operator dashboard** (`bridge-tauri/ui/dashboard/*.html`, `css/`, `js/`) in an **embedded frame** below the header. The frame URL is `dashboard/index.html?api=<bridge HTTP origin>`; scripts call `http://127.0.0.1:<port>/api/…` over the network (CORS is open on the bridge). The main window stays on the Tauri origin so **`invoke` works** for **Stop bridge**, **Open in browser**, and the menu. **Refreshing (F5)** while the bridge is still running reconnects to the embedded dashboard instead of leaving you stuck on setup.

**CLI mode (any extra arguments):** the bridge starts immediately; the app shows the same vendored dashboard once HTTP is ready.

**Keeping the desktop UI in sync with `bridge/static`:** the canonical operator UI lives in `bridge/static/` for the CLI and browsers. The desktop app ships a **copy** under `bridge-tauri/ui/dashboard/`. When you change `bridge/static`, refresh that copy (and re-apply `js/api-base.js` / `rkstratumApiUrl` patches in `dashboard.js` / `raw.js` if those files change), or add an automation step later.

## Requirements

- **Windows** with **WebView2 Runtime** (normal on Windows 10/11; optional [Evergreen installer](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) if missing).
- **Rust** toolchain matching the repo (`rust-version` in the workspace root `Cargo.toml`).
- **Node.js** (only if you use the npm scripts / Tauri CLI from npm).

## Build from the workspace root

Clone the **rusty-kaspa** repository (any directory name is fine), then from the **repository root** (where the root `Cargo.toml` lists `bridge-tauri/src-tauri` as a workspace member):

```powershell
cd path\to\rusty-kaspa
cargo build -p rkstratum-bridge-desktop --release
```

There are **no machine-specific paths** in the desktop crate: `kaspa-stratum-bridge` and `kaspa-alloc` are normal **relative** `path = "../../…"` dependencies from `bridge-tauri/src-tauri/`, so anyone with the same tree layout can build.

The binary is `target\release\rkstratum-bridge-desktop.exe`. Put `config.yaml` next to it (or rely on defaults / cwd search — same as the CLI).

### Command line (same as `stratum-bridge`)

Any time you pass **more than the program name**, arguments are parsed with the **same** [`Cli`](../../bridge/src/cli.rs) as `stratum-bridge`: `--config`, `--node-mode`, `--appdir`, `--coinbase-tag-suffix`, `--testnet`, instance flags, and **kaspad passthrough** after `--` (see `stratum-bridge --help`).

**Examples** (from the repo root; the first `--` is for `cargo`, the rest goes to the desktop exe):

```powershell
cargo run -p rkstratum-bridge-desktop --release --features rkstratum_cpu_miner -- `
  --config bridge/config.yaml `
  --node-mode inprocess `
  --appdir "C:\KaspaData" `
  --coinbase-tag-suffix dablacksplash `
  -- --utxoindex --rpclisten=127.0.0.1:16110 --rpclisten-borsh=127.0.0.1:17110 --rpclisten-json=127.0.0.1:18110 --uacomment=RKStratum
```

Installed binary (no `cargo`):

```powershell
.\rkstratum-bridge-desktop.exe --config bridge\config.yaml --node-mode inprocess --appdir "C:\KaspaData" --coinbase-tag-suffix dablacksplash -- --utxoindex --rpclisten=127.0.0.1:16110
```

**Internal CPU miner flags** (`--internal-cpu-miner`, …) require building with `--features rkstratum_cpu_miner` on **this** package (it forwards to `kaspa-stratum-bridge`).

If you **double-click** the exe with **no** arguments, you get the **setup** form first. If `config.yaml` exists next to the executable, that path is prefilled—you can edit it, clear it, or add flags such as `--node-mode` / `--appdir` before **Start**. Nothing runs until you click **Start**.

## Run (development)

From `bridge-tauri/`:

```powershell
npm install
npm run dev
```

Or with the Tauri CLI, from `bridge-tauri/src-tauri/`:

```powershell
cargo install tauri-cli --version "^1.5" --locked
cargo tauri dev
```

## Behavior

- **Embedded bridge**: a background thread runs `kaspa_stratum_bridge::runner::run` with `RKSTRATUM_BRIDGE_EMBEDDED=1` so a second Ctrl+C does not `exit()` the whole desktop process.
- **GUI vs CLI**: with **no** command-line arguments, the bridge starts only after **Start bridge** on the setup page. With **any** extra arguments, the bridge starts at launch (same as `stratum-bridge`).
- **Shutdown**: **Stop bridge** in the window header (or **Bridge → Stop bridge** in the menu) calls `stop_bridge` / `request_bridge_shutdown()`; closing the window or choosing Quit does the same and joins the bridge thread.
- **Dashboard UI**: the operator dashboard is embedded **below** the desktop header (iframe to loopback) so Tauri commands keep working. **Open in browser** launches the same URL in your default browser if you prefer a separate window.

## Security note

This is an **operator tool**: the WebView loads your bridge dashboard over HTTP. Prefer loopback or a trusted LAN URL; see `bridge/docs/UI.md`.
