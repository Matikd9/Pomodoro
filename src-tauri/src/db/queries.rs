use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Session CRUD (DATA-03)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Session CRUD (DATA-03)
// ---------------------------------------------------------------------------

/// Inserts a new session row when a round begins (or reaches threshold).
/// Returns the row ID so it can be passed to `complete_session` later.
pub fn insert_session(
    conn: &Connection,
    round_type: &str,
    duration_secs: u32,
    target_secs: u32,
    task_name: &str,
) -> Result<i64> {
    let started_at = unix_now();
    let task = if task_name.trim().is_empty() {
        "General"
    } else {
        task_name.trim()
    };
    conn.execute(
        "INSERT INTO sessions (started_at, round_type, duration_secs, target_secs, completed, task_name)
         VALUES (?1, ?2, ?3, ?4, 0, ?5)",
        params![started_at, round_type, duration_secs, target_secs, task],
    )?;
    let id = conn.last_insert_rowid();
    log::debug!("[db] session started: id={id} type={round_type} duration={duration_secs}s target={target_secs}s task={task}");
    Ok(id)
}

/// Updates session progress (elapsed seconds) while the round is running.
pub fn update_session_progress(
    conn: &Connection,
    session_id: i64,
    duration_secs: u32,
) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET duration_secs = ?1 WHERE id = ?2",
        params![duration_secs, session_id],
    )?;
    Ok(())
}

/// Updates a session when the round ends (by completion, skip, or reset).
pub fn complete_session(
    conn: &Connection,
    session_id: i64,
    duration_secs: u32,
    completed: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET ended_at = ?1, duration_secs = ?2, completed = ?3 WHERE id = ?4",
        params![unix_now(), duration_secs, completed as i64, session_id],
    )?;
    log::debug!("[db] session ended: id={session_id} duration={duration_secs}s completed={completed}");
    Ok(())
}

/// Deletes a session if it was canceled before the threshold (e.g., < 2 mins).
pub fn discard_session(
    conn: &Connection,
    session_id: i64,
) -> Result<()> {
    conn.execute("DELETE FROM sessions WHERE id = ?1", params![session_id])?;
    log::debug!("[db] session discarded (< 2 mins): id={session_id}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Stats queries
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct SessionStats {
    pub total_work_sessions: i64,
    /// Total rounds including portions (e.g., 3.5 rounds).
    pub completed_work_sessions: f32,
    /// Sum of duration_secs for all recorded work sessions >= 120s.
    pub total_work_secs: i64,
}

pub fn get_all_time_stats(conn: &Connection) -> Result<SessionStats> {
    let total_work_sessions: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE round_type = 'work' AND duration_secs >= 120",
        [],
        |r| r.get(0),
    )?;

    let completed_work_sessions: f64 = conn.query_row(
        "SELECT COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0)
         FROM sessions WHERE round_type = 'work' AND duration_secs >= 120",
        [],
        |r| r.get(0),
    )?;

    let total_work_secs: i64 = conn.query_row(
        "SELECT COALESCE(SUM(duration_secs), 0)
         FROM sessions WHERE round_type = 'work' AND duration_secs >= 120",
        [],
        |r| r.get(0),
    )?;

    Ok(SessionStats {
        total_work_sessions,
        completed_work_sessions: completed_work_sessions as f32,
        total_work_secs,
    })
}

// ---------------------------------------------------------------------------
// Detailed stats queries (DATA-04)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TaskStat {
    pub task_name: String,
    pub focus_secs: u32,
    pub rounds: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DailyStats {
    /// Total work rounds today (including fractions, e.g., 2.5).
    pub rounds: f32,
    pub focus_mins: u32,
    /// None when no work sessions were started today (avoids 0/0).
    pub completion_rate: Option<f32>,
    /// Work rounds (including fractions) per hour of the day (index 0 = midnight).
    pub by_hour: Vec<f32>,
    /// Breakdown of focus time and rounds by task for today.
    pub task_breakdown: Vec<TaskStat>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct DayStat {
    /// Local calendar date in "YYYY-MM-DD" format.
    pub date: String,
    pub rounds: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct CalendarWeekStats {
    pub start_date: String,
    pub end_date: String,
    pub iso_year: i32,
    pub iso_week: u32,
    pub days: Vec<DayStat>,
    pub tasks: Vec<TaskStat>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct HeatmapEntry {
    /// Local calendar date in "YYYY-MM-DD" format.
    pub date: String,
    pub count: f32,
    pub focus_secs: u32,
    pub hours: f32,
}

#[derive(Debug, Serialize)]
pub struct StreakInfo {
    pub current: u32,
    pub longest: u32,
}

/// Work rounds (including fractions) and focus time for a specific date (YYYY-MM-DD).
pub fn get_daily_stats_by_date(conn: &Connection, target_date: &str) -> Result<DailyStats> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') = ?1",
        [target_date],
        |r| r.get(0),
    )?;

    let completed_full: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions
         WHERE round_type = 'work' AND completed = 1 AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') = ?1",
        [target_date],
        |r| r.get(0),
    )?;

    let rounds: f64 = conn.query_row(
        "SELECT COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0)
         FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') = ?1",
        [target_date],
        |r| r.get(0),
    )?;

    let focus_secs: i64 = conn.query_row(
        "SELECT COALESCE(SUM(duration_secs), 0) FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') = ?1",
        [target_date],
        |r| r.get(0),
    )?;

    let mut by_hour = vec![0.0f32; 24];
    let mut stmt = conn.prepare(
        "SELECT CAST(strftime('%H', datetime(started_at, 'unixepoch', 'localtime')) AS INTEGER) as h,
                SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)) as cnt
         FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') = ?1
         GROUP BY h",
    )?;
    let rows = stmt.query_map([target_date], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?;
    for row in rows.flatten() {
        let (h, cnt) = row;
        if (0..24).contains(&h) {
            by_hour[h as usize] = cnt as f32;
        }
    }

    let task_breakdown = get_daily_task_breakdown(conn, target_date)?;

    Ok(DailyStats {
        rounds: rounds as f32,
        focus_mins: ((focus_secs + 30) / 60) as u32,
        completion_rate: if total > 0 { Some(completed_full as f32 / total as f32) } else { None },
        by_hour,
        task_breakdown,
    })
}

