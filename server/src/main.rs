use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Json, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::{ServeDir, ServeFile};

// Import shared business logic from src-tauri
#[path = "../../src-tauri/src/db/migrations.rs"]
pub mod migrations;

#[path = "../../src-tauri/src/db/queries.rs"]
pub mod queries;

#[path = "../../src-tauri/src/obsidian.rs"]
pub mod obsidian;

#[path = "../../src-tauri/src/settings/defaults.rs"]
pub mod defaults;

#[path = "../../src-tauri/src/settings/mod.rs"]
pub mod settings;

#[path = "../../src-tauri/src/timer/engine.rs"]
pub mod engine;

#[path = "../../src-tauri/src/timer/sequence.rs"]
pub mod sequence;

use engine::{EngineHandle, TimerCommand, TimerEvent};
use sequence::{RoundType, SequenceState};
use settings::Settings;

// ---------------------------------------------------------------------------
// Snapshot & Payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerSnapshot {
    pub round_type: String,
    pub previous_round_type: String,
    pub elapsed_secs: u32,
    pub total_secs: u32,
    pub is_running: bool,
    pub is_paused: bool,
    pub work_round_number: u32,
    pub work_rounds_total: u32,
    pub session_work_count: u32,
    pub today_focus_secs: u32,
    pub current_task: String,
}

#[derive(Clone, Serialize)]
pub struct ElapsedPayload {
    pub elapsed_secs: u32,
}

#[derive(Clone, Serialize)]
pub struct StartedPayload {
    pub total_secs: u32,
}

#[derive(Clone, Serialize)]
pub struct TickPayload {
    pub elapsed_secs: u32,
    pub total_secs: u32,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WsEvent {
    Started { payload: StartedPayload },
    Tick { payload: TickPayload },
    RoundChange { payload: TimerSnapshot },
    Paused { payload: ElapsedPayload },
    Resumed { payload: ElapsedPayload },
    Reset { payload: TimerSnapshot },
    SettingsChanged { payload: Settings },
    SessionsCleared,
}

// ---------------------------------------------------------------------------
// Shared Server State
// ---------------------------------------------------------------------------

struct TimerShared {
    elapsed_secs: u32,
    is_running: bool,
}

pub struct ServerController {
    engine: EngineHandle,
    sequence: Arc<Mutex<SequenceState>>,
    settings: Arc<Mutex<Settings>>,
    shared: Arc<Mutex<TimerShared>>,
    pub db: Arc<Mutex<Connection>>,
    pub static_dir: PathBuf,
    current_task: Arc<Mutex<String>>,
    broadcast_tx: broadcast::Sender<WsEvent>,
}

impl ServerController {
    pub fn get_snapshot(&self) -> TimerSnapshot {
        let seq = self.sequence.lock().unwrap();
        let s = self.settings.lock().unwrap();
        let sh = self.shared.lock().unwrap();
        let task = self.current_task.lock().unwrap().clone();

        let today_focus_secs = if let Ok(conn) = self.db.lock() {
            queries::get_today_focus_secs(&conn).unwrap_or(0)
        } else {
            0
        };

        TimerSnapshot {
            round_type: seq.current_round.as_str().to_string(),
            previous_round_type: seq.previous_round.map(|r| r.as_str().to_string()).unwrap_or_default(),
            elapsed_secs: sh.elapsed_secs,
            total_secs: seq.current_duration_secs(&s),
            is_running: sh.is_running,
            is_paused: !sh.is_running && sh.elapsed_secs > 0,
            work_round_number: seq.work_round_number,
            work_rounds_total: s.long_break_interval,
            session_work_count: seq.session_work_count,
            today_focus_secs,
            current_task: task,
        }
    }

    pub fn toggle(&self) -> TimerSnapshot {
        let s = self.shared.lock().unwrap();
        if s.is_running {
            self.engine.send(TimerCommand::Pause);
        } else if s.elapsed_secs > 0 {
            self.engine.send(TimerCommand::Resume);
        } else {
            self.engine.send(TimerCommand::Start);
        }
        drop(s);
        self.get_snapshot()
    }

    pub fn reset(&self) -> TimerSnapshot {
        self.sequence.lock().unwrap().reset();
        self.engine.send(TimerCommand::Reset);
        self.get_snapshot()
    }

    pub fn skip(&self) -> TimerSnapshot {
        self.engine.send(TimerCommand::Skip);
        self.get_snapshot()
    }

    pub fn restart_round(&self) -> TimerSnapshot {
        self.engine.send(TimerCommand::Reset);
        self.get_snapshot()
    }

