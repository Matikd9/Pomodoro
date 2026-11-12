# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Pomotroid is a Pomodoro timer desktop app: **Tauri 2 + Rust** backend (`src-tauri/`), **SvelteKit + Svelte 5** frontend (`src/`, static adapter, output in `build/`). There is also a **headless web server** (`server/`) that serves the same frontend over HTTP/WebSocket and is deployed with Docker.

## Commands

```bash
npm run tauri dev          # Desktop app with hot reload (Vite on :1420 + Tauri)
npm run tauri build        # Production desktop build
npm run dev                # Frontend only — no Tauri IPC available
npm run build              # Compile paraglide + static frontend build into build/
npm run check              # paraglide compile + svelte-kit sync + svelte-check (TS/Svelte types)
npm run format             # Prettier (single quotes, 2 spaces, width 100)
npm run paraglide:compile  # Regenerate src/paraglide/ after editing src/messages/*.json

cargo check  --manifest-path src-tauri/Cargo.toml
cargo test   --manifest-path src-tauri/Cargo.toml
cargo test   --manifest-path src-tauri/Cargo.toml <test_name_substring>   # single test
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings           # CI requires zero warnings

cargo build --manifest-path server/Cargo.toml    # headless server
docker compose up --build                        # server + frontend on :8085 (data in ./data)
```

CI (`.github/workflows/build.yml`) runs `npm run check`, clippy with `-D warnings`, and `cargo test`. Rust unit tests live in `#[cfg(test)] mod tests` blocks inside the modules (settings, db/migrations, commands, audio, …); there are no frontend tests.

## Architecture

### Windows / routes

All windows are frameless webviews. `main` (`src/routes/+page.svelte`) is the timer; `settings`, `stats` and `mini` (compact player) are opened from `Titlebar.svelte` via `new WebviewWindow(label, …)` and live under `src/routes/<label>/`. Every window must keep the custom titlebar drag region and support `uiScale` (applied with CSS `zoom`, not `transform: scale`).

### IPC layer

All frontend↔backend calls go through `src/lib/ipc/index.ts` (typed wrappers over `invoke()`/`listen()`). Never call `invoke()`/`listen()` directly from components or routes.

**Remote mode.** Every wrapper in `ipc/index.ts` has two paths. With `isRemoteMode()` it uses HTTP (`remoteFetch`, 8 s timeout) against the headless server plus a `/ws` WebSocket for events; otherwise it uses Tauri `invoke`/`listen`. The remote URL lives in `localStorage` (`pomotroid_remote_url`). If that key is unset it falls back to `DEFAULT_REMOTE_URL`, the user's Tailscale server `http://100.64.60.57:8085`; an empty string forces local mode. It can be changed in Settings → System. In the desktop app, `+layout.ts` calls `initRemoteMode()` before any window renders. That probes `/api/state` with a 1.5 s timeout, and if the server is down the window uses the local backend for the rest of its life (restart the app to reconnect). Without this probe, an unreachable server left the `.exe` blank for 16+ s with dead controls. The web build (`!isTauri`) is always remote. Rust commands are in `src-tauri/src/commands.rs`, return `Result<T, String>`, and must be registered in the `invoke_handler` list in `src-tauri/src/lib.rs`. New frontend-facing plugin APIs must be allowlisted in `src-tauri/capabilities/default.json`.

Backend → frontend events: `timer:started`, `timer:tick`, `timer:paused`, `timer:resumed`, `timer:round-change`, `timer:reset` (full `TimerSnapshot`, used to resync after settings changes), `timer:task-changed`, `timer:day-changed` (midnight rollover resets "today" counters), `settings:changed`, `presets:changed`, `themes:changed`, `sessions:cleared`.

### Rust ↔ TypeScript contract

`src/lib/types.ts` mirrors the Rust structs with snake_case fields. When changing `Settings` (`src-tauri/src/settings/mod.rs`), `TimerSnapshot` (`src-tauri/src/timer/mod.rs`) or query result structs, update `types.ts` too.

Volume is stored in the DB as **0–100** and exposed as **0.0–1.0**. Timer durations are stored and exposed in **seconds** (`time_*_secs`; MIGRATION_2 converted the old minute keys).

### Settings and database

SQLite (rusqlite). Settings are a key/value table. Some DB keys differ from struct field names (e.g. `work_rounds` → `long_break_interval`). Adding a setting means: field + load mapping in `settings/mod.rs`, default in `settings/defaults.rs`, and a key handler in `settings_set` (`commands.rs`), plus the `types.ts` field.

Other tables: `sessions` (each with a `task_name`), `tasks` (soft complete/delete flags; "General" is the fallback task that gets auto-selected), `presets` (named timer-duration sets). Queries are in `src-tauri/src/db/queries.rs`.