/// Work rounds (including fractions) and focus time for today (local calendar date).
pub fn get_daily_stats(conn: &Connection) -> Result<DailyStats> {
    let today: String = conn.query_row(
        "SELECT date('now', 'localtime')",
        [],
        |r| r.get(0),
    )?;
    get_daily_stats_by_date(conn, &today)
}

/// Completed work rounds and portions per local calendar day for the last 7 days.
pub fn get_weekly_stats(conn: &Connection) -> Result<Vec<DayStat>> {
    let mut stmt = conn.prepare(
        "SELECT date(started_at, 'unixepoch', 'localtime') as day,
                COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0) as rounds
         FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') >= date('now', 'localtime', '-6 days')
         GROUP BY day
         ORDER BY day",
    )?;
    let rows = stmt.query_map([], |r| Ok(DayStat { date: r.get(0)?, rounds: r.get::<_, f64>(1)? as f32 }))?
        .collect();
    rows
}

/// Work rounds and portions per local calendar day, all time (no date limit).
pub fn get_heatmap_data(conn: &Connection) -> Result<Vec<HeatmapEntry>> {
    let mut stmt = conn.prepare(
        "SELECT date(started_at, 'unixepoch', 'localtime') as day,
                COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0) as cnt,
                COALESCE(SUM(duration_secs), 0) as secs
         FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         GROUP BY day
         ORDER BY day",
    )?;
    let rows = stmt
        .query_map([], |r| {
            let secs: i64 = r.get(2)?;
            let focus_secs = secs as u32;
            let hours = focus_secs as f32 / 3600.0;
            Ok(HeatmapEntry {
                date: r.get(0)?,
                count: r.get::<_, f64>(1)? as f32,
                focus_secs,
                hours,
            })
        })?
        .collect();
    rows
}

/// Current and longest work-session streaks (consecutive local calendar days).
/// A streak stays active until midnight: if yesterday had sessions but today does not,
/// the streak is still counted as current.
pub fn get_streak(conn: &Connection) -> Result<StreakInfo> {
    let today: String = conn.query_row(
        "SELECT date('now', 'localtime')",
        [],
        |r| r.get(0),
    )?;

    let mut stmt = conn.prepare(
        "SELECT date(started_at, 'unixepoch', 'localtime') as day
         FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         GROUP BY day
         ORDER BY day",
    )?;
    let days: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .flatten()
        .collect();

    Ok(compute_streak(&days, &today))
}