    pub fn set_task(&self, task: String) -> TimerSnapshot {
        let clean = if task.trim().is_empty() {
            "General".to_string()
        } else {
            task.trim().to_string()
        };
        *self.current_task.lock().unwrap() = clean.clone();
        if let Ok(conn) = self.db.lock() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES ('last_task_name', ?1)",
                rusqlite::params![clean],
            );
            let _ = queries::create_task(&conn, &clean);
        }
        self.get_snapshot()
    }
}

// ---------------------------------------------------------------------------
// Server Initialization
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Data directory resolution: environment DATA_DIR or default to ./data
    let data_dir = std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string());
    std::fs::create_dir_all(&data_dir).expect("Failed to create data directory");
    let db_path = Path::new(&data_dir).join("pomotroid.db");
    log::info!("[server] Opening database at {:?}", db_path);

    let conn = Connection::open(&db_path).expect("Failed to open sqlite database");
    migrations::run(&conn).expect("Failed to run migrations");

    let initial_settings = {
        let _ = settings::seed_defaults(&conn);
        settings::load(&conn).unwrap_or_default()
    };

    let initial_task = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'last_task_name'",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "General".to_string());

    let seq = SequenceState::new(initial_settings.long_break_interval);
    let duration = seq.current_duration_secs(&initial_settings);

    let (engine, event_rx) = engine::spawn(duration, Duration::from_secs(1));

    let sequence = Arc::new(Mutex::new(seq));
    let settings_arc = Arc::new(Mutex::new(initial_settings));
    let shared = Arc::new(Mutex::new(TimerShared {
        elapsed_secs: 0,
        is_running: false,
    }));
    let db_arc = Arc::new(Mutex::new(conn));
    let current_task = Arc::new(Mutex::new(initial_task));
    let (broadcast_tx, _) = broadcast::channel::<WsEvent>(128);

    // Event listener thread
    {
        let seq_thread = Arc::clone(&sequence);
        let settings_thread = Arc::clone(&settings_arc);
        let shared_thread = Arc::clone(&shared);
        let engine_thread = engine.clone();
        let db_thread = Arc::clone(&db_arc);
        let task_thread = Arc::clone(&current_task);
        let tx = broadcast_tx.clone();

        std::thread::Builder::new()
            .name("server-timer-events".to_string())
            .spawn(move || {
                let mut current_session_id: Option<i64> = None;
                const MIN_RECORD_SECS: u32 = 120;

                while let Ok(event) = event_rx.recv() {
                    match event {
                        TimerEvent::Started { total_secs } => {
                            shared_thread.lock().unwrap().is_running = true;
                            let _ = tx.send(WsEvent::Started {
                                payload: StartedPayload { total_secs },
                            });
                        }
                        TimerEvent::Tick { elapsed_secs, total_secs } => {
                            {
                                let mut s = shared_thread.lock().unwrap();
                                s.elapsed_secs = elapsed_secs;
                                s.is_running = true;
                            }
                            let _ = tx.send(WsEvent::Tick {
                                payload: TickPayload { elapsed_secs, total_secs },
                            });

                            let is_work = seq_thread.lock().unwrap().current_round == RoundType::Work;
                            if is_work {
                                if elapsed_secs == MIN_RECORD_SECS && current_session_id.is_none() {
                                    let total = {
                                        let seq = seq_thread.lock().unwrap();
                                        let s = settings_thread.lock().unwrap();
                                        seq.current_duration_secs(&s)
                                    };
                                    let task = task_thread.lock().unwrap().clone();
                                    if let Ok(conn) = db_thread.lock() {
                                        if let Ok(id) = queries::insert_session(&conn, "work", elapsed_secs, total, &task) {
                                            current_session_id = Some(id);
                                        }
                                    }
                                } else if elapsed_secs > MIN_RECORD_SECS && elapsed_secs % 15 == 0 {
                                    if let Some(session_id) = current_session_id {
                                        if let Ok(conn) = db_thread.lock() {
                                            let _ = queries::update_session_progress(&conn, session_id, elapsed_secs);
                                        }
                                    }
                                }
                            }
                        }
                        TimerEvent::Complete { skipped } => {
                            let completed_round = seq_thread.lock().unwrap().current_round;
                            if completed_round == RoundType::Work {
                                let total = {
                                    let seq = seq_thread.lock().unwrap();
                                    let s = settings_thread.lock().unwrap();
                                    seq.current_duration_secs(&s)
                                };
                                if !skipped {
                                    if let Some(session_id) = current_session_id.take() {
                                        if let Ok(conn) = db_thread.lock() {
                                            let _ = queries::complete_session(&conn, session_id, total, true);
                                        }
                                    } else if total >= MIN_RECORD_SECS {
                                        let task = task_thread.lock().unwrap().clone();
                                        if let Ok(conn) = db_thread.lock() {
                                            if let Ok(id) = queries::insert_session(&conn, "work", total, total, &task) {
                                                let _ = queries::complete_session(&conn, id, total, true);
                                            }
                                        }
                                    }
                                } else {
                                    let elapsed = shared_thread.lock().unwrap().elapsed_secs;
                                    if let Some(session_id) = current_session_id.take() {
                                        if let Ok(conn) = db_thread.lock() {
                                            let _ = queries::complete_session(&conn, session_id, elapsed, false);
                                        }
                                    }
                                }
                            }
                            current_session_id = None;

                            let (next_round, next_duration, auto_start_work, auto_start_break) = {
                                let mut seq = seq_thread.lock().unwrap();
                                let s = settings_thread.lock().unwrap();
                                seq.work_rounds_total = s.long_break_interval;
                                let (rt, dur) = seq.advance(&s);
                                (rt, dur, s.auto_start_work, s.auto_start_break)
                            };

                            {
                                let mut s = shared_thread.lock().unwrap();
                                s.elapsed_secs = 0;
                                s.is_running = false;
                            }

                            engine_thread.send(TimerCommand::Prime { duration_secs: next_duration });

                            let snap = build_snapshot_raw(&seq_thread, &settings_thread, &shared_thread, &db_thread, &task_thread);
                            let _ = tx.send(WsEvent::RoundChange { payload: snap });

                            let should_auto = match next_round {
                                RoundType::Work => auto_start_work,
                                _ => auto_start_break,
                            };
                            if should_auto {
                                engine_thread.send(TimerCommand::Start);
                            }
                        }
                        TimerEvent::Paused { elapsed_secs } => {
                            shared_thread.lock().unwrap().is_running = false;
                            if let Some(session_id) = current_session_id {
                                if let Ok(conn) = db_thread.lock() {
                                    let _ = queries::update_session_progress(&conn, session_id, elapsed_secs);
                                }
                            }
                            let _ = tx.send(WsEvent::Paused { payload: ElapsedPayload { elapsed_secs } });
                        }
                        TimerEvent::Resumed { elapsed_secs } => {
                            shared_thread.lock().unwrap().is_running = true;
                            let _ = tx.send(WsEvent::Resumed { payload: ElapsedPayload { elapsed_secs } });
                        }
                        TimerEvent::Reset => {
                            let is_work = seq_thread.lock().unwrap().current_round == RoundType::Work;
                            let elapsed = shared_thread.lock().unwrap().elapsed_secs;
                            if is_work {
                                if let Some(session_id) = current_session_id.take() {
                                    if let Ok(conn) = db_thread.lock() {
                                        let _ = queries::complete_session(&conn, session_id, elapsed, false);
                                    }
                                }
                            }
                            current_session_id = None;

                            {
                                let mut s = shared_thread.lock().unwrap();
                                s.elapsed_secs = 0;
                                s.is_running = false;
                            }

                            let snap = build_snapshot_raw(&seq_thread, &settings_thread, &shared_thread, &db_thread, &task_thread);
                            let _ = tx.send(WsEvent::Reset { payload: snap });

                            let duration = {
                                let seq = seq_thread.lock().unwrap();
                                let s = settings_thread.lock().unwrap();
                                seq.current_duration_secs(&s)
                            };
                            engine_thread.send(TimerCommand::Prime { duration_secs: duration });
                        }
                        TimerEvent::Suspended { .. } => {}
                    }
                }
            })
            .expect("failed to spawn server timer events thread");
    }

    // Static Web directory (dist / build)
    let static_dir = std::env::var("STATIC_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            if Path::new("./dist").exists() {
                PathBuf::from("./dist")
            } else if Path::new("./build").exists() {
                PathBuf::from("./build")
            } else {
                PathBuf::from("../build")
            }
        });
    log::info!("[server] Serving static frontend from {:?}", static_dir);

    let controller = Arc::new(ServerController {
        engine,
        sequence,
        settings: settings_arc,
        shared,
        db: db_arc,
        static_dir: static_dir.clone(),
        current_task,
        broadcast_tx,
    });

    let index_file = static_dir.join("index.html");

    // Axum Router
    let app = Router::new()
        // API routes
        .route("/api/state", get(api_get_state))
        .route("/api/timer/toggle", post(api_timer_toggle))
        .route("/api/timer/reset", post(api_timer_reset))
        .route("/api/timer/skip", post(api_timer_skip))
        .route("/api/timer/restart", post(api_timer_restart))
        .route("/api/timer/task", post(api_timer_set_task))
        .route("/api/tasks", get(api_tasks_list).post(api_tasks_create))
        .route("/api/tasks/complete", post(api_tasks_complete))
        .route("/api/tasks/delete", post(api_tasks_delete))
        .route("/api/export/obsidian", post(api_export_obsidian))
        .route("/api/stats/detailed", get(api_stats_detailed))
        .route("/api/stats/heatmap", get(api_stats_heatmap))
        .route("/api/settings", get(api_settings_get).post(api_settings_set))
        .route("/api/settings/reset", post(api_settings_reset))
        .route("/api/sessions/clear", post(api_sessions_clear))
        .route("/api/themes", get(api_themes_list))
        // WebSocket
        .route("/ws", get(ws_handler))
        .with_state(controller)
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any));

    // Fallback to static web files if directory exists
    let router = if index_file.exists() {
        app.fallback_service(
            ServeDir::new(&static_dir).fallback(ServeFile::new(index_file)),
        )
    } else {
        log::warn!("[server] Frontend index.html not found at {:?}. API mode only.", index_file);
        app
    };

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8085);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    log::info!("[server] Pomotroid Server running on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind port 8085");
    axum::serve(listener, router).await.unwrap();
}

