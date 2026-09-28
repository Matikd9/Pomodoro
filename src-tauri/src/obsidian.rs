use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const REFLECTION_HEADER: &str = "## 📝 Notas y Reflexión Semanal";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObsidianExportResult {
    pub success: bool,
    pub exported: bool,
    pub file_path: Option<String>,
    pub message: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
}

pub fn get_obsidian_dir() -> PathBuf {
    if let Ok(profile) = std::env::var("USERPROFILE") {
        PathBuf::from(profile).join("Documents").join("Obsidian").join("Pomodoro")
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join("Documents").join("Obsidian").join("Pomodoro")
    } else {
        PathBuf::from(r"C:\Users\Matiql\Documents\Obsidian\Pomodoro")
    }
}

fn month_name_es(month: u32) -> &'static str {
    match month {
        1 => "Ene",
        2 => "Feb",
        3 => "Mar",
        4 => "Abr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Ago",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dic",
        _ => "",
    }
}

fn day_name_es(idx: usize) -> &'static str {
    match idx {
        0 => "Lunes",
        1 => "Martes",
        2 => "Miércoles",
        3 => "Jueves",
        4 => "Viernes",
        5 => "Sábado",
        6 => "Domingo",
        _ => "",
    }
}

fn format_duration_hm(secs: u32) -> String {
    let hours = secs / 3600;
    let mins = (secs % 3600 + 30) / 60;
    if hours > 0 {
        format!("{hours}h {mins:02}m")
    } else {
        format!("{mins}m")
    }
}

struct WeekBounds {
    start_date: String,
    end_date: String,
    iso_year: i32,
    iso_week: u32,
}

fn get_week_bounds(conn: &Connection, week_offset: i32) -> rusqlite::Result<WeekBounds> {
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

    Ok(WeekBounds {
        start_date,
        end_date,
        iso_year,
        iso_week,
    })
}

fn build_date_range_label(start_date: &str, end_date: &str) -> String {
    // "YYYY-MM-DD"
    let parse_parts = |s: &str| -> (u32, u32) {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() == 3 {
            (parts[1].parse().unwrap_or(1), parts[2].parse().unwrap_or(1))
        } else {
            (1, 1)
        }
    };
    let (m1, d1) = parse_parts(start_date);
    let (m2, d2) = parse_parts(end_date);

    if m1 == m2 {
        format!("{d1} al {d2} {}", month_name_es(m1))
    } else {
        format!("{d1} {} al {d2} {}", month_name_es(m1), month_name_es(m2))
    }
}

fn default_reflection_text() -> &'static str {
    "> Espacio para tus anotaciones personales, logros de la semana y áreas de mejora.\n"
}