/// Returns the total work focus seconds accumulated today.
pub fn get_today_focus_secs(conn: &Connection) -> Result<u32> {
    let today: String = conn.query_row(
        "SELECT date('now', 'localtime')",
        [],
        |r| r.get(0),
    )?;
    let secs: i64 = conn.query_row(
        "SELECT COALESCE(SUM(duration_secs), 0)
         FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') = ?1",
        [&today],
        |r| r.get(0),
    )?;
    Ok(secs as u32)
}

/// Returns the task breakdown (focus seconds and rounds) for a specific local date.
pub fn get_daily_task_breakdown(conn: &Connection, date: &str) -> Result<Vec<TaskStat>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(NULLIF(task_name, ''), 'General') as task,
                COALESCE(SUM(duration_secs), 0) as total_secs,
                COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0) as total_rounds
         FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') = ?1
         GROUP BY task
         ORDER BY total_secs DESC",
    )?;
    let rows = stmt.query_map([date], |r| {
        Ok(TaskStat {
            task_name: r.get(0)?,
            focus_secs: r.get::<_, i64>(1)? as u32,
            rounds: r.get::<_, f64>(2)? as f32,
        })
    })?;
    let mut list = Vec::new();
    for item in rows.flatten() {
        list.push(item);
    }
    Ok(list)
}

/// Returns the task breakdown (focus seconds and rounds) for a date range [start_date, end_date].
pub fn get_weekly_task_breakdown_between(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<TaskStat>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(NULLIF(task_name, ''), 'General') as task,
                COALESCE(SUM(duration_secs), 0) as total_secs,
                COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0) as total_rounds
         FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
           AND date(started_at, 'unixepoch', 'localtime') >= ?1
           AND date(started_at, 'unixepoch', 'localtime') <= ?2
         GROUP BY task
         ORDER BY total_secs DESC",
    )?;
    let rows = stmt.query_map([start_date, end_date], |r| {
        Ok(TaskStat {
            task_name: r.get(0)?,
            focus_secs: r.get::<_, i64>(1)? as u32,
            rounds: r.get::<_, f64>(2)? as f32,
        })
    })?;
    let mut list = Vec::new();
    for item in rows.flatten() {
        list.push(item);
    }
    Ok(list)
}

/// Returns the 7 calendar days (Monday to Sunday) and task breakdown for a week offset.
/// offset 0 = current week, -1 = previous week, etc.
pub fn get_calendar_week_stats(conn: &Connection, week_offset: i32) -> Result<CalendarWeekStats> {
    let day_modifier = format!("{} days", week_offset * 7);

    let (start_date, end_date): (String, String) = conn.query_row(
        "SELECT date('now', 'localtime', ?1, 'weekday 0', '-6 days'),
                date('now', 'localtime', ?1, 'weekday 0')",
        [&day_modifier],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let (iso_year, iso_week): (i32, u32) = conn.query_row(
        "SELECT CAST(strftime('%Y', date(?1, '+3 days')) AS INTEGER),
                CAST((strftime('%j', date(?1, '+3 days')) - 1) / 7 + 1 AS INTEGER)",
        [&start_date],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    let mut days = Vec::with_capacity(7);
    for i in 0..7 {
        let day_date: String = conn.query_row(
            "SELECT date(?1, ?2)",
            [&start_date, &format!("+{i} days")],
            |r| r.get(0),
        )?;
        let rounds: f64 = conn
            .query_row(
                "SELECT COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0)
                 FROM sessions
                 WHERE round_type = 'work' AND duration_secs >= 120
                   AND date(started_at, 'unixepoch', 'localtime') = ?1",
                [&day_date],
                |r| r.get(0),
            )
            .unwrap_or(0.0);
        days.push(DayStat {
            date: day_date,
            rounds: rounds as f32,
        });
    }

    let tasks = get_weekly_task_breakdown_between(conn, &start_date, &end_date)?;

    Ok(CalendarWeekStats {
        start_date,
        end_date,
        iso_year,
        iso_week,
        days,
        tasks,
    })
}

/// Returns the task breakdown (focus seconds and rounds) for the last 7 days.
pub fn get_weekly_task_breakdown(conn: &Connection) -> Result<Vec<TaskStat>> {
    let mut stmt = conn.prepare(
        "SELECT COALESCE(NULLIF(task_name, ''), 'General') as task,
                COALESCE(SUM(duration_secs), 0) as total_secs,
                COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0) as total_rounds
         FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') >= date('now', 'localtime', '-6 days')
         GROUP BY task
         ORDER BY total_secs DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(TaskStat {
            task_name: r.get(0)?,
            focus_secs: r.get::<_, i64>(1)? as u32,
            rounds: r.get::<_, f64>(2)? as f32,
        })
    })?;
    let mut list = Vec::new();
    for item in rows.flatten() {
        list.push(item);
    }
    Ok(list)
}

