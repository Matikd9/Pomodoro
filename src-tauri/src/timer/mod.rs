pub mod engine;
pub mod sequence;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{AudioCue, AudioManager};
use crate::db::{queries, DbState};
use crate::settings::Settings;
use crate::tray::{self, TrayState};
use crate::websocket::{self, WsState};

use engine::{EngineHandle, TimerCommand, TimerEvent};
use sequence::{RoundType, SequenceState};

// ---------------------------------------------------------------------------
// Snapshot — serialized to JSON for the frontend
// ---------------------------------------------------------------------------

/// Full timer state snapshot. Sent as the payload of Tauri events and
/// returned by the `timer_get_state` IPC command.
#[derive(Debug, Clone, Serialize)]
pub struct TimerSnapshot {
    /// "work" | "short-break" | "long-break"
    pub round_type: String,
    /// Round type that was active before this one. Empty string on the first round of a session.
    pub previous_round_type: String,
    pub elapsed_secs: u32,
    pub total_secs: u32,
    pub is_running: bool,
    /// True if the timer has been started and then paused (elapsed > 0, not running).
    pub is_paused: bool,
    pub work_round_number: u32,
    pub work_rounds_total: u32,
    /// Monotonically-increasing focus round count since last reset. Used as a
    /// session counter when long breaks are disabled.
    pub session_work_count: u32,
    /// Total focus work seconds accumulated today from completed and partial sessions.
    pub today_focus_secs: u32,
    /// Currently selected task/subject (e.g. "General", "Math").
    pub current_task: String,
}

// ---------------------------------------------------------------------------
// Shared mutable state between the controller and the event-listener thread
// ---------------------------------------------------------------------------

struct TimerShared {
    elapsed_secs: u32,
    is_running: bool,
}

// ---------------------------------------------------------------------------
// TimerController — public API registered as Tauri state
// ---------------------------------------------------------------------------

pub struct TimerController {
    engine: EngineHandle,
    sequence: Arc<Mutex<SequenceState>>,
    settings: Arc<Mutex<Settings>>,
    shared: Arc<Mutex<TimerShared>>,
    /// Kept alive so TrayState is not dropped if lib.rs forgets its copy.
    #[allow(dead_code)]
    tray: Arc<TrayState>,
    db: DbState,
    current_task: Arc<Mutex<String>>,
}