fn build_snapshot_raw(
    seq: &Arc<Mutex<SequenceState>>,
    settings: &Arc<Mutex<Settings>>,
    shared: &Arc<Mutex<TimerShared>>,
    db: &Arc<Mutex<Connection>>,
    task: &Arc<Mutex<String>>,
) -> TimerSnapshot {
    let s_seq = seq.lock().unwrap();
    let s_sett = settings.lock().unwrap();
    let s_sh = shared.lock().unwrap();
    let s_task = task.lock().unwrap().clone();

    let today_focus_secs = if let Ok(conn) = db.lock() {
        queries::get_today_focus_secs(&conn).unwrap_or(0)
    } else {
        0
    };

    TimerSnapshot {
        round_type: s_seq.current_round.as_str().to_string(),
        previous_round_type: s_seq.previous_round.map(|r| r.as_str().to_string()).unwrap_or_default(),
        elapsed_secs: s_sh.elapsed_secs,
        total_secs: s_seq.current_duration_secs(&s_sett),
        is_running: s_sh.is_running,
        is_paused: !s_sh.is_running && s_sh.elapsed_secs > 0,
        work_round_number: s_seq.work_round_number,
        work_rounds_total: s_sett.long_break_interval,
        session_work_count: s_seq.session_work_count,
        today_focus_secs,
        current_task: s_task,
    }
}