#[derive(Debug, Serialize, serde::Deserialize, Clone, PartialEq)]
pub struct TaskItem {
    pub name: String,
    pub completed: bool,
    #[serde(default)]
    pub deleted: bool,
}

#[derive(Debug, Serialize, serde::Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TaskStatsSummary {
    pub name: String,
    pub completed: bool,
    pub deleted: bool,
    #[serde(alias = "allTimeSecs")]
    pub all_time_secs: u32,
    #[serde(alias = "allTimeRounds")]
    pub all_time_rounds: f32,
    #[serde(alias = "monthSecs")]
    pub month_secs: u32,
    #[serde(alias = "monthRounds")]
    pub month_rounds: f32,
    #[serde(alias = "weekSecs")]
    pub week_secs: u32,
    #[serde(alias = "weekRounds")]
    pub week_rounds: f32,
    #[serde(alias = "todaySecs")]
    pub today_secs: u32,
    #[serde(alias = "todayRounds")]
    pub today_rounds: f32,
}

/// Returns the list of active tasks (excluding soft-deleted ones), ensuring pending ones come first ('General' always at top),
/// followed by completed ones.
pub fn get_tasks(conn: &Connection) -> Result<Vec<TaskItem>> {
    let mut stmt = conn.prepare(
        "SELECT name, completed, deleted FROM tasks
         WHERE deleted = 0
         ORDER BY completed ASC,
                  CASE WHEN name = 'General' THEN 0 ELSE 1 END,
                  name COLLATE NOCASE ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(TaskItem {
            name: r.get(0)?,
            completed: r.get::<_, i64>(1)? == 1,
            deleted: r.get::<_, i64>(2)? == 1,
        })
    })?;
    let mut tasks = Vec::new();
    for t in rows.flatten() {
        tasks.push(t);
    }
    if !tasks.iter().any(|t| t.name == "General") {
        tasks.insert(0, TaskItem { name: "General".to_string(), completed: false, deleted: false });
    }
    Ok(tasks)
}