impl TimerController {
    /// Construct and start the background threads.
    /// Call once from `lib.rs` during Tauri `setup`.
    pub fn new(
        app: AppHandle,
        settings: Settings,
        tray: Arc<TrayState>,
        db: DbState,
    ) -> Self {
        let seq = SequenceState::new(settings.long_break_interval);
        let duration = seq.current_duration_secs(&settings);

        let (engine, event_rx) = engine::spawn(duration, Duration::from_secs(1));

        let sequence = Arc::new(Mutex::new(seq));
        let settings_arc = Arc::new(Mutex::new(settings));
        let shared = Arc::new(Mutex::new(TimerShared {
            elapsed_secs: 0,
            is_running: false,
        }));

        let initial_task = if let Ok(conn) = db.lock() {
            conn.query_row(
                "SELECT value FROM settings WHERE key = 'last_task_name'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap_or_else(|_| "General".to_string())
        } else {
            "General".to_string()
        };
        let current_task = Arc::new(Mutex::new(initial_task));

        // Clone handles for the event-listener thread.
        let seq_thread = Arc::clone(&sequence);
        let settings_thread = Arc::clone(&settings_arc);
        let shared_thread = Arc::clone(&shared);
        let engine_thread = engine.clone();
        let tray_thread = Arc::clone(&tray);
        let db_thread = Arc::clone(&db);
        let current_task_thread = Arc::clone(&current_task);

        std::thread::Builder::new()
            .name("timer-events".to_string())
            .spawn(move || {
                listen_events(
                    app,
                    event_rx,
                    ListenContext {
                        sequence: seq_thread,
                        settings: settings_thread,
                        shared: shared_thread,
                        engine: engine_thread,
                        tray: tray_thread,
                        db: db_thread,
                        current_task: current_task_thread,
                    },
                );
            })
            .expect("failed to spawn timer event listener");

        Self {
            engine,
            sequence,
            settings: settings_arc,
            shared,
            tray,
            db,
            current_task,
        }
    }

    // --- Commands ---

    /// Toggle: start a fresh timer if idle, resume if paused, pause if running.
    pub fn toggle(&self) {
        let s = self.shared.lock().unwrap();
        if s.is_running {
            log::info!("[timer] pause");
            self.engine.send(TimerCommand::Pause);
        } else if s.elapsed_secs > 0 {
            log::info!("[timer] resume");
            self.engine.send(TimerCommand::Resume);
        } else {
            log::info!("[timer] start");
            self.engine.send(TimerCommand::Start);
        }
    }

    pub fn reset(&self) {
        log::info!("[timer] reset");
        self.sequence.lock().unwrap().reset();
        // Send only Reset — the event listener's Reset handler will follow up
        // with Prime once the engine is confirmed Idle. Sending a duration
        // update here first would race the Reset and can leave the UI stale.
        self.engine.send(TimerCommand::Reset);
    }

    /// Restart only the current round's timer without touching the sequence.
    /// Round type, round number, and position in the work/break cycle are all
    /// preserved — only the elapsed time is zeroed.
    pub fn restart_round(&self) {
        log::info!("[timer] restart round");
        self.engine.send(TimerCommand::Reset);
    }

    pub fn skip(&self) {
        log::info!("[timer] skip");
        self.engine.send(TimerCommand::Skip);
    }

    pub fn suspend(&self) {
        self.engine.send(TimerCommand::Suspend);
    }

    pub fn wake_resume(&self) {
        self.engine.send(TimerCommand::WakeResume);
    }

    /// Update the duration for the current round when settings change.
    /// Only takes effect after the next Start/Resume (current countdown is not interrupted).
    pub fn reconfigure(&self) {
        let duration = {
            let seq = self.sequence.lock().unwrap();
            let settings = self.settings.lock().unwrap();
            seq.current_duration_secs(&settings)
        };
        self.engine.send(TimerCommand::Reconfigure { duration_secs: duration });
    }

    // --- Query ---

    pub fn get_snapshot(&self) -> TimerSnapshot {
        build_snapshot(&self.sequence, &self.settings, &self.shared, &self.db, &self.current_task)
    }

    /// Sets the active task/subject, saving it as the default/last-used task in settings and DB.
    pub fn set_task(&self, task: String) {
        let clean = if task.trim().is_empty() {
            "General".to_string()
        } else {
            task.trim().to_string()
        };
        log::info!("[timer] set task={clean}");
        *self.current_task.lock().unwrap() = clean.clone();
        if let Ok(conn) = self.db.lock() {
            let _ = conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES ('last_task_name', ?1)",
                rusqlite::params![clean],
            );
            let _ = queries::create_task(&conn, &clean);
        }
    }

    /// Apply new settings values. Updates the in-memory copy and, if the
    /// timer is idle (not running and no elapsed progress), reconfigures the
    /// engine so the next Start uses the new duration.
    ///
    /// When the timer is running or paused, the current countdown is left
    /// untouched; the new duration takes effect at the start of the next
    /// round or after a manual reset.  Sending Reconfigure to a running
    /// engine transitions it to Idle, which would freeze the timer.
    pub fn apply_settings(&self, new: Settings) {
        // Sync work_rounds_total so the round counter and advance() logic both
        // reflect the new long_break_interval immediately.
        self.sequence.lock().unwrap().work_rounds_total = new.long_break_interval;
        *self.settings.lock().unwrap() = new;
        let s = self.shared.lock().unwrap();
        let is_idle = !s.is_running && s.elapsed_secs == 0;
        drop(s);
        if is_idle {
            self.reconfigure();
        }
    }
}

// ---------------------------------------------------------------------------
// Background event listener thread
// ---------------------------------------------------------------------------

struct ListenContext {
    sequence: Arc<Mutex<SequenceState>>,
    settings: Arc<Mutex<Settings>>,
    shared: Arc<Mutex<TimerShared>>,
    engine: EngineHandle,
    tray: Arc<TrayState>,
    db: DbState,
    current_task: Arc<Mutex<String>>,
}