// ---------------------------------------------------------------------------
// HTTP API Handlers
// ---------------------------------------------------------------------------

async fn api_get_state(State(ctl): State<Arc<ServerController>>) -> Json<TimerSnapshot> {
    Json(ctl.get_snapshot())
}

async fn api_timer_toggle(State(ctl): State<Arc<ServerController>>) -> Json<TimerSnapshot> {
    Json(ctl.toggle())
}

async fn api_timer_reset(State(ctl): State<Arc<ServerController>>) -> Json<TimerSnapshot> {
    Json(ctl.reset())
}

async fn api_timer_skip(State(ctl): State<Arc<ServerController>>) -> Json<TimerSnapshot> {
    Json(ctl.skip())
}

async fn api_timer_restart(State(ctl): State<Arc<ServerController>>) -> Json<TimerSnapshot> {
    Json(ctl.restart_round())
}

#[derive(Deserialize)]
struct SetTaskReq {
    task: String,
}

async fn api_timer_set_task(
    State(ctl): State<Arc<ServerController>>,
    Json(req): Json<SetTaskReq>,
) -> Json<TimerSnapshot> {
    Json(ctl.set_task(req.task))
}

async fn api_tasks_list(State(ctl): State<Arc<ServerController>>) -> Result<Json<Vec<queries::TaskItem>>, StatusCode> {
    let conn = ctl.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tasks = queries::get_tasks(&conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(tasks))
}

#[derive(Deserialize)]
struct CreateTaskReq {
    name: String,
}