/// Returns the comprehensive summary of all tasks (active, completed, deleted) with focus times and rounds
/// across all-time, this month, this week, and today.
pub fn get_tasks_summary(conn: &Connection) -> Result<Vec<TaskStatsSummary>> {
    let mut stmt = conn.prepare(
        "SELECT 
            t.name,
            t.completed,
            t.deleted,
            COALESCE(SUM(s.duration_secs), 0) AS all_time_secs,
            COALESCE(SUM(CAST(s.duration_secs AS REAL) / CAST(CASE WHEN s.target_secs > 0 THEN s.target_secs ELSE 1500 END AS REAL)), 0.0) AS all_time_rounds,
            COALESCE(SUM(CASE WHEN strftime('%Y-%m', datetime(s.started_at, 'unixepoch', 'localtime')) = strftime('%Y-%m', 'now', 'localtime') THEN s.duration_secs ELSE 0 END), 0) AS month_secs,
            COALESCE(SUM(CASE WHEN strftime('%Y-%m', datetime(s.started_at, 'unixepoch', 'localtime')) = strftime('%Y-%m', 'now', 'localtime') THEN CAST(s.duration_secs AS REAL) / CAST(CASE WHEN s.target_secs > 0 THEN s.target_secs ELSE 1500 END AS REAL) ELSE 0.0 END), 0.0) AS month_rounds,
            COALESCE(SUM(CASE WHEN date(s.started_at, 'unixepoch', 'localtime') >= date('now', 'localtime', 'weekday 0', '-6 days') AND date(s.started_at, 'unixepoch', 'localtime') <= date('now', 'localtime', 'weekday 0') THEN s.duration_secs ELSE 0 END), 0) AS week_secs,
            COALESCE(SUM(CASE WHEN date(s.started_at, 'unixepoch', 'localtime') >= date('now', 'localtime', 'weekday 0', '-6 days') AND date(s.started_at, 'unixepoch', 'localtime') <= date('now', 'localtime', 'weekday 0') THEN CAST(s.duration_secs AS REAL) / CAST(CASE WHEN s.target_secs > 0 THEN s.target_secs ELSE 1500 END AS REAL) ELSE 0.0 END), 0.0) AS week_rounds,
            COALESCE(SUM(CASE WHEN date(s.started_at, 'unixepoch', 'localtime') = date('now', 'localtime') THEN s.duration_secs ELSE 0 END), 0) AS today_secs,
            COALESCE(SUM(CASE WHEN date(s.started_at, 'unixepoch', 'localtime') = date('now', 'localtime') THEN CAST(s.duration_secs AS REAL) / CAST(CASE WHEN s.target_secs > 0 THEN s.target_secs ELSE 1500 END AS REAL) ELSE 0.0 END), 0.0) AS today_rounds
         FROM (
             SELECT name, completed, deleted FROM tasks
             UNION
             SELECT DISTINCT task_name AS name, 0 AS completed, 0 AS deleted
             FROM sessions
             WHERE task_name != '' AND task_name NOT IN (SELECT name FROM tasks)
         ) t
         LEFT JOIN sessions s ON (s.task_name = t.name COLLATE NOCASE AND s.round_type = 'work' AND s.duration_secs >= 120)
         GROUP BY t.name
         ORDER BY 
            t.deleted ASC,
            t.completed ASC,
            CASE WHEN t.name = 'General' THEN 0 ELSE 1 END,
            all_time_secs DESC,
            t.name COLLATE NOCASE ASC",
    )?;

    let rows = stmt.query_map([], |r| {
        Ok(TaskStatsSummary {
            name: r.get(0)?,
            completed: r.get::<_, i64>(1)? == 1,
            deleted: r.get::<_, i64>(2)? == 1,
            all_time_secs: r.get::<_, i64>(3)? as u32,
            all_time_rounds: r.get::<_, f64>(4)? as f32,
            month_secs: r.get::<_, i64>(5)? as u32,
            month_rounds: r.get::<_, f64>(6)? as f32,
            week_secs: r.get::<_, i64>(7)? as u32,
            week_rounds: r.get::<_, f64>(8)? as f32,
            today_secs: r.get::<_, i64>(9)? as u32,
            today_rounds: r.get::<_, f64>(10)? as f32,
        })
    })?;

    let mut list = Vec::new();
    for t in rows.flatten() {
        list.push(t);
    }
    if !list.iter().any(|t| t.name == "General") {
        list.insert(
            0,
            TaskStatsSummary {
                name: "General".to_string(),
                completed: false,
                deleted: false,
                all_time_secs: 0,
                all_time_rounds: 0.0,
                month_secs: 0,
                month_rounds: 0.0,
                week_secs: 0,
                week_rounds: 0.0,
                today_secs: 0,
                today_rounds: 0.0,
            },
        );
    }
    Ok(list)
}

/// Creates a new task or unarchives an existing one, and returns the updated task list.
pub fn create_task(conn: &Connection, name: &str) -> Result<Vec<TaskItem>> {
    let clean = name.trim();
    if !clean.is_empty() {
        conn.execute(
            "INSERT INTO tasks (name, created_at, completed, deleted) VALUES (?1, CAST(strftime('%s', 'now') AS INTEGER), 0, 0)
             ON CONFLICT(name) DO UPDATE SET completed = 0, completed_at = NULL, deleted = 0, deleted_at = NULL",
            params![clean],
        )?;
    }
    get_tasks(conn)
}

/// Renames an existing task, simultaneously updating all historical sessions and saved settings
/// to preserve total accumulated focus hours under the new name.
pub fn rename_task(conn: &Connection, old_name: &str, new_name: &str) -> Result<()> {
    let old_clean = old_name.trim();
    let new_clean = new_name.trim();
    if old_clean.is_empty()
        || new_clean.is_empty()
        || old_clean == "General"
        || new_clean == "General"
        || old_clean == new_clean
    {
        return Ok(());
    }

    conn.execute_batch("BEGIN TRANSACTION;")?;
    let rename_result: Result<()> = (|| {
        // Update task row
        conn.execute(
            "UPDATE tasks SET name = ?1 WHERE name = ?2 COLLATE NOCASE",
            params![new_clean, old_clean],
        )?;
        // Update historical sessions to keep accumulated hours
        conn.execute(
            "UPDATE sessions SET task_name = ?1 WHERE task_name = ?2 COLLATE NOCASE",
            params![new_clean, old_clean],
        )?;
        // Update setting if it was the selected task
        conn.execute(
            "UPDATE settings SET value = ?1 WHERE key = 'last_task_name' AND value = ?2 COLLATE NOCASE",
            params![new_clean, old_clean],
        )?;
        Ok(())
    })();

    match rename_result {
        Ok(()) => {
            conn.execute_batch("COMMIT;")?;
            Ok(())
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK;");
            Err(e)
        }
    }
}