fn listen_events(
    app: AppHandle,
    event_rx: std::sync::mpsc::Receiver<TimerEvent>,
    ctx: ListenContext,
) {
    let ListenContext { sequence, settings, shared, engine, tray, db, current_task } = ctx;
    // Track last tray progress to throttle redraws to ≥ 1% delta.
    let mut last_tray_progress: f32 = -1.0;
    // Active session row ID for recording (None = not started or < 2 min threshold).
    let mut current_session_id: Option<i64> = None;
    const MIN_RECORD_SECS: u32 = 120;

    while let Ok(event) = event_rx.recv() {
        match event {
            TimerEvent::Started { total_secs } => {
                log::info!("[timer] started total={total_secs}s");
                shared.lock().unwrap().is_running = true;
                let _ = app.emit("timer:started", serde_json::json!({ "total_secs": total_secs }));
                if let Some(ws) = app.try_state::<Arc<WsState>>() {
                    websocket::broadcast_started(&ws, total_secs);
                }
                tray::update_menu_items(&tray, true, false);
            }

            TimerEvent::Tick { elapsed_secs, total_secs } => {
                {
                    let mut s = shared.lock().unwrap();
                    s.elapsed_secs = elapsed_secs;
                    s.is_running = true;
                }
                let _ = app.emit(
                    "timer:tick",
                    serde_json::json!({ "elapsed_secs": elapsed_secs, "total_secs": total_secs }),
                );

                // --- Session recording: only for Work rounds, start at MIN_RECORD_SECS (2 mins) ---
                let is_work = sequence.lock().unwrap().current_round == RoundType::Work;
                if is_work {
                    if elapsed_secs == MIN_RECORD_SECS && current_session_id.is_none() {
                        let total = {
                            let seq = sequence.lock().unwrap();
                            let s = settings.lock().unwrap();
                            seq.current_duration_secs(&s)
                        };
                        let task = current_task.lock().unwrap().clone();
                        if let Ok(conn) = db.lock() {
                            match queries::insert_session(&conn, "work", elapsed_secs, total, &task) {
                                Ok(id) => current_session_id = Some(id),
                                Err(e) => log::error!("[timer] failed to record session: {e}"),
                            }
                        }
                    } else if elapsed_secs > MIN_RECORD_SECS && elapsed_secs % 15 == 0 {
                        // Persist progress every 15s to withstand sudden app closes or power loss.
                        if let Some(session_id) = current_session_id {
                            if let Ok(conn) = db.lock() {
                                let _ = queries::update_session_progress(&conn, session_id, elapsed_secs);
                            }
                        }
                    }
                }

                // --- Tick sound ---
                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                if let Some(audio) = app.try_state::<Arc<AudioManager>>() {
                    if audio.tick_enabled_for(&rt) {
                        audio.play_cue(AudioCue::Tick);
                    }
                }

                // Update tray arc — throttle to 1% visual change.
                let progress = if total_secs > 0 {
                    elapsed_secs as f32 / total_secs as f32
                } else {
                    0.0
                };
                if (progress - last_tray_progress).abs() >= 0.01 {
                    tray::update_icon(&tray, &rt, false, progress);
                    last_tray_progress = progress;
                }
            }

            TimerEvent::Complete { skipped: was_skipped } => {
                let completed_round = sequence.lock().unwrap().current_round;
                log::info!(
                    "[timer] round complete type={} skipped={was_skipped}",
                    completed_round.as_str()
                );

                // --- Session recording: complete work session if it reached >= 2 mins ---
                if completed_round == RoundType::Work {
                    let total = {
                        let seq = sequence.lock().unwrap();
                        let s = settings.lock().unwrap();
                        seq.current_duration_secs(&s)
                    };
                    if !was_skipped {
                        if let Some(session_id) = current_session_id.take() {
                            if let Ok(conn) = db.lock() {
                                let _ = queries::complete_session(&conn, session_id, total, true);
                            }
                        } else if total >= MIN_RECORD_SECS {
                            let task = current_task.lock().unwrap().clone();
                            if let Ok(conn) = db.lock() {
                                if let Ok(id) = queries::insert_session(&conn, "work", total, total, &task) {
                                    let _ = queries::complete_session(&conn, id, total, true);
                                }
                            }
                        }
                    } else {
                        let elapsed = shared.lock().unwrap().elapsed_secs;
                        if let Some(session_id) = current_session_id.take() {
                            if let Ok(conn) = db.lock() {
                                let _ = queries::complete_session(&conn, session_id, elapsed, false);
                            }
                        }
                    }
                }
                current_session_id = None;

                // Advance sequence.
                let (next_round, next_duration, auto_start_work, auto_start_break) = {
                    let mut seq = sequence.lock().unwrap();
                    let s = settings.lock().unwrap();
                    seq.work_rounds_total = s.long_break_interval;
                    let (rt, dur) = seq.advance(&s);
                    (rt, dur, s.auto_start_work, s.auto_start_break)
                };

                // Reset shared state for the new round.
                {
                    let mut s = shared.lock().unwrap();
                    s.elapsed_secs = 0;
                    s.is_running = false;
                }

                // Arm the next round's duration without risking a late
                // reconfigure that kicks a freshly-started timer back to Idle.
                engine.send(TimerCommand::Prime {
                    duration_secs: next_duration,
                });

                // Emit round-change with the new snapshot.
                let snapshot = build_snapshot(&sequence, &settings, &shared, &db, &current_task);
                let _ = app.emit("timer:round-change", &snapshot);

                // Dispatch Telegram notification if enabled and in pomodoro mode.
                let (telegram_enabled, bot_token, chat_id, timer_mode) = {
                    let s = settings.lock().unwrap();
                    (s.telegram_enabled, s.telegram_bot_token.clone(), s.telegram_chat_id.clone(), s.timer_mode.clone())
                };
                if telegram_enabled && !bot_token.is_empty() && !chat_id.is_empty() && timer_mode == "pomodoro" {
                    let task_name = current_task.lock().unwrap().clone();
                    let msg = match completed_round {
                        RoundType::Work => {
                            let break_mins = (next_duration + 30) / 60;
                            format!("🍅 ¡Tiempo de concentración terminado! Tarea: {task_name}. Toca descansar {break_mins} min.")
                        }
                        RoundType::ShortBreak | RoundType::LongBreak => {
                            format!("☕ ¡Descanso terminado! Hora de volver a concentrarse en {task_name}.")
                        }
                    };
                    tokio::spawn(async move {
                        if let Err(e) = crate::telegram::send_telegram_message(&bot_token, &chat_id, &msg).await {
                            log::warn!("[telegram] Error enviando notificación: {e}");
                        }
                    });
                }

                // Audio alert for the new round.
                if let Some(audio) = app.try_state::<Arc<AudioManager>>() {
                    let cue = match next_round {
                        RoundType::Work => AudioCue::WorkAlert,
                        RoundType::ShortBreak => AudioCue::ShortBreakAlert,
                        RoundType::LongBreak => AudioCue::LongBreakAlert,
                    };
                    audio.play_cue(cue);
                }

                // Lower-priority-during-breaks: when always_on_top is on and
                // break_always_on_top is enabled, disable always-on-top for
                // breaks and restore it when work resumes.
                let (always_on_top, break_always_on_top) = {
                    let s = settings.lock().unwrap();
                    (s.always_on_top, s.break_always_on_top)
                };
                if always_on_top {
                    if let Some(window) = app.get_webview_window("main") {
                        let is_break = next_round != RoundType::Work;
                        let _ = window.set_always_on_top(!(break_always_on_top && is_break));
                    }
                }

                // Update tray to reflect new round type and reset progress.
                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                tray::update_icon(&tray, &rt, false, 0.0);
                last_tray_progress = -1.0;

                // Broadcast round-change to any connected WebSocket clients.
                if let Some(ws) = app.try_state::<Arc<WsState>>() {
                    websocket::broadcast_round_change(&ws, snapshot);
                }

                // Auto-start if configured.
                let should_auto = match next_round {
                    RoundType::Work => auto_start_work,
                    _ => auto_start_break,
                };
                if should_auto {
                    log::debug!("[timer] auto-starting {}", next_round.as_str());
                    engine.send(TimerCommand::Start);
                } else {
                    // Timer is idle waiting for the user to start the new round.
                    // Reset the tray menu to "Start" so it doesn't keep showing
                    // "Pause" from the round that just completed.
                    tray::update_menu_items(&tray, false, false);
                }
            }

            TimerEvent::Paused { elapsed_secs } => {
                log::info!("[timer] paused elapsed={elapsed_secs}s");
                shared.lock().unwrap().is_running = false;

                if let Some(session_id) = current_session_id {
                    if let Ok(conn) = db.lock() {
                        let _ = queries::update_session_progress(&conn, session_id, elapsed_secs);
                    }
                }

                let _ = app.emit("timer:paused", serde_json::json!({ "elapsed_secs": elapsed_secs }));
                if let Some(ws) = app.try_state::<Arc<WsState>>() {
                    websocket::broadcast_paused(&ws, elapsed_secs);
                }

                // Show pause bars in tray.
                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                let total = {
                    let seq = sequence.lock().unwrap();
                    let s = settings.lock().unwrap();
                    seq.current_duration_secs(&s)
                };
                let progress = if total > 0 { elapsed_secs as f32 / total as f32 } else { 0.0 };
                tray::update_icon(&tray, &rt, true, progress);
                tray::update_menu_items(&tray, false, true);
            }

            TimerEvent::Resumed { elapsed_secs } => {
                log::info!("[timer] resumed elapsed={elapsed_secs}s");
                shared.lock().unwrap().is_running = true;
                let _ = app.emit("timer:resumed", serde_json::json!({ "elapsed_secs": elapsed_secs }));
                if let Some(ws) = app.try_state::<Arc<WsState>>() {
                    websocket::broadcast_resumed(&ws, elapsed_secs);
                }

                // Restore arc in tray.
                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                let total = {
                    let seq = sequence.lock().unwrap();
                    let s = settings.lock().unwrap();
                    seq.current_duration_secs(&s)
                };
                let progress = if total > 0 { elapsed_secs as f32 / total as f32 } else { 0.0 };
                tray::update_icon(&tray, &rt, false, progress);
                last_tray_progress = progress;
                tray::update_menu_items(&tray, true, false);
            }

            TimerEvent::Reset => {
                log::debug!("[timer] reset / idle");
                // Save current work session if it reached the minimum threshold.
                let is_work = sequence.lock().unwrap().current_round == RoundType::Work;
                let elapsed = shared.lock().unwrap().elapsed_secs;
                if is_work {
                    if let Some(session_id) = current_session_id.take() {
                        if let Ok(conn) = db.lock() {
                            let _ = queries::complete_session(&conn, session_id, elapsed, false);
                        }
                    }
                }
                current_session_id = None;

                {
                    let mut s = shared.lock().unwrap();
                    s.elapsed_secs = 0;
                    s.is_running = false;
                }
                let snapshot = build_snapshot(&sequence, &settings, &shared, &db, &current_task);
                let _ = app.emit("timer:reset", &snapshot);
                if let Some(ws) = app.try_state::<Arc<WsState>>() {
                    websocket::broadcast_reset(&ws);
                }

                let duration = {
                    let seq = sequence.lock().unwrap();
                    let s = settings.lock().unwrap();
                    seq.current_duration_secs(&s)
                };
                engine.send(TimerCommand::Prime { duration_secs: duration });

                // Reset tray to idle (empty arc).
                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                tray::update_icon(&tray, &rt, false, 0.0);
                last_tray_progress = -1.0;
                tray::update_menu_items(&tray, false, false);
            }

            TimerEvent::Suspended { elapsed_secs } => {
                log::info!("[timer] suspended by system elapsed={elapsed_secs}s");
                shared.lock().unwrap().is_running = false;
                let _ = app.emit(
                    "timer:suspended",
                    serde_json::json!({ "elapsed_secs": elapsed_secs }),
                );

                // Show pause bars while suspended.
                let rt = sequence.lock().unwrap().current_round.as_str().to_string();
                let total = {
                    let seq = sequence.lock().unwrap();
                    let s = settings.lock().unwrap();
                    seq.current_duration_secs(&s)
                };
                let progress = if total > 0 { elapsed_secs as f32 / total as f32 } else { 0.0 };
                tray::update_icon(&tray, &rt, true, progress);
            }
        }
    }
}

fn build_snapshot(
    sequence: &Arc<Mutex<SequenceState>>,
    settings: &Arc<Mutex<Settings>>,
    shared: &Arc<Mutex<TimerShared>>,
    db: &DbState,
    current_task: &Arc<Mutex<String>>,
) -> TimerSnapshot {
    let seq = sequence.lock().unwrap();
    let s = settings.lock().unwrap();
    let sh = shared.lock().unwrap();
    let task = current_task.lock().unwrap().clone();

    let today_focus_secs = if let Ok(conn) = db.lock() {
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