async fn api_tasks_create(
    State(ctl): State<Arc<ServerController>>,
    Json(req): Json<CreateTaskReq>,
) -> Result<Json<Vec<queries::TaskItem>>, StatusCode> {
    let conn = ctl.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tasks = queries::create_task(&conn, &req.name).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(tasks))
}

#[derive(Deserialize)]
struct ToggleTaskReq {
    name: String,
    completed: bool,
}

async fn api_tasks_complete(
    State(ctl): State<Arc<ServerController>>,
    Json(req): Json<ToggleTaskReq>,
) -> Result<Json<Vec<queries::TaskItem>>, StatusCode> {
    let conn = ctl.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tasks = queries::toggle_task_complete(&conn, &req.name, req.completed)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(tasks))
}

#[derive(Deserialize)]
struct DeleteTaskReq {
    name: String,
}

async fn api_tasks_delete(
    State(ctl): State<Arc<ServerController>>,
    Json(req): Json<DeleteTaskReq>,
) -> Result<Json<Vec<queries::TaskItem>>, StatusCode> {
    let conn = ctl.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let tasks = queries::delete_task(&conn, &req.name)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(tasks))
}

#[derive(Deserialize)]
struct ObsidianExportReq {
    week_offset: Option<i32>,
}

async fn api_export_obsidian(
    State(ctl): State<Arc<ServerController>>,
    Json(req): Json<ObsidianExportReq>,
) -> Result<Json<obsidian::ObsidianExportResult>, StatusCode> {
    let conn = ctl.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let result = obsidian::export_weekly_report(&conn, req.week_offset.unwrap_or(0), None)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(result))
}

#[derive(Serialize)]
struct DetailedStatsResp {
    today: queries::DailyStats,
    week: Vec<queries::DayStat>,
    streak: queries::StreakInfo,
    week_tasks: Vec<queries::TaskStat>,
}