/// Marks a task as completed or restores it to active. 'General' cannot be completed.
pub fn toggle_task_complete(conn: &Connection, name: &str, completed: bool) -> Result<Vec<TaskItem>> {
    let clean = name.trim();
    if clean != "General" && !clean.is_empty() {
        conn.execute(
            "UPDATE tasks SET completed = ?1, completed_at = CASE WHEN ?1 = 1 THEN CAST(strftime('%s', 'now') AS INTEGER) ELSE NULL END WHERE name = ?2",
            params![if completed { 1 } else { 0 }, clean],
        )?;
    }
    get_tasks(conn)
}

/// Soft-deletes a task from active view and moves it to deleted tasks section. 'General' cannot be deleted.
pub fn delete_task(conn: &Connection, name: &str) -> Result<Vec<TaskItem>> {
    let clean = name.trim();
    if clean != "General" && !clean.is_empty() {
        conn.execute(
            "UPDATE tasks SET deleted = 1, deleted_at = CAST(strftime('%s', 'now') AS INTEGER) WHERE name = ?1",
            params![clean],
        )?;
    }
    get_tasks(conn)
}

/// Restores a soft-deleted task back to active.
pub fn restore_task(conn: &Connection, name: &str) -> Result<Vec<TaskItem>> {
    let clean = name.trim();
    if !clean.is_empty() {
        conn.execute(
            "UPDATE tasks SET deleted = 0, deleted_at = NULL WHERE name = ?1",
            params![clean],
        )?;
    }
    get_tasks(conn)
}

// ---------------------------------------------------------------------------
// Streak helpers
// ---------------------------------------------------------------------------

/// Convert a "YYYY-MM-DD" string to a day number for arithmetic comparison.
/// Uses the proleptic Gregorian calendar; absolute value is arbitrary — only
/// differences between dates matter.
fn date_to_day_num(s: &str) -> Option<i32> {
    let mut parts = s.splitn(3, '-');
    let y: i32 = parts.next()?.parse().ok()?;
    let m: i32 = parts.next()?.parse().ok()?;
    let d: i32 = parts.next()?.parse().ok()?;
    let y = if m <= 2 { y - 1 } else { y };
    let m = if m <= 2 { m + 12 } else { m };
    Some(y * 365 + y / 4 - y / 100 + y / 400 + (153 * m - 457) / 5 + d)
}

