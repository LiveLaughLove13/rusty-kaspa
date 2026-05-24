# `master` vs `RKStratumTN12` — maintainer diff write-up

**Repository:** [LiveLaughLove13/rusty-kaspa](https://github.com/LiveLaughLove13/rusty-kaspa)  
**Compared branches:** [`master`](https://github.com/LiveLaughLove13/rusty-kaspa/tree/master) → [`RKStratumTN12`](https://github.com/LiveLaughLove13/rusty-kaspa/tree/RKStratumTN12)  
**As of:** May 24, 2026 (after merge `aefc86c5` brought `master` into `RKStratumTN12`)

---

## Executive summary

`RKStratumTN12` is **not** a small bridge-only patch branch. It is a **Testnet 12 / Toccata tracking line** that contains:

1. **~200 commits of upstream Kaspa protocol work** (Toccata hard-fork, TN12 testnet, SMT/seq-commit, covenants, mempool policy, P2P v10, etc.) merged from [kaspanet/rusty-kaspa](https://github.com/kaspanet/rusty-kaspa) `toccata` / `tn12`.
2. **RK-Stratum operator/bridge enhancements** (full web dashboard, host metrics, Linux AppImage, sync-aware mining, security-minded HTTP binding).
3. **Everything already on `master` today**, including merged upstream PRs **#1015** (dashboard hashrate/uptime session sync) and **#1014** (default ASIC worker labels).

**Relationship today:** `master` is fully contained in `RKStratumTN12`. The merge-base is the current `master` tip (`a07d8b38`). There are **0 commits on `master` that are missing from `RKStratumTN12`**.

| Metric | Value |
|--------|-------|
| Commits on TN12 not in master | **209** |
| Commits on master not in TN12 | **0** |
| Files changed | **382** |
| Lines added / removed | **+49,743 / −4,965** |
| New files | **111** |
| Workspace version (master) | `1.1.0` |
| Workspace version (RKStratumTN12) | `1.2.0-toc.2` |

**Important for maintainers:** A literal “merge `RKStratumTN12` → `master`” would bring the **entire Toccata/TN12 protocol stack**, not just bridge UI work. If the goal is upstreaming bridge improvements to mainnet `master`, the realistic path is **selective cherry-picks of the `bridge/` (and related CI/release) commits**, not a wholesale merge.

---

## What is already identical between branches

These landed on `master` first (via kaspanet PRs #1015 and #1014) and were merged into `RKStratumTN12` in commit `aefc86c5`. **No diff remains** for the core worker-label / session-metric fixes:

| Area | Status |
|------|--------|
| `bridge/src/stratum_context.rs` — `asic-{id}` default worker name | Identical |
| `bridge/src/client_handler.rs` — wallet-based 5s stats hook, `sync_worker_prom_metrics` | Identical |
| `bridge/src/default_client.rs` — authorize + prom session sync | Identical (TN12 removes one blank line only) |
| Worker display / dashboard session metrics (#1014, #1015) | Identical |

TN12 had an earlier parallel commit (`f197b5ca`) for the same worker-name behavior; that is superseded by the merged `master` state.

---

## Part A — Upstream Toccata / Testnet 12 (~85% of the diff)

This is **kaspanet upstream work**, tracked on `RKStratumTN12` for TN12 experimentation. It is the bulk of the 382 changed files.

### Version and toolchain

- Rust MSRV: `1.88.0` → **`1.91.0`**
- Crate versions: `1.1.0` → **`1.2.0-toc.2`**

### Major new workspace crates (111 new files include these)

| Crate | Purpose |
|-------|---------|
| `consensus/seq-commit` | Sequential commitment verification (Toccata) |
| `consensus/smt-store` | Sparse Merkle tree store for pruning/IBD |
| `crypto/smt` | SMT crypto primitives |
| `build-info` | Git/build metadata (split from `utils`, upstream #1010) |
| `system-info` | System info (moved from `utils`) |

### Consensus and protocol (113 files under `consensus/`)

- **Toccata hard fork** (#1005, #1006): network params, genesis, mass model, transient mass activation
- **Covenants** — new client types, covenant ID hashing, WASM examples (Groth16, RISC0)
- **UTXO model changes** — `pre_toccata` compatibility, `utxo_entry`, DB compat tests
- **Virtual processor / pruning** — SMT-aware IBD, fork logging, bounds, seq-commit accessor
- **Transaction validation** — contextual validation split (header vs UTXO context), seqcommit replay boundaries (#1011)
- **Mempool** — raised min relay fee (#1004), gas/feerate selection refactor, `check_transaction_limits`, frontier search tree

### P2P and IBD (21 files under `protocol/`)

- **P2P protocol v10** — `request_pruning_point_smt_state`, SMT state sync
- **User-agent admission rules** (#981)
- **IBD fixes** — SMT flag in pre-toccata no-op sync (#1008), pruning point anticone guard (#980)
- **DNS seeders** for TN12 (#982)

### Node and docs

- `kaspad/src/args.rs` — TN12 / Toccata flags
- **`docs/testnet12.md`** — participation guide (port 16311, `--netsuffix=12`, Rothschild, kaspa-miner)
- `docs/override-params.md` updates

### CI aligned with upstream toccata

- Extended `no_std` checks (addresses, hashes, merkle, smt, seq-commit, utils)
- WASM tests for `consensus/client`
- Integration test fixtures updated for Toccata blocks

**Why this exists on `RKStratumTN12`:** operators need a **single branch** that runs TN12 consensus *and* the enhanced stratum bridge against `--testnet --netsuffix=12`.

### Top-level directory change counts

| Directory | Files changed |
|-----------|---------------|
| `consensus/` | 113 |
| `crypto/` | 63 |
| `mining/` | 28 |
| `utils/` | 26 |
| `bridge/` | 25 |
| `protocol/` | 21 |
| `wallet/` | 20 |
| `rpc/` | 16 |
| `testing/` | 15 |
| `wasm/` | 13 |
| Other | remainder |

---

## Part B — RK-Stratum fork work (the bridge/operator layer)

This is the **fork-specific value** most relevant when discussing a merge into `master`.  
**Bridge-only diff:** 25 files, **+5,486 / −470 lines**.

### B1. Web dashboard (largest single change)

| File | Change |
|------|--------|
| `bridge/static/js/dashboard.js` | **~2,625 lines added** — full operator UI |
| `bridge/static/index.html` | Restructured layout |
| `bridge/static/css/site.css` | **~713 lines** styling |
| `bridge/docs/UI.md` | **New** — UI reference doc |

**Dashboard capabilities (implemented):**

- **Header:** bridge status, kaspad version, instances, uptime, mining/network tiles, internal CPU miner stats
- **Kaspad node panel:** sync state, peers, DAG/header counts, virtual DAA, sink blue score, mempool, RPC difficulty vs Prometheus difficulty (with explanatory notes)
- **Trends and analytics:** session Chart.js charts + long-range Prometheus/Grafana operator tools
- **Bridge host panel:** CPU, RAM, load, disk I/O, network, optional geo (feature-gated)
- **Recent blocks:** filters, CSV export, charts, detail modal
- **Workers table:** responsive layout, wallet filter, session uptime, CSV export, `unnamed-asic` for legacy IP-keyed rows (#1014)
- **Raw debug view:** `raw.html` JSON dump

**Data sources:** polls `/api/status`, `/api/stats`, `/api/host` — no direct browser→kaspad RPC.

### B2. Host metrics and optional geo (`bridge/src/host_metrics.rs` — **new, 579 lines**)

| Feature | Detail |
|---------|--------|
| Compile-time features | `rkstratum_host_metrics` (sysinfo), `rkstratum_geoip` (ureq + host) — **default build includes geo feature chain** |
| Minimal build | `cargo build -p kaspa-stratum-bridge --no-default-features` |
| Runtime toggle | `approximate_geo_lookup` in config / CLI / `POST /api/config` |
| Privacy | Geo sends egress IP to configurable URL (default ip-api.com); documented opt-in |
| Operator location | `RKSTRATUM_LOCATION` env for manual label |

Exposed via `/api/status`, `/api/host`, and dashboard Host card.

### B3. HTTP API and Prometheus extensions (`bridge/src/prom.rs` — **+212 lines net**)

New / extended endpoints and status fields:

- `GET /api/host` — host snapshot
- `/api/status` — adds `host`, `host_metrics_enabled`, `geoip_enabled`, richer nested `node`
- `POST /api/config` — runtime config updates (gated by `RKSTRATUM_ALLOW_CONFIG_WRITE=1`)
- `ensure_worker_session_metrics` — shared with master (#1015)
- Host metrics background task on web server start

### B4. Node integration (`bridge/src/kaspaapi.rs` — **+253 lines net**)

- **`NodeStatusApi`** — structured JSON for dashboard (camelCase, timestamps, sink blue score)
- **`network_display_from_id()`** — shared network label parsing (also used in terminal stats)
- **Sync-aware mining** (commit `b37f1c75`):
  - `wait_for_mining_ready_with_shutdown` — waits for `get_sync_status` **and** `get_block_template` with `is_synced: true`
  - Refuses templates when `is_synced: false`
  - Internal CPU miner pauses on `Reject(IsInIBD)`
  - `[NODE]` log line uses `get_sync_status` so UI matches bridge behavior

### B5. Security and binding defaults (`bridge/src/net_utils.rs`)

| Listener | Port-only config (`:3030`) binds to |
|----------|-------------------------------------|
| **Stratum** (miners) | `0.0.0.0` — LAN miners can connect |
| **Dashboard / metrics HTTP** | **`127.0.0.1`** — not exposed to LAN by default |

Explicit `0.0.0.0:3030` required for remote dashboard access. Documented in `bridge/docs/README.md`.

### B6. Linux AppImage packaging (`bridge/appimage/` — **new**)

- `AppRun`, `build.sh`, `.desktop`, icon
- Release workflow builds `.AppImage.tar.gz` (preserves executable bit)
- Desktop launcher tries to open a terminal for logs; `RKSTRATUM_NO_AUTO_TERMINAL=1` to disable
- Config discovery: `$XDG_CONFIG_HOME/stratum-bridge/config.yaml`

### B7. Other bridge changes

| Item | Detail |
|------|--------|
| `bridge/Cargo.toml` | Features: `rkstratum_host_metrics`, `rkstratum_geoip`; mimalloc split for musl vs non-musl |
| `bridge/config.yaml` | `approximate_geo_lookup: true`; sample uses `0.0.0.0:3030` for dashboard |
| `bridge/src/share_handler.rs` | Terminal stats interval **10s → 60s**; internal CPU block all-time counter; uses shared `network_display_from_id` |
| `bridge/src/rkstratum_cpu_miner.rs` | Sync/IBD-aware work pausing |
| `bridge/src/main.rs` | Host metrics init, geo config, embedded kaspad detection |
| `bridge/src/tests.rs` | Host/status API tests, randomized in-process ports for TN12 CI |
| `bridge/docs/README.md` | Expanded operator docs (dashboard, geo, dual-bridge setup, AppImage) |

### B8. Release CI (`.github/workflows/deploy.yaml`)

- Linux AppImage build + tarball upload on release
- Requires `librsvg2-bin`, `fuse`, `libfuse2`

---

## Part C — Commit history shape (how the branch was built)

Recent TN12-only commits (top of `master..RKStratumTN12`):

```
aefc86c5  Merge branch 'master' into RKStratumTN12
f197b5ca  fix(bridge): default worker name (superseded by master merge)
a6a40894  Merge branch 'toccata' from kaspanet/rusty-kaspa
…         Upstream: #1012, #1011, #1010, #1008, #1007, #1006, #1005, #1004, …
a81c41da  Merge RkStratumUIAppImage (UI + AppImage)
0984732b  bridge: UI dashboard and host metrics
b37f1c75  Wait for sync (bridge mining readiness)
9a009e8d  Add Linux stratum-bridge AppImage packaging
```

The branch history is a **stack of upstream toccata merges** plus **RK-Stratum UI/AppImage/sync** work, periodically rebased/merged with `master`.

### Bridge-related commits on TN12 (selected)

| Commit | Description |
|--------|-------------|
| `0984732b` | bridge: UI dashboard and host metrics |
| `b37f1c75` | Wait for sync — mining readiness gating |
| `9a009e8d` | Linux stratum-bridge AppImage packaging |
| `a81c41da` | Merge RkStratumUIAppImage into RKStratumTN12 |
| `87f48186` | Randomize bridge in-process node listen ports (tests) |
| `f197b5ca` | Default worker name (now aligned with master #1014) |

---

## What merging `RKStratumTN12` → `master` would actually do

| If you merge wholesale | Effect |
|------------------------|--------|
| Protocol | Brings **Toccata / TN12** consensus, incompatible with current mainnet `master` |
| Version | Jumps `1.1.0` → `1.2.0-toc.2` |
| Bridge | Brings full dashboard, host metrics, AppImage, sync waits, local-first HTTP |
| Risk | **High** for mainnet — this is a testnet/hard-fork line, not a bridge-only feature branch |

### Recommended merge strategies (for maintainers)

**Option 1 — Full merge (only if `master` should become TN12/Toccata)**  
Appropriate if LiveLaughLove13 `master` is intentionally becoming a TN12 operator distribution. **Not** appropriate for kaspanet mainnet `master` without a coordinated hard-fork release.

**Option 2 — Bridge-only upstream (recommended for kaspanet bridge maintainers)**  
Cherry-pick or PR these areas from `RKStratumTN12` onto current `master`:

- `bridge/static/` (dashboard UI)
- `bridge/src/host_metrics.rs`, `bridge/src/net_utils.rs`, `bridge/src/kaspaapi.rs` (sync wait portions)
- `bridge/src/prom.rs` (host/API extensions — **excluding** anything already in #1015)
- `bridge/appimage/`
- `bridge/docs/UI.md`, `bridge/docs/README.md` updates
- `.github/workflows/deploy.yaml` AppImage steps
- `bridge/Cargo.toml` feature flags

Worker-label and session-metric fixes are **already on kaspanet `master`** via #1014/#1015.

**Option 3 — Keep branches separate (current practical setup)**  
- `master` — mainnet-aligned, receives kaspanet bridge fixes  
- `RKStratumTN12` — TN12/Toccata + RK-Stratum operator stack  

This matches how the branch is used today: TN12 testnet operators run `RKStratumTN12`; mainnet operators run `master`.

---

## Bridge file diff reference (accurate file list)

Files that differ between `master` and `RKStratumTN12` under `bridge/`:

```
bridge/Cargo.toml
bridge/appimage/*                    (new)
bridge/config.yaml
bridge/docs/README.md
bridge/docs/UI.md                    (new)
bridge/src/app_config.rs
bridge/src/cli.rs
bridge/src/default_client.rs         (trivial)
bridge/src/host_metrics.rs           (new)
bridge/src/kaspaapi.rs
bridge/src/lib.rs
bridge/src/main.rs
bridge/src/net_utils.rs
bridge/src/prom.rs
bridge/src/rkstratum_cpu_miner.rs
bridge/src/share_handler.rs
bridge/src/tests.rs
bridge/static/css/site.css
bridge/static/index.html
bridge/static/js/dashboard.js
bridge/static/js/raw.js
bridge/static/raw.html
```

**No diff:** `bridge/src/client_handler.rs`, `bridge/src/stratum_context.rs` (post-merge).

---

## Suggested PR title and description (bridge-only path)

**Title:** `bridge: RK-Stratum operator dashboard, host metrics, AppImage, and sync-aware mining`

**Summary:**

- Full web dashboard with node panel, workers/blocks analytics, wallet filter, CSV export, session charts
- Optional host metrics (CPU/RAM/load/disk/network) and opt-in approximate geo lookup
- Local-first default for dashboard/metrics HTTP (`127.0.0.1`); Stratum remains LAN-accessible
- Sync-aware block template / internal miner gating aligned with kaspad mining readiness
- Linux AppImage release packaging
- Builds on existing #1014 (ASIC worker labels) and #1015 (dashboard session/uptime) — no regression to those fixes

**Test plan:**

- `cargo test -p kaspa-stratum-bridge`
- Connect unnamed ASIC → dashboard shows `asic-N`, hashrate/uptime populate after authorize
- Dashboard on `127.0.0.1:3030` with default `:3030` config
- Optional: AppImage build via `bash bridge/appimage/build.sh <tag>`

---

## Verification commands (reproduce this analysis)

```powershell
git fetch origin master RKStratumTN12

# Scale
git diff --stat origin/master...origin/RKStratumTN12
git rev-list --left-right --count origin/master...origin/RKStratumTN12

# Bridge-only
git diff --stat origin/master...origin/RKStratumTN12 -- bridge/

# Confirm worker fixes identical
git diff origin/master origin/RKStratumTN12 -- bridge/src/client_handler.rs bridge/src/stratum_context.rs
```

---

## Branch tips (reference)

| Branch | Tip commit | Description |
|--------|------------|-------------|
| `origin/master` | `a07d8b38` | fix IPv4-looking worker labels (#1014) |
| `origin/RKStratumTN12` | `aefc86c5` | Merge branch 'master' into RKStratumTN12 |
| Merge-base | `a07d8b38` | Same as current master (master fully contained in TN12) |