Migrations in `src-tauri/src/db/migrations.rs` are **append-only**: never edit an existing `MIGRATION_N`. Add a new constant, apply it in `run()` behind the `schema_version` check, and have it insert its version row.

### Timer engine

`timer/engine.rs` runs the clock on a background thread driven by `TimerCommand` messages and emits `TimerEvent`s. `TimerController` (`timer/mod.rs`) is the Tauri-managed API: it records sessions, emits events, and handles day rollover. `SequenceState` (`timer/sequence.rs`) tracks round type/count and keeps its own `work_rounds_total`, which must be resynced via `apply_settings()` when settings change. `timer_mode == "continuous"` is a count-up mode: duration stays 0 and rounds never auto-complete or reset.

### Headless server shares backend code

`server/src/main.rs` pulls in desktop modules through `#[path = "../../src-tauri/src/..."]` (migrations, queries, settings, defaults, obsidian, telegram, timer engine/sequence). It exposes `/api/*` REST routes plus a WebSocket, and serves the static frontend from `STATIC_DIR`. Those shared files must stay free of Tauri-only dependencies, and changing their signatures can break the server build even when the desktop app compiles. Run `cargo build --manifest-path server/Cargo.toml` after touching them.

Production runs as the `pomotroid` container (image `matikd19/pomotroid:latest`, port 8085) on the Tailscale host `makinadb` (100.64.60.57). Shell access is the user's own SSH session; diagnose with `docker ps -a`, `docker logs pomotroid`, and `curl localhost:8085/api/state` there.

### Mutex rule (desktop and server)

State lives in several `std::sync::Mutex`es (`db`, `sequence`, `settings`, `shared`, `current_task`). They are **not reentrant**: if a thread locks one it already holds, it waits on itself forever. `get_snapshot()`, `build_snapshot*()`, `set_task()` and `broadcast_task()` all lock `db` internally, so **drop every DB guard (scope it in a block) before calling them**. Breaking this froze the whole server: it spent 0 % CPU with every thread in `futex_wait`, and even `/` stopped answering because both tokio workers were stuck. If the server hangs like that, `docker restart pomotroid` recovers it; then look for a guard held across one of those calls.

### Other backend modules

`themes` (built-in JSON in `static/themes/` + user dir watched with `notify`, emits `themes:changed`; the frontend applies `colors` as CSS custom properties on `:root` via `stores/theme.ts`), `tray` (tiny-skia progress-arc icon), `audio` (rodio; custom sounds), `shortcuts` (global shortcuts; local in-window shortcuts are in `src/lib/utils/localShortcuts.ts`), `websocket` (opt-in axum server for overlays), `telegram` (Bot API notifications), `obsidian` (weekly Markdown export).

### Localization

Paraglide (`@inlang/paraglide-js`). Messages live in `src/messages/{locale}.json`; the active locales are set in `project.inlang/settings.json` (currently `en`, `es`). `src/paraglide/` is generated and not committed: run `npm run paraglide:compile` after adding keys so types resolve. Import messages through `$lib/locale.svelte.ts`.

### Conventions

- Svelte 5 runes only (`$state`, `$derived`, `$effect`, `$props`); no `export let`, `$:` or `let:`.
- Conventional Commits (`feat:`, `fix:`, `refactor:`, `chore:`, `test:`).
- Feature design history lives in `openspec/changes/archive/` (proposal/design/spec per feature).
- Never change production code just to make a test pass.

### Stop and ask the user before

- Adding any npm package or Rust crate (`npm install <pkg>`, `cargo add`).
- Adding permissions or capabilities to `src-tauri/capabilities/default.json`.
- Changing the signature of an existing command in `commands.rs` when the frontend change it forces is outside the task.
- Dropping or altering columns that hold existing sessions/settings data without a non-destructive migration.
- Retrying after the same `npm run check` / `cargo check` error survives two fix attempts.

### Verification before calling a task done

Run what applies to the files touched:

- Frontend: `npm run check`, `npx prettier --check .` (for touched files), `npm run build`.
- Rust: `cargo check`, `cargo test`, `cargo clippy -- -D warnings` (all with `--manifest-path src-tauri/Cargo.toml`); also build `server/` if shared modules changed.
- Desktop smoke test: `npm run tauri build -- --debug` (or `--bundles nsis` for a release installer), then launch `src-tauri/target/<profile>/pomotroid.exe` and confirm the window renders and the timer runs. Compiling is not enough.

App log (useful when the window is blank): `%LOCALAPPDATA%\com.splode.pomotroid\logs\Pomotroid.log`; data/DB: `%APPDATA%\com.splode.pomotroid\`.