pub fn compute_streak(days: &[String], today: &str) -> StreakInfo {
    let nums: Vec<i32> = days.iter().filter_map(|s| date_to_day_num(s)).collect();
    if nums.is_empty() {
        return StreakInfo { current: 0, longest: 0 };
    }

    let today_n = match date_to_day_num(today) {
        Some(n) => n,
        None => return StreakInfo { current: 0, longest: 0 },
    };

    // Current streak — alive if most recent session day is today or yesterday.
    let last = *nums.last().unwrap();
    let current = if last == today_n || last == today_n - 1 {
        let mut count = 0u32;
        let mut expected = last;
        for &n in nums.iter().rev() {
            if n == expected {
                count += 1;
                expected -= 1;
            } else {
                break;
            }
        }
        count
    } else {
        0
    };

    // Longest streak.
    let mut longest = 1u32;
    let mut run = 1u32;
    for i in 1..nums.len() {
        if nums[i] == nums[i - 1] + 1 {
            run += 1;
            if run > longest { longest = run; }
        } else {
            run = 1;
        }
    }

    StreakInfo { current, longest }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn insert_and_complete_session() {
        let conn = setup();
        let id = insert_session(&conn, "work", 1500, 1500, "General").unwrap();
        assert!(id > 0);

        complete_session(&conn, id, 1500, true).unwrap();

        let completed: i64 = conn
            .query_row(
                "SELECT completed FROM sessions WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(completed, 1);
    }

    #[test]
    fn stats_empty_db() {
        let conn = setup();
        let stats = get_all_time_stats(&conn).unwrap();
        assert_eq!(stats.total_work_sessions, 0);
        assert_eq!(stats.completed_work_sessions, 0.0);
        assert_eq!(stats.total_work_secs, 0);
    }

    #[test]
    fn compute_streak_empty() {
        let info = compute_streak(&[], "2024-03-15");
        assert_eq!(info.current, 0);
        assert_eq!(info.longest, 0);
    }

    #[test]
    fn compute_streak_active_today() {
        let days = vec!["2024-03-13".to_string(), "2024-03-14".to_string(), "2024-03-15".to_string()];
        let info = compute_streak(&days, "2024-03-15");
        assert_eq!(info.current, 3);
        assert_eq!(info.longest, 3);
    }

    #[test]
    fn compute_streak_active_until_midnight() {
        // Yesterday had sessions, today does not — streak still live.
        let days = vec!["2024-03-13".to_string(), "2024-03-14".to_string()];
        let info = compute_streak(&days, "2024-03-15");
        assert_eq!(info.current, 2);
    }

    #[test]
    fn compute_streak_broken() {
        // Last session was 2 days ago — streak is broken.
        let days = vec!["2024-03-12".to_string(), "2024-03-13".to_string()];
        let info = compute_streak(&days, "2024-03-15");
        assert_eq!(info.current, 0);
    }

    #[test]
    fn compute_streak_longest_across_break() {
        let days = vec![
            "2024-03-01".to_string(), "2024-03-02".to_string(), "2024-03-03".to_string(),
            "2024-03-10".to_string(), "2024-03-11".to_string(),
        ];
        let info = compute_streak(&days, "2024-03-11");
        assert_eq!(info.current, 2);
        assert_eq!(info.longest, 3);
    }

    #[test]
    fn get_daily_stats_empty() {
        let conn = setup();
        let stats = get_daily_stats(&conn).unwrap();
        assert_eq!(stats.rounds, 0.0);
        assert_eq!(stats.focus_mins, 0);
        assert!(stats.completion_rate.is_none());
        assert_eq!(stats.by_hour.len(), 24);
    }

    #[test]
    fn get_weekly_stats_empty() {
        let conn = setup();
        let stats = get_weekly_stats(&conn).unwrap();
        assert!(stats.is_empty());
    }

    #[test]
    fn get_heatmap_data_empty() {
        let conn = setup();
        let entries = get_heatmap_data(&conn).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn get_heatmap_data_with_sessions() {
        let conn = setup();
        let id = insert_session(&conn, "work", 3600, 1500, "Math").unwrap();
        complete_session(&conn, id, 3600, true).unwrap();
        let entries = get_heatmap_data(&conn).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].focus_secs, 3600);
        assert!((entries[0].hours - 1.0).abs() < f32::EPSILON);
        assert!((entries[0].count - 2.4).abs() < 0.01);
    }

    #[test]
    fn focus_mins_rounds_to_nearest_minute() {
        let conn = setup();

        // 339 s = 5:39 → rounds up to 6 min (remainder 39 ≥ 30).
        let id1 = insert_session(&conn, "work", 339, 1500, "General").unwrap();
        complete_session(&conn, id1, 339, true).unwrap();
        let stats = get_daily_stats(&conn).unwrap();
        assert_eq!(stats.focus_mins, 6, "339 s should round to 6 min");

        // Reset and test round-down: 324 s = 5:24 → rounds down to 5 min (remainder 24 < 30).
        let conn2 = setup();
        let id2 = insert_session(&conn2, "work", 324, 1500, "General").unwrap();
        complete_session(&conn2, id2, 324, true).unwrap();
        let stats2 = get_daily_stats(&conn2).unwrap();
        assert_eq!(stats2.focus_mins, 5, "324 s should round to 5 min");

        // Exact minute boundary: 1500 s = 25:00 → stays 25 min.
        let conn3 = setup();
        let id3 = insert_session(&conn3, "work", 1500, 1500, "General").unwrap();
        complete_session(&conn3, id3, 1500, true).unwrap();
        let stats3 = get_daily_stats(&conn3).unwrap();
        assert_eq!(stats3.focus_mins, 25, "1500 s should be exactly 25 min");
    }

    #[test]
    fn stats_counts_correctly_with_fractions_and_threshold() {
        let conn = setup();

        // Session 1: completed 1500s of 1500s -> 1.0 round
        let id1 = insert_session(&conn, "work", 1500, 1500, "Math").unwrap();
        complete_session(&conn, id1, 1500, true).unwrap();

        // Session 2: partial 750s of 1500s -> 0.5 round (skipped / reset >= 120s)
        let id2 = insert_session(&conn, "work", 750, 1500, "Language").unwrap();
        complete_session(&conn, id2, 750, false).unwrap();

        // Session 3: under threshold (< 120s, e.g. 60s) -> discarded or ignored
        let id3 = insert_session(&conn, "work", 60, 1500, "Math").unwrap();
        complete_session(&conn, id3, 60, false).unwrap();

        // Short break: should not be counted in work stats
        let _id4 = insert_session(&conn, "short-break", 300, 300, "General").unwrap();

        let stats = get_all_time_stats(&conn).unwrap();
        assert_eq!(stats.total_work_sessions, 2, "session < 120s is ignored");
        assert_eq!(stats.completed_work_sessions, 1.5, "1.0 + 0.5 = 1.5 rounds");
        assert_eq!(stats.total_work_secs, 2250, "1500 + 750 = 2250s");

        // Check task breakdown
        let daily = get_daily_stats(&conn).unwrap();
        assert_eq!(daily.task_breakdown.len(), 2);
        assert_eq!(daily.task_breakdown[0].task_name, "Math");
        assert_eq!(daily.task_breakdown[0].focus_secs, 1500);
        assert_eq!(daily.task_breakdown[0].rounds, 1.0);
        assert_eq!(daily.task_breakdown[1].task_name, "Language");
        assert_eq!(daily.task_breakdown[1].focus_secs, 750);
        assert_eq!(daily.task_breakdown[1].rounds, 0.5);
    }

    #[test]
    fn tasks_management_and_defaults() {
        let conn = setup();
        let tasks = get_tasks(&conn).unwrap();
        assert_eq!(tasks, vec![TaskItem { name: "General".to_string(), completed: false, deleted: false }]);

        create_task(&conn, "Math").unwrap();
        create_task(&conn, "Physics").unwrap();
        create_task(&conn, "Math").unwrap(); // Duplicate ignored

        let updated = get_tasks(&conn).unwrap();
        assert_eq!(updated.len(), 3);
        assert_eq!(updated[0].name, "General");
        assert_eq!(updated[1].name, "Math");
        assert_eq!(updated[2].name, "Physics");

        // Complete Math
        let after_complete = toggle_task_complete(&conn, "Math", true).unwrap();
        let math = after_complete.iter().find(|t| t.name == "Math").unwrap();
        assert!(math.completed);

        // Rename Physics -> Chemistry
        rename_task(&conn, "Physics", "Chemistry").unwrap();
        let after_rename = get_tasks(&conn).unwrap();
        assert!(after_rename.iter().any(|t| t.name == "Chemistry"));
        assert!(!after_rename.iter().any(|t| t.name == "Physics"));

        // Delete Math (soft delete)
        let after_delete = delete_task(&conn, "Math").unwrap();
        assert!(!after_delete.iter().any(|t| t.name == "Math"));

        // Restore Math
        let after_restore = restore_task(&conn, "Math").unwrap();
        assert!(after_restore.iter().any(|t| t.name == "Math"));

        // Summary
        let summary = get_tasks_summary(&conn).unwrap();
        assert!(summary.iter().any(|t| t.name == "Math"));
        assert!(summary.iter().any(|t| t.name == "Chemistry"));
        assert!(summary.iter().any(|t| t.name == "General"));
    }

    #[test]
    fn get_daily_stats_by_date_test() {
        let conn = setup();
        let stats = get_daily_stats_by_date(&conn, "2026-09-28").unwrap();
        assert_eq!(stats.rounds, 0.0);
        assert_eq!(stats.focus_mins, 0);
        assert_eq!(stats.by_hour.len(), 24);
    }

    #[test]
    fn get_calendar_week_stats_test() {
        let conn = setup();
        let week = get_calendar_week_stats(&conn, 0).unwrap();
        assert_eq!(week.days.len(), 7);
        assert!(!week.start_date.is_empty());
        assert!(!week.end_date.is_empty());
    }
}
