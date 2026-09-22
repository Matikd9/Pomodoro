use rusqlite::{params, Connection, Result};
use serde::Serialize;

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

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct TaskStat {
    pub task_name: String,
    pub focus_secs: u32,
    pub rounds: f32,
}

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
pub struct DayStat {
    /// Local calendar date in "YYYY-MM-DD" format.
    pub date: String,
    pub rounds: f32,
}

#[derive(Debug, Serialize)]
pub struct HeatmapEntry {
    /// Local calendar date in "YYYY-MM-DD" format.
    pub date: String,
    pub count: f32,
}

#[derive(Debug, Serialize)]
pub struct StreakInfo {
    pub current: u32,
    pub longest: u32,
}

/// Work rounds (including fractions) and focus time for today (local calendar date).
pub fn get_daily_stats(conn: &Connection) -> Result<DailyStats> {
    let today: String = conn.query_row(
        "SELECT date('now', 'localtime')",
        [],
        |r| r.get(0),
    )?;

    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') = ?1",
        [&today],
        |r| r.get(0),
    )?;

    let completed_full: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions
         WHERE round_type = 'work' AND completed = 1 AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') = ?1",
        [&today],
        |r| r.get(0),
    )?;

    let rounds: f64 = conn.query_row(
        "SELECT COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0)
         FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') = ?1",
        [&today],
        |r| r.get(0),
    )?;

    let focus_secs: i64 = conn.query_row(
        "SELECT COALESCE(SUM(duration_secs), 0) FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         AND date(started_at, 'unixepoch', 'localtime') = ?1",
        [&today],
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
    let rows = stmt.query_map([&today], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?;
    for row in rows.flatten() {
        let (h, cnt) = row;
        if (0..24).contains(&h) {
            by_hour[h as usize] = cnt as f32;
        }
    }

    let task_breakdown = get_daily_task_breakdown(conn, &today)?;

    Ok(DailyStats {
        rounds: rounds as f32,
        focus_mins: ((focus_secs + 30) / 60) as u32,
        completion_rate: if total > 0 { Some(completed_full as f32 / total as f32) } else { None },
        by_hour,
        task_breakdown,
    })
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
                COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0) as cnt
         FROM sessions
         WHERE round_type = 'work' AND duration_secs >= 120
         GROUP BY day
         ORDER BY day",
    )?;
    let rows = stmt.query_map([], |r| Ok(HeatmapEntry { date: r.get(0)?, count: r.get::<_, f64>(1)? as f32 }))?
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

/// Returns the list of tasks, ensuring 'General' is first, followed by others alphabetically.
pub fn get_tasks(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM tasks ORDER BY CASE WHEN name = 'General' THEN 0 ELSE 1 END, name COLLATE NOCASE ASC",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    let mut tasks = Vec::new();
    for t in rows.flatten() {
        tasks.push(t);
    }
    if !tasks.iter().any(|t| t == "General") {
        tasks.insert(0, "General".to_string());
    }
    Ok(tasks)
}

/// Creates a new task if it does not already exist, and returns the updated task list.
pub fn create_task(conn: &Connection, name: &str) -> Result<Vec<String>> {
    let clean = name.trim();
    if !clean.is_empty() {
        conn.execute(
            "INSERT OR IGNORE INTO tasks (name, created_at) VALUES (?1, CAST(strftime('%s', 'now') AS INTEGER))",
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
        let id = insert_session(&conn, "work", 1500, 1500).unwrap();
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
        assert_eq!(tasks, vec!["General"]);

        create_task(&conn, "Math").unwrap();
        create_task(&conn, "Physics").unwrap();
        create_task(&conn, "Math").unwrap(); // Duplicate ignored

        let updated = get_tasks(&conn).unwrap();
        assert_eq!(updated, vec!["General", "Math", "Physics"]);
    }
}