pub fn export_weekly_report(
    conn: &Connection,
    week_offset: i32,
    custom_dir: Option<PathBuf>,
) -> Result<ObsidianExportResult, String> {
    let bounds = get_week_bounds(conn, week_offset).map_err(|e| e.to_string())?;

    // Query weekly totals
    let (total_sessions, total_focus_secs, total_rounds): (i64, i64, f64) = conn
        .query_row(
            "SELECT COUNT(*),
                    COALESCE(SUM(duration_secs), 0),
                    COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0)
             FROM sessions
             WHERE round_type = 'work' AND duration_secs >= 120
               AND date(started_at, 'unixepoch', 'localtime') >= ?1
               AND date(started_at, 'unixepoch', 'localtime') <= ?2",
            [&bounds.start_date, &bounds.end_date],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| e.to_string())?;

    // Rule: If there is no activity in that week, DO NOT generate any file or note.
    if total_focus_secs == 0 {
        return Ok(ObsidianExportResult {
            success: true,
            exported: false,
            file_path: None,
            message: format!(
                "No hubo sesiones de enfoque registradas en la semana {} ({}). No se generó reporte.",
                bounds.iso_week,
                build_date_range_label(&bounds.start_date, &bounds.end_date)
            ),
            filename: None,
            content: None,
        });
    }

    let target_dir = custom_dir.unwrap_or_else(get_obsidian_dir);
    std::fs::create_dir_all(&target_dir).map_err(|e| {
        format!(
            "No se pudo crear el directorio de Obsidian en {}: {e}",
            target_dir.display()
        )
    })?;

    let range_label = build_date_range_label(&bounds.start_date, &bounds.end_date);
    let filename = format!(
        "{}-W{:02} ({}).md",
        bounds.iso_year, bounds.iso_week, range_label
    );
    let target_file = target_dir.join(&filename);

    // Query task breakdown for this week
    struct TaskRow {
        task_name: String,
        secs: u32,
        rounds: f32,
    }
    let mut stmt = conn
        .prepare(
            "SELECT COALESCE(NULLIF(task_name, ''), 'General') as task,
                    COALESCE(SUM(duration_secs), 0) as total_secs,
                    COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0) as total_rounds
             FROM sessions
             WHERE round_type = 'work' AND duration_secs >= 120
               AND date(started_at, 'unixepoch', 'localtime') >= ?1
               AND date(started_at, 'unixepoch', 'localtime') <= ?2
             GROUP BY task
             ORDER BY total_secs DESC",
        )
        .map_err(|e| e.to_string())?;

    let task_rows = stmt
        .query_map([&bounds.start_date, &bounds.end_date], |r| {
            Ok(TaskRow {
                task_name: r.get(0)?,
                secs: r.get::<_, i64>(1)? as u32,
                rounds: r.get::<_, f64>(2)? as f32,
            })
        })
        .map_err(|e| e.to_string())?
        .flatten()
        .collect::<Vec<_>>();

    // Query daily breakdown (7 days: Monday to Sunday)
    struct DayRow {
        date: String,
        day_name: &'static str,
        secs: u32,
        rounds: f32,
    }

    let mut daily_rows: Vec<DayRow> = Vec::with_capacity(7);
    for i in 0..7 {
        let day_date: String = conn
            .query_row(
                "SELECT date(?1, ?2)",
                [&bounds.start_date, &format!("+{i} days")],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;

        let (d_secs, d_rounds): (i64, f64) = conn
            .query_row(
                "SELECT COALESCE(SUM(duration_secs), 0),
                        COALESCE(SUM(CAST(duration_secs AS REAL) / CAST(CASE WHEN target_secs > 0 THEN target_secs ELSE 1500 END AS REAL)), 0.0)
                 FROM sessions
                 WHERE round_type = 'work' AND duration_secs >= 120
                   AND date(started_at, 'unixepoch', 'localtime') = ?1",
                [&day_date],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap_or((0, 0.0));

        daily_rows.push(DayRow {
            date: day_date,
            day_name: day_name_es(i),
            secs: d_secs as u32,
            rounds: d_rounds as f32,
        });
    }

    let active_days = daily_rows.iter().filter(|d| d.secs > 0).count();
    let daily_avg_secs = if active_days > 0 {
        (total_focus_secs as u32) / (active_days as u32)
    } else {
        0
    };

    let total_mins = (total_focus_secs + 30) / 60;
    let total_hours = (total_focus_secs as f64) / 3600.0;

    // Preserve existing reflection notes if re-exporting
    let preserved_reflection = if target_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&target_file) {
            if let Some(pos) = content.find(REFLECTION_HEADER) {
                let after = &content[pos + REFLECTION_HEADER.len()..];
                let trimmed = after.trim_start_matches(['\r', '\n']);
                if !trimmed.trim().is_empty() {
                    trimmed.to_string()
                } else {
                    default_reflection_text().to_string()
                }
            } else {
                default_reflection_text().to_string()
            }
        } else {
            default_reflection_text().to_string()
        }
    } else {
        default_reflection_text().to_string()
    };

    let settings = crate::settings::load(conn).unwrap_or_default();
    let goal_hours = settings.weekly_goal_hours;
    let cumplimiento_pct = if goal_hours > 0 {
        (total_hours / goal_hours as f64) * 100.0
    } else {
        0.0
    };

    // Format Markdown content
    let mut md = String::new();

    // Frontmatter for Dataview
    md.push_str("---\n");
    md.push_str("tipo: reporte-pomodoro\n");
    md.push_str(&format!("semana: \"{}-W{:02}\"\n", bounds.iso_year, bounds.iso_week));
    md.push_str(&format!("fecha_inicio: {}\n", bounds.start_date));
    md.push_str(&format!("fecha_fin: {}\n", bounds.end_date));
    md.push_str(&format!("tiempo_total_min: {}\n", total_mins));
    md.push_str(&format!("horas_totales: {:.1}\n", total_hours));
    md.push_str(&format!("meta_semanal_horas: {}\n", goal_hours));
    md.push_str(&format!("cumplimiento_meta_pct: {:.1}\n", cumplimiento_pct));
    md.push_str(&format!("rondas_totales: {:.1}\n", total_rounds));
    md.push_str(&format!("sesiones_totales: {}\n", total_sessions));
    md.push_str(&format!("dias_activos: {}\n", active_days));
    md.push_str("tags:\n");
    md.push_str("  - pomodoro\n");
    md.push_str("  - reporte-semanal\n");
    md.push_str("---\n\n");

    // Title
    md.push_str(&format!(
        "# 🍅 Reporte Semanal: Semana {} ({} {})\n\n",
        bounds.iso_week, range_label, bounds.iso_year
    ));

    // Executive summary
    md.push_str("## 📊 Resumen Ejecutivo\n");
    md.push_str(&format!(
        "- **Tiempo Total Enfocado:** {} ({} min)\n",
        format_duration_hm(total_focus_secs as u32),
        total_mins
    ));
    md.push_str(&format!(
        "- **Meta Semanal:** {:.1}h / {}h ({:.1}% cumplido)\n",
        total_hours, goal_hours, cumplimiento_pct
    ));
    md.push_str(&format!("- **Rondas Completadas:** {:.1}\n", total_rounds));
    let days_label = if active_days == 1 { "día activo" } else { "días activos" };
    md.push_str(&format!(
        "- **Promedio Diario:** {} (en {} {})\n",
        format_duration_hm(daily_avg_secs),
        active_days,
        days_label
    ));
    md.push_str(&format!("- **Días con Actividad:** {} / 7\n\n", active_days));

    // Tasks breakdown table
    md.push_str("## 🎯 Distribución por Tareas\n");
    md.push_str("| Tarea | Tiempo | Horas | Rondas | % del Total |\n");
    md.push_str("| :--- | :---: | :---: | :---: | :---: |\n");
    for t in &task_rows {
        let pct = if total_focus_secs > 0 {
            ((t.secs as f64) / (total_focus_secs as f64) * 100.0).round() as u32
        } else {
            0
        };
        let hours = (t.secs as f64) / 3600.0;
        md.push_str(&format!(
            "| {} | {} | {:.1}h | {:.1} | {}% |\n",
            t.task_name,
            format_duration_hm(t.secs),
            hours,
            t.rounds,
            pct
        ));
    }
    md.push('\n');

    // Daily breakdown table
    md.push_str("## 📅 Detalle Diario\n");
    md.push_str("| Día | Fecha | Rondas | Tiempo |\n");
    md.push_str("| :--- | :---: | :---: | :---: |\n");
    for d in &daily_rows {
        md.push_str(&format!(
            "| {} | {} | {:.1} | {} |\n",
            d.day_name,
            d.date,
            d.rounds,
            format_duration_hm(d.secs)
        ));
    }
    md.push('\n');

    // Reflection section
    md.push_str(REFLECTION_HEADER);
    md.push('\n');
    md.push_str(&preserved_reflection);
    if !preserved_reflection.ends_with('\n') {
        md.push('\n');
    }

    std::fs::write(&target_file, &md).map_err(|e| {
        format!(
            "Error al escribir reporte en {}: {e}",
            target_file.display()
        )
    })?;

    log::info!("[obsidian] Weekly report written to {}", target_file.display());

    Ok(ObsidianExportResult {
        success: true,
        exported: true,
        file_path: Some(target_file.to_string_lossy().to_string()),
        message: format!("Reporte semanal exportado a Obsidian: {}", filename),
        filename: Some(filename),
        content: Some(md),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations;
    use crate::db::queries;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn week_with_no_sessions_skips_export() {
        let conn = setup_db();
        let temp_dir = std::env::temp_dir().join("pomotroid_test_obsidian_empty");
        let result = export_weekly_report(&conn, 0, Some(temp_dir)).unwrap();
        assert!(!result.exported);
        assert!(result.file_path.is_none());
    }

    #[test]
    fn week_with_sessions_exports_and_preserves_reflection() {
        let conn = setup_db();
        let temp_dir = std::env::temp_dir().join("pomotroid_test_obsidian_export");
        let _ = std::fs::remove_dir_all(&temp_dir);

        // Insert sessions today
        let id1 = queries::insert_session(&conn, "work", 1500, 1500, "Math").unwrap();
        queries::complete_session(&conn, id1, 1500, true).unwrap();

        let id2 = queries::insert_session(&conn, "work", 1500, 1500, "Coding").unwrap();
        queries::complete_session(&conn, id2, 1500, true).unwrap();

        let res1 = export_weekly_report(&conn, 0, Some(temp_dir.clone())).unwrap();
        assert!(res1.exported);
        let path = PathBuf::from(res1.file_path.unwrap());
        assert!(path.exists());

        let content1 = std::fs::read_to_string(&path).unwrap();
        assert!(content1.contains("Math"));
        assert!(content1.contains("Coding"));
        assert!(content1.contains("tipo: reporte-pomodoro"));

        // Simulate user writing custom notes
        let custom_notes = "\nEsta semana logré resolver el problema de álgebra y avanzar con el backend.\n";
        let modified_content = format!("{}{}", content1, custom_notes);
        std::fs::write(&path, modified_content).unwrap();

        // Add another session
        let id3 = queries::insert_session(&conn, "work", 1500, 1500, "Math").unwrap();
        queries::complete_session(&conn, id3, 1500, true).unwrap();

        // Re-export
        let res2 = export_weekly_report(&conn, 0, Some(temp_dir.clone())).unwrap();
        assert!(res2.exported);

        let content2 = std::fs::read_to_string(&path).unwrap();
        assert!(content2.contains("Esta semana logré resolver el problema de álgebra"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
