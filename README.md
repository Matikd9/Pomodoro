<div align="center">
  <img alt="Pomotroid" src=".github/images/pomotroid-title.png" width="800px">
</div>
<div align="center">
  <img alt="Pomotroid in action" src=".github/images/pomotroid-screens.png" width="800px">
</div>

<p align="center">Pomodoro timer with tasks, statistics and a self-hosted server.</p>

---

- [Overview](#overview)
- [Features](#features)
- [Tasks and manual time](#tasks-and-manual-time)
- [Statistics](#statistics)
- [Themes](#themes)
- [Headless server and remote mode](#headless-server-and-remote-mode)
- [Building the desktop app](#building-the-desktop-app)
- [WebSocket API](#websocket-api)
- [Development](#development)
- [License](#license)

## Overview

This is a personal fork of [Splode/pomotroid](https://github.com/Splode/pomotroid) (v1.7.1). On top of the original timer it adds named tasks, timer presets, a count-up mode, Telegram notifications, a weekly Obsidian export, and a headless server that serves the same UI over HTTP so the desktop app, the browser and other devices share one timer and one history.

Built with [Tauri 2](https://tauri.app), [Rust](https://www.rust-lang.org) and [SvelteKit](https://svelte.dev) with Svelte 5.

## Features

- **Configurable timer**: work and break durations, rounds per long break, and optional short or long breaks.
- **Timer presets**: named sets of durations you can switch between (a `Default` preset is always present).
- **Continuous mode**: a count-up stopwatch that never auto-completes, for open-ended study sessions.
- **Tasks**: every session is recorded against a task; time and rounds per task are shown in the stats window.
- **Manual time adjustments**: add time you forgot to track, or remove time from a session you forgot to pause.
- **Statistics**: daily view with hourly breakdown, Monday–Sunday weekly view with day and week navigation, weekly focus goal, and a yearly heatmap based on hours studied.
- **39 bundled themes** plus custom JSON themes with hot reload; follows the OS light/dark preference.
- **Mini player**: a compact floating window with the countdown and controls.
- **Telegram notifications**: a message from your own bot when a focus or break round ends (set it up in **Settings → Telegram**).
- **Obsidian export**: a weekly Markdown report written to `Documents/Obsidian/Pomodoro`, keeping the reflection notes you add under its notes heading.
- **Headless server**: run the timer in Docker and use it from any browser, or point the desktop app at it.
- **Localization**: English and Spanish, auto-detected from the OS.
- From the original app: global and in-window shortcuts, custom alert sounds, tick sounds, dynamic tray icon with progress arc, minimise/close to tray, desktop notifications, always on top, opt-in WebSocket server, and rotating log files.

## Tasks and manual time

Pick the current task from the selector in the main window. Sessions shorter than 2 minutes are not recorded. Resetting the timer saves the time studied so far and keeps the round counter (`X | 4`); only the midnight rollover sends the sequence back to round 1. Completing or deleting the current task falls back to the `General` task.

The **Tasks** tab of the statistics window lists active, completed and deleted tasks with their time and rounds for today, this week, this month or all time. The clock button on a task opens **Adjust time**:

- **Add**: pick a date (today by default), a duration and an optional start time. Without a start time the entry ends now, or at 23:59 for a past day. Entries must be between 2 minutes and 12 hours and cannot end in the future. You are warned if the entry overlaps another session.
- **Remove**: pick a date, choose one of that task's finished sessions and enter the minutes to remove. A preview shows the result (`1h 50m → 1h 10m`). A session left under 2 minutes is deleted after confirmation. The session in progress is not listed; reset the timer first.

Manual entries are marked `manual`. They count in the statistics like any other session, but they do not move the live round counter.

## Statistics

The statistics window has four tabs: **Tasks**, **Today** (hourly breakdown and time per task), **This Week** (hours per day, weekly goal, streak) and **All Time** (yearly heatmap, total rounds, focus hours and best streak).

Rounds are fractional: a session counts as `duration / target`, so 50 minutes against a 25-minute target is 2 rounds.

<div align="center">
  <img alt="Pomotroid statistics window" src=".github/images/pomotroid-stats.png" width="800px">
</div>

## Themes

Pomotroid ships with 39 themes. **Dark Red** is the default dark theme.

![Screenshots of Pomotroid using various themes](.github/images/pomotroid-themes-preview.png)

See [THEMES.md](./THEMES.md) for the theme list and how to create your own. Besides the required colors, a theme can set these optional keys:

- `--color-background-gradient`: a CSS gradient drawn behind the window.
- `--color-heat-1` to `--color-heat-4`: the four heatmap levels, from least to most time. When they are missing, the heatmap uses the theme's focus color mixed with the background at 30, 55, 80 and 100 %.

## Headless server and remote mode

`server/` is an [axum](https://github.com/tokio-rs/axum) server that reuses the desktop app's timer, database and settings code. It exposes the same features as REST routes under `/api/*` plus a `/ws` WebSocket, and serves the built frontend.

### Remote mode in the desktop app

The desktop app connects to a remote server when one is configured in **Settings → System**. On startup each window checks `/api/state` with a 1.5 s timeout. If the server does not answer, that window uses the local backend until the app is restarted. Leave the URL empty to always use the local backend. The browser build always talks to the server it was loaded from.

### Run with Docker Compose

```bash
docker compose up --build
```

The UI is then at `http://localhost:8085`. Data is stored in `./data`. The container uses `TZ=America/Santiago`; change it in `docker-compose.yml` so "today" matches your time zone.

### Publish an image and update a server

Build on your machine and push to Docker Hub (use `linux/arm64` for an ARM server):

```bash
docker build --platform linux/amd64 -t matikd19/pomotroid:latest .
docker push matikd19/pomotroid:latest
```

On the server, the compose file must use `image: matikd19/pomotroid:latest` instead of `build: .`. Back up the data, then pull and recreate the container:

```bash
cp -r data data.bak-$(date +%F)
docker compose pull
docker compose up -d
docker logs --tail 20 pomotroid
```

Database migrations run automatically when the server starts.

### Environment variables

| Variable     | Default     | Description                    |
| ------------ | ----------- | ------------------------------ |
| `PORT`       | `8085`      | HTTP port                      |
| `DATA_DIR`   | `./data`    | Folder for the SQLite database |
| `STATIC_DIR` | `/app/dist` | Built frontend served at `/`   |
| `RUST_LOG`   | `info`      | Log level                      |
| `TZ`         | (none)      | Time zone used for daily stats |

If the server stops answering while using 0 % CPU, a thread is stuck on a lock. `docker restart pomotroid` recovers it.

## Building the desktop app

Requirements: [Node.js](https://nodejs.org) 22, the [Rust toolchain](https://rustup.rs) and the [Tauri 2 prerequisites](https://tauri.app/start/prerequisites/) for your OS (on Windows, the Microsoft C++ Build Tools and WebView2).

```bash
npm install
npm run tauri build
```

On Windows this produces:

- `src-tauri/target/release/pomotroid.exe`: the standalone executable.
- `src-tauri/target/release/bundle/nsis/Pomotroid_<version>_x64-setup.exe`: the installer.
- `src-tauri/target/release/bundle/msi/Pomotroid_<version>_x64_en-US.msi`: the MSI package.

To build only the NSIS installer:

```bash
npm run tauri build -- --bundles nsis
```

For a faster debug build without installers, which outputs `src-tauri/target/debug/pomotroid.exe`:

```bash
npm run tauri build -- --debug --no-bundle
```

Only one instance runs at a time. If Pomotroid is already open, a newly launched copy just focuses the existing window, so close the app before testing a new build.

Local data lives in `%APPDATA%\com.splode.pomotroid\`. The log file, useful when the window stays blank, is `%LOCALAPPDATA%\com.splode.pomotroid\logs\Pomotroid.log`.

## WebSocket API

Separate from the headless server, the desktop app has an optional local WebSocket server (disabled by default) for stream overlays and scripts. Enable it in **Settings → System**, then connect to `ws://127.0.0.1:<port>` (default port: 1314).

**Client → Server**

| Message                  | Description                     |
| ------------------------ | ------------------------------- |
| `{ "type": "getState" }` | Request the current timer state |

**Server → Client**

| Event         | Payload             | Description                                      |
| ------------- | ------------------- | ------------------------------------------------ |
| `state`       | `TimerState` object | Response to `getState`                           |
| `roundChange` | `TimerState` object | Fired whenever the timer advances to a new round |
| `error`       | `{ message }`       | Protocol error                                   |

`TimerState` fields: `elapsed_secs`, `total_secs`, `is_running`, `is_paused`, `round_type`, `work_round_number`, `work_rounds_total`.

## Development

```bash
npm install
npm run tauri dev      # desktop app with hot reload (Vite on :1420)
npm run dev            # frontend only, without Tauri IPC
```

Checks run by CI (`.github/workflows/build.yml`):

```bash
npm run check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

After touching backend files shared with the server (database, settings, timer engine, Obsidian, Telegram), also build the server:

```bash
cargo build --manifest-path server/Cargo.toml
```

Project layout:

| Path                       | Contents                                                       |
| -------------------------- | -------------------------------------------------------------- |
| `src/`                     | SvelteKit frontend: windows in `routes/`, IPC in `lib/ipc/`    |
| `src-tauri/`               | Tauri app: commands, timer engine, SQLite, tray, audio, themes |
| `server/`                  | Headless HTTP/WebSocket server                                 |
| `static/themes/`           | Built-in theme JSON files                                      |
| `src/messages/`            | UI strings per locale                                          |
| `openspec/changes/archive` | Design history of past features                                |

[CLAUDE.md](./CLAUDE.md) has a more detailed architecture guide, and [CONTRIBUTING.md](./CONTRIBUTING.md) describes the upstream setup and release process.

### Localization

UI strings live in `src/messages/en.json` and `src/messages/es.json`. The compiled output in `src/paraglide/` is generated and not committed. After adding or changing keys, regenerate it so types resolve:

```bash
npm run paraglide:compile
```

`npm run check` and the Vite plugin also run it.

## License

MIT &copy; [Christopher Murphy](https://github.com/Splode). Fork changes by [Matikd9](https://github.com/Matikd9).