async fn api_stats_detailed(
    State(ctl): State<Arc<ServerController>>,
) -> Result<Json<DetailedStatsResp>, StatusCode> {
    let conn = ctl.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let today = queries::get_daily_stats(&conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let week = queries::get_weekly_stats(&conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let streak = queries::get_streak(&conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let week_tasks = queries::get_weekly_task_breakdown(&conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(DetailedStatsResp { today, week, streak, week_tasks }))
}

#[derive(Serialize)]
struct HeatmapStatsResp {
    entries: Vec<queries::HeatmapEntry>,
    total_rounds: f32,
    total_hours: u32,
    longest_streak: u32,
}

async fn api_stats_heatmap(
    State(ctl): State<Arc<ServerController>>,
) -> Result<Json<HeatmapStatsResp>, StatusCode> {
    let conn = ctl.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let entries = queries::get_heatmap_data(&conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let raw = queries::get_all_time_stats(&conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let streak = queries::get_streak(&conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(HeatmapStatsResp {
        entries,
        total_rounds: raw.completed_work_sessions,
        total_hours: (raw.total_work_secs / 3600) as u32,
        longest_streak: streak.longest,
    }))
}

async fn api_settings_get(
    State(ctl): State<Arc<ServerController>>,
) -> Result<Json<Settings>, StatusCode> {
    let s = ctl.settings.lock().unwrap().clone();
    Ok(Json(s))
}

#[derive(Deserialize)]
struct SettingSetReq {
    key: String,
    value: String,
}

async fn api_settings_set(
    State(ctl): State<Arc<ServerController>>,
    Json(req): Json<SettingSetReq>,
) -> Result<Json<Settings>, StatusCode> {
    let conn = ctl.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    settings::save_setting(&conn, &req.key, &req.value)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let updated = settings::load(&conn)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    *ctl.settings.lock().unwrap() = updated.clone();
    ctl.sequence.lock().unwrap().work_rounds_total = updated.long_break_interval;

    let dur = ctl.sequence.lock().unwrap().current_duration_secs(&updated);
    let s = ctl.shared.lock().unwrap();
    let is_idle = !s.is_running && s.elapsed_secs == 0;
    drop(s);
    if is_idle {
        ctl.engine.send(TimerCommand::Reconfigure { duration_secs: dur });
    }

    let _ = ctl.broadcast_tx.send(WsEvent::SettingsChanged { payload: updated.clone() });
    let snap = ctl.get_snapshot();
    let _ = ctl.broadcast_tx.send(WsEvent::Reset { payload: snap });
    Ok(Json(updated))
}

async fn api_settings_reset(
    State(ctl): State<Arc<ServerController>>,
) -> Result<Json<Settings>, StatusCode> {
    let conn = ctl.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    conn.execute("DELETE FROM settings", []).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    settings::seed_defaults(&conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let updated = settings::load(&conn).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    *ctl.settings.lock().unwrap() = updated.clone();
    ctl.sequence.lock().unwrap().work_rounds_total = updated.long_break_interval;

    let dur = ctl.sequence.lock().unwrap().current_duration_secs(&updated);
    let s = ctl.shared.lock().unwrap();
    let is_idle = !s.is_running && s.elapsed_secs == 0;
    drop(s);
    if is_idle {
        ctl.engine.send(TimerCommand::Reconfigure { duration_secs: dur });
    }

    let _ = ctl.broadcast_tx.send(WsEvent::SettingsChanged { payload: updated.clone() });
    let snap = ctl.get_snapshot();
    if !snap.is_running && !snap.is_paused {
        let _ = ctl.broadcast_tx.send(WsEvent::Reset { payload: snap });
    }
    Ok(Json(updated))
}

async fn api_sessions_clear(
    State(ctl): State<Arc<ServerController>>,
) -> Result<StatusCode, StatusCode> {
    let conn = ctl.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    conn.execute("DELETE FROM sessions", []).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let _ = ctl.broadcast_tx.send(WsEvent::SessionsCleared);
    Ok(StatusCode::OK)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub name: String,
    pub colors: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub is_custom: bool,
}

pub fn load_themes(static_dir: &Path) -> Vec<Theme> {
    let themes_dir = if static_dir.join("themes").exists() {
        static_dir.join("themes")
    } else if Path::new("./static/themes").exists() {
        PathBuf::from("./static/themes")
    } else {
        PathBuf::from("../static/themes")
    };

    let mut list = Vec::new();
    if let Ok(entries) = std::fs::read_dir(themes_dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("json") {
                if let Ok(content) = std::fs::read_to_string(&p) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let (Some(name), Some(colors_obj)) = (
                            val.get("name").and_then(|n| n.as_str()),
                            val.get("colors").and_then(|c| c.as_object()),
                        ) {
                            let colors = colors_obj
                                .iter()
                                .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                                .collect();
                            list.push(Theme {
                                name: name.to_string(),
                                colors,
                                is_custom: false,
                            });
                        }
                    }
                }
            }
        }
    }
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

async fn api_themes_list(State(ctl): State<Arc<ServerController>>) -> Json<Vec<Theme>> {
    Json(load_themes(&ctl.static_dir))
}

// ---------------------------------------------------------------------------
// WebSocket
// ---------------------------------------------------------------------------

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(ctl): State<Arc<ServerController>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, ctl))
}

async fn handle_socket(socket: WebSocket, ctl: Arc<ServerController>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = ctl.broadcast_tx.subscribe();
    let (direct_tx, mut direct_rx) = mpsc::unbounded_channel::<String>();

    // Forward broadcast events and direct replies to client
    let mut send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                result = rx.recv() => {
                    let Ok(event) = result else { break };
                    let Ok(json) = serde_json::to_string(&event) else { continue };
                    if sender.send(Message::Text(json.into())).await.is_err() { break }
                }
                msg = direct_rx.recv() => {
                    let Some(json) = msg else { break };
                    if sender.send(Message::Text(json.into())).await.is_err() { break }
                }
            }
        }
    });

    // Handle incoming client messages
    let ctl_clone = Arc::clone(&ctl);
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    handle_client_ws(&text, &ctl_clone, &direct_tx).await;
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

async fn handle_client_ws(
    text: &str,
    ctl: &Arc<ServerController>,
    direct_tx: &mpsc::UnboundedSender<String>,
) {
    let Ok(msg) = serde_json::from_str::<serde_json::Value>(text) else { return };
    let msg_type = msg.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match msg_type {
        "getState" => {
            let snap = ctl.get_snapshot();
            let json = serde_json::to_string(&serde_json::json!({
                "type": "state",
                "payload": snap
            })).unwrap_or_default();
            let _ = direct_tx.send(json);
        }
        "toggle" => {
            let _ = ctl.toggle();
        }
        "reset" => {
            let _ = ctl.reset();
        }
        "skip" => {
            let _ = ctl.skip();
        }
        "restartRound" => {
            let _ = ctl.restart_round();
        }
        "setTask" => {
            if let Some(task) = msg.get("task").and_then(|t| t.as_str()) {
                let _ = ctl.set_task(task.to_string());
            }
        }
        _ => {}
    }
}
