use rusqlite::{Connection, Result};

/// Full schema for version 1. Tables use IF NOT EXISTS so the batch is
/// idempotent, but the schema_version check in `run()` prevents re-execution.
const MIGRATION_1: &str = "
    CREATE TABLE IF NOT EXISTS schema_version (
        version INTEGER NOT NULL
    );

    CREATE TABLE IF NOT EXISTS settings (
        key   TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS sessions (
        id            INTEGER PRIMARY KEY AUTOINCREMENT,
        started_at    INTEGER NOT NULL,
        ended_at      INTEGER,
        round_type    TEXT NOT NULL CHECK(round_type IN ('work', 'short-break', 'long-break')),
        duration_secs INTEGER NOT NULL CHECK(duration_secs > 0),
        completed     INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0, 1))
    );

    CREATE TABLE IF NOT EXISTS custom_themes (
        id     INTEGER PRIMARY KEY AUTOINCREMENT,
        name   TEXT NOT NULL UNIQUE,
        colors TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_sessions_started_at ON sessions(started_at);
    CREATE INDEX IF NOT EXISTS idx_sessions_round_type ON sessions(round_type);

    INSERT INTO schema_version VALUES (1);
";

/// Migrates timer duration storage from minute-resolution keys to second-resolution keys.
/// Reads existing `time_*_mins` rows, multiplies by 60, writes `time_*_secs`, then deletes
/// the old keys so key names align with the Settings struct field names.
const MIGRATION_2: &str = "
    INSERT OR IGNORE INTO settings (key, value)
        SELECT 'time_work_secs', CAST(CAST(value AS INTEGER) * 60 AS TEXT)
          FROM settings WHERE key = 'time_work_mins';
    INSERT OR IGNORE INTO settings (key, value)
        SELECT 'time_short_break_secs', CAST(CAST(value AS INTEGER) * 60 AS TEXT)
          FROM settings WHERE key = 'time_short_break_mins';
    INSERT OR IGNORE INTO settings (key, value)
        SELECT 'time_long_break_secs', CAST(CAST(value AS INTEGER) * 60 AS TEXT)
          FROM settings WHERE key = 'time_long_break_mins';
    DELETE FROM settings WHERE key IN
        ('time_work_mins', 'time_short_break_mins', 'time_long_break_mins');
    INSERT INTO schema_version VALUES (2);
";

/// Seeds the `check_for_updates` setting for users upgrading from a version
/// that did not have this setting. Fresh installs get it via seed_defaults.
const MIGRATION_3: &str = "
    INSERT OR IGNORE INTO settings (key, value) VALUES ('check_for_updates', 'true');
    INSERT INTO schema_version VALUES (3);
";

/// Seeds the `global_shortcuts_enabled` setting for all installs. Defaults to
/// 'false' — global shortcuts are now opt-in. This is a breaking change for
/// existing users who relied on shortcuts being active by default; they must
/// re-enable them in Settings → Shortcuts.
const MIGRATION_4: &str = "
    INSERT OR IGNORE INTO settings (key, value) VALUES ('global_shortcuts_enabled', 'false');
    INSERT INTO schema_version VALUES (4);
";

/// Seeds the `short_breaks_enabled` and `long_breaks_enabled` settings for
/// users upgrading from a version that did not have these settings.
/// Both default to 'true' — existing behaviour is preserved.
const MIGRATION_5: &str = "
    INSERT OR IGNORE INTO settings (key, value) VALUES ('short_breaks_enabled', 'true');
    INSERT OR IGNORE INTO settings (key, value) VALUES ('long_breaks_enabled', 'true');
    INSERT INTO schema_version VALUES (5);
";

/// Seeds the seven local shortcut key bindings for users upgrading from a version
/// Seeds the seven local shortcut key bindings for users upgrading from a version
/// that did not have this feature. These shortcuts are handled entirely by the frontend
/// (keydown listeners) and require no Rust-side dispatch logic.
const MIGRATION_6: &str = "
    INSERT OR IGNORE INTO settings (key, value) VALUES ('local_shortcut_toggle', ' ');
    INSERT OR IGNORE INTO settings (key, value) VALUES ('local_shortcut_reset', 'ArrowLeft');
    INSERT OR IGNORE INTO settings (key, value) VALUES ('local_shortcut_skip', 'ArrowRight');
    INSERT OR IGNORE INTO settings (key, value) VALUES ('local_shortcut_volume_down', 'ArrowDown');
    INSERT OR IGNORE INTO settings (key, value) VALUES ('local_shortcut_volume_up', 'ArrowUp');
    INSERT OR IGNORE INTO settings (key, value) VALUES ('local_shortcut_mute', 'm');
    INSERT OR IGNORE INTO settings (key, value) VALUES ('local_shortcut_fullscreen', 'F11');
    INSERT INTO schema_version VALUES (6);
";

/// Adds `target_secs` to `sessions` to track target duration alongside actual duration,
/// backfilling existing sessions, and cleans up incomplete abandoned sessions with < 2 mins.
const MIGRATION_7: &str = "
    ALTER TABLE sessions ADD COLUMN target_secs INTEGER NOT NULL DEFAULT 1500;
    UPDATE sessions SET target_secs = duration_secs WHERE duration_secs > 0;
    DELETE FROM sessions WHERE completed = 0 AND (ended_at IS NULL OR duration_secs < 120);
    INSERT INTO schema_version VALUES (7);
";

/// Adds `task_name` to `sessions` to track task/subject, creates `tasks` table with 'General',
/// and stores 'last_task_name' in settings.
const MIGRATION_8: &str = "
    ALTER TABLE sessions ADD COLUMN task_name TEXT NOT NULL DEFAULT 'General';
    CREATE TABLE IF NOT EXISTS tasks (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        name       TEXT NOT NULL UNIQUE,
        created_at INTEGER NOT NULL
    );
    INSERT OR IGNORE INTO tasks (name, created_at) VALUES ('General', strftime('%s', 'now'));
    INSERT OR IGNORE INTO settings (key, value) VALUES ('last_task_name', 'General');
    INSERT INTO schema_version VALUES (8);
";

/// Adds `completed` and `completed_at` to `tasks` table to allow marking tasks as done and archiving them.
const MIGRATION_9: &str = "
    ALTER TABLE tasks ADD COLUMN completed INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE tasks ADD COLUMN completed_at INTEGER;
    INSERT INTO schema_version VALUES (9);
";

/// Adds `deleted` and `deleted_at` to `tasks` table to allow soft-deleting tasks,
/// and populates deleted tasks from any orphaned task names in sessions.
const MIGRATION_10: &str = "
    ALTER TABLE tasks ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE tasks ADD COLUMN deleted_at INTEGER;
    INSERT OR IGNORE INTO tasks (name, created_at, completed, deleted, deleted_at)
        SELECT DISTINCT task_name, min(started_at), 0, 0, NULL
        FROM sessions
        WHERE task_name != '' AND task_name NOT IN (SELECT name FROM tasks);
    INSERT INTO schema_version VALUES (10);
";

/// Restores any historical tasks that were erroneously marked as deleted,
/// and ensures all tasks with recorded sessions are active (deleted = 0).
const MIGRATION_11: &str = "
    UPDATE tasks SET deleted = 0, deleted_at = NULL 
    WHERE name IN (SELECT DISTINCT task_name FROM sessions WHERE task_name != '');
    INSERT OR IGNORE INTO tasks (name, created_at, completed, deleted, deleted_at)
        SELECT DISTINCT task_name, min(started_at), 0, 0, NULL
        FROM sessions
        WHERE task_name != '' AND task_name NOT IN (SELECT name FROM tasks);
    INSERT INTO schema_version VALUES (11);
";

/// Creates the presets table for timer duration presets and seeds the "Default" preset.
const MIGRATION_12: &str = "
    CREATE TABLE IF NOT EXISTS presets (
        id               INTEGER PRIMARY KEY AUTOINCREMENT,
        name             TEXT NOT NULL UNIQUE,
        work_secs        INTEGER NOT NULL,
        short_break_secs INTEGER NOT NULL,
        long_break_secs  INTEGER NOT NULL,
        rounds           INTEGER NOT NULL
    );

    INSERT OR IGNORE INTO presets (name, work_secs, short_break_secs, long_break_secs, rounds)
    VALUES ('Default', 1500, 300, 900, 4);

    INSERT INTO schema_version VALUES (12);
";

/// Apply any pending migrations. Each migration is wrapped in a transaction
/// so a partial failure leaves the database unchanged.
pub fn run(conn: &Connection) -> Result<()> {
    let version = current_version(conn)?;

    if version < 1 {
        log::info!("[db/migrations] applying MIGRATION_1: initial schema");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_1} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_1 complete");
    }

    if version < 2 {
        log::info!("[db/migrations] applying MIGRATION_2: timer durations minutes → seconds");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_2} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_2 complete");
    }

    if version < 3 {
        log::info!("[db/migrations] applying MIGRATION_3: seed check_for_updates setting");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_3} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_3 complete");
    }

    if version < 4 {
        log::info!("[db/migrations] applying MIGRATION_4: seed global_shortcuts_enabled setting");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_4} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_4 complete");
    }

    if version < 5 {
        log::info!("[db/migrations] applying MIGRATION_5: seed short_breaks_enabled and long_breaks_enabled");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_5} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_5 complete");
    }

    if version < 6 {
        log::info!("[db/migrations] applying MIGRATION_6: seed local shortcut key bindings");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_6} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_6 complete");
    }

    if version < 7 {
        log::info!("[db/migrations] applying MIGRATION_7: add target_secs and clean sessions");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_7} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_7 complete");
    }

    if version < 8 {
        log::info!("[db/migrations] applying MIGRATION_8: add task_name to sessions and create tasks table");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_8} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_8 complete");
    }

    if version < 9 {
        log::info!("[db/migrations] applying MIGRATION_9: add completed and completed_at to tasks table");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_9} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_9 complete");
    }

    if version < 10 {
        log::info!("[db/migrations] applying MIGRATION_10: add deleted and deleted_at to tasks table");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_10} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_10 complete");
    }

    if version < 11 {
        log::info!("[db/migrations] applying MIGRATION_11: restore session tasks to active");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_11} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_11 complete");
    }

    if version < 12 {
        log::info!("[db/migrations] applying MIGRATION_12: create presets table and seed Default preset");
        conn.execute_batch(&format!("BEGIN; {MIGRATION_12} COMMIT;"))?;
        log::info!("[db/migrations] MIGRATION_12 complete");
    }

    Ok(())
}

/// Returns the current schema version, or 0 if the database is fresh.
fn current_version(conn: &Connection) -> Result<i64> {
    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_version'",
        [],
        |row| row.get(0),
    )?;

    if !table_exists {
        return Ok(0);
    }

    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_version",
        [],
        |row| row.get(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_is_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        // Second run must not error (version check prevents re-application).
        run(&conn).unwrap();
        let v: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 12);
    }

    #[test]
    fn all_tables_created() {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        for table in &["settings", "sessions", "custom_themes", "schema_version", "tasks", "presets"] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    [table],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "table '{table}' was not created");
        }
    }
}
