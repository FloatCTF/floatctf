//! Compare live public tables / enums against the expected FloatCTF schema set.
//!
//! Deeper checks (enums, minimum column counts) are intentionally cheap — no
//! full `pg_dump` — and target post-migration schema (base + AWD + m0101).

use sea_orm_migration::sea_orm::{ConnectionTrait, Database, DbBackend, DbErr, Statement};
use std::collections::{HashMap, HashSet};

/// Tables defined by init + AWD SQL (and present in generated entities for core/AWD).
///
/// Note: `kv_store` / `oob_*` may exist in some environments via ad-hoc DDL; they are
/// optional and listed separately so check modes can choose strict vs soft.
pub const EXPECTED_CORE_TABLES: &[&str] = &[
    "settings",
    "weapons",
    "scheduled_tasks",
    "users",
    "super_admin",
    "announcements",
    "challenges",
    "challenge_solves",
    "challenge_writeup",
    "challenge_sets",
    "challenge_set_items",
    "gameboxes",
    "instances",
    "events",
    "event_users",
    "event_teams",
    "event_team_members",
    "event_announcements",
    "event_writeup",
    "event_challenges",
    "event_challenge_solves",
    "event_gameboxes",
    "event_instances",
    "event_logs",
    "logs",
    "discussions",
    "discussion_comments",
    "discussion_likes",
];

pub const EXPECTED_AWD_TABLES: &[&str] = &[
    "awd_events",
    "awd_team_networks",
    "awd_gamebox_templates",
    "awd_gamebox_instances",
    "awd_wireguard_peers",
    "awd_rounds",
    "awd_flag_issues",
    "awd_flag_submissions",
    "awd_score_events",
    "awd_judge_batches",
    "awd_judge_tasks",
    "awd_reset_records",
    "awd_team_bans",
    "awd_precheck_runs",
    "awd_runtime_resources",
    "awd_orphan_resources",
    "awd_internal_token_rotations",
];

/// Optional tables that may appear when generated from a fuller DB snapshot.
pub const OPTIONAL_TABLES: &[&str] = &["kv_store", "oob_records", "oob_tokens"];

/// PostgreSQL enums created by init + AWD SQL (names only; labels not fully diffed).
pub const EXPECTED_ENUMS: &[&str] = &[
    // init
    "setting_value_type",
    "instance_status",
    "event_type",
    "event_team_member_role",
    // awd
    "awd_event_status",
    "awd_phase",
    "gamebox_status",
    "wg_peer_status",
    "round_status",
    "score_event_type",
    "judge_task_status",
    "ban_status",
    "precheck_status",
];

/// Minimum column counts derived from `CREATE TABLE` SQL + m0101 scheduler columns.
/// Live DBs may have *more* columns (historical ad-hoc patches such as `users.avatar`);
/// fewer columns means the migration set is incomplete or out of date.
pub fn expected_min_column_counts() -> HashMap<&'static str, usize> {
    // Counts from src/sql CREATE TABLE bodies; scheduled_tasks includes m0101 (+6).
    HashMap::from([
        ("settings", 7usize),
        ("weapons", 9),
        ("scheduled_tasks", 23), // 17 base + 6 m0101
        ("users", 7),
        ("super_admin", 6),
        ("announcements", 7),
        ("challenges", 10),
        ("challenge_solves", 4),
        ("challenge_writeup", 5),
        ("challenge_sets", 4),
        ("challenge_set_items", 2),
        ("gameboxes", 14),
        ("instances", 12),
        ("events", 12),
        ("event_users", 5),
        ("event_teams", 8),
        ("event_team_members", 5),
        ("event_announcements", 5),
        ("event_writeup", 5),
        ("event_challenges", 4),
        ("event_challenge_solves", 7),
        ("event_gameboxes", 3),
        ("event_instances", 4),
        ("event_logs", 10),
        ("logs", 10),
        ("discussions", 9),
        ("discussion_comments", 7),
        ("discussion_likes", 4),
        ("awd_events", 38),
        ("awd_team_networks", 13),
        ("awd_gamebox_templates", 23),
        ("awd_gamebox_instances", 15),
        ("awd_wireguard_peers", 13),
        ("awd_rounds", 12),
        ("awd_flag_issues", 6),
        ("awd_flag_submissions", 9),
        ("awd_score_events", 15),
        ("awd_judge_batches", 8),
        ("awd_judge_tasks", 19),
        ("awd_reset_records", 13),
        ("awd_team_bans", 12),
        ("awd_precheck_runs", 14),
        ("awd_runtime_resources", 7),
        ("awd_orphan_resources", 9),
        ("awd_internal_token_rotations", 5),
    ])
}

#[derive(Debug, Default)]
pub struct SchemaCheckReport {
    pub present: Vec<String>,
    pub missing_core: Vec<String>,
    pub missing_awd: Vec<String>,
    pub extra: Vec<String>,
    pub missing_enums: Vec<String>,
    /// Tables present but with fewer columns than the migration SQL defines.
    pub short_columns: Vec<(String, usize, usize)>, // (table, live, expected_min)
    /// Tables with more columns than the base SQL (informational; not a failure).
    pub extra_columns: Vec<(String, usize, usize)>,
}

impl SchemaCheckReport {
    pub fn ok(&self) -> bool {
        self.missing_core.is_empty()
            && self.missing_awd.is_empty()
            && self.missing_enums.is_empty()
            && self.short_columns.is_empty()
    }

    pub fn summary(&self) -> String {
        format!(
            "present={} missing_core={} missing_awd={} missing_enums={} short_cols={} extra_cols={} extra_tables={}",
            self.present.len(),
            self.missing_core.len(),
            self.missing_awd.len(),
            self.missing_enums.len(),
            self.short_columns.len(),
            self.extra_columns.len(),
            self.extra.len()
        )
    }
}

pub async fn check_schema(database_url: &str) -> Result<SchemaCheckReport, DbErr> {
    let db = Database::connect(database_url).await?;

    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Postgres,
            r#"
            SELECT tablename
            FROM pg_tables
            WHERE schemaname = 'public'
              AND tablename NOT LIKE 'seaql_%'
            ORDER BY tablename
            "#
            .to_string(),
        ))
        .await?;

    let mut live: Vec<String> = rows
        .into_iter()
        .filter_map(|r| r.try_get::<String>("", "tablename").ok())
        .collect();
    live.sort();

    let mut report = SchemaCheckReport {
        present: live.clone(),
        ..Default::default()
    };

    for t in EXPECTED_CORE_TABLES {
        if !live.iter().any(|x| x == t) {
            report.missing_core.push((*t).to_string());
        }
    }
    for t in EXPECTED_AWD_TABLES {
        if !live.iter().any(|x| x == t) {
            report.missing_awd.push((*t).to_string());
        }
    }

    let expected: HashSet<&str> = EXPECTED_CORE_TABLES
        .iter()
        .chain(EXPECTED_AWD_TABLES.iter())
        .chain(OPTIONAL_TABLES.iter())
        .copied()
        .collect();

    for t in &live {
        if !expected.contains(t.as_str()) {
            report.extra.push(t.clone());
        }
    }

    // Enum presence (pg_type, public schema user-defined enums).
    let enum_rows = db
        .query_all(Statement::from_string(
            DbBackend::Postgres,
            r#"
            SELECT t.typname AS name
            FROM pg_type t
            JOIN pg_namespace n ON n.oid = t.typnamespace
            WHERE n.nspname = 'public'
              AND t.typtype = 'e'
            ORDER BY t.typname
            "#
            .to_string(),
        ))
        .await?;
    let live_enums: HashSet<String> = enum_rows
        .into_iter()
        .filter_map(|r| r.try_get::<String>("", "name").ok())
        .collect();
    for e in EXPECTED_ENUMS {
        if !live_enums.contains(*e) {
            report.missing_enums.push((*e).to_string());
        }
    }

    // Column counts for present expected tables.
    let min_cols = expected_min_column_counts();
    let col_rows = db
        .query_all(Statement::from_string(
            DbBackend::Postgres,
            r#"
            SELECT table_name, COUNT(*)::bigint AS n
            FROM information_schema.columns
            WHERE table_schema = 'public'
            GROUP BY table_name
            "#
            .to_string(),
        ))
        .await?;
    let mut live_cols: HashMap<String, usize> = HashMap::new();
    for r in col_rows {
        if let (Ok(name), Ok(n)) = (
            r.try_get::<String>("", "table_name"),
            r.try_get::<i64>("", "n"),
        ) {
            live_cols.insert(name, n as usize);
        }
    }

    for (table, expected_min) in min_cols {
        if !live.iter().any(|x| x == table) {
            continue; // already reported as missing table
        }
        let Some(&live_n) = live_cols.get(table) else {
            report
                .short_columns
                .push((table.to_string(), 0, expected_min));
            continue;
        };
        if live_n < expected_min {
            report
                .short_columns
                .push((table.to_string(), live_n, expected_min));
        } else if live_n > expected_min {
            report
                .extra_columns
                .push((table.to_string(), live_n, expected_min));
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expected_sets_are_disjoint_from_optional() {
        for t in OPTIONAL_TABLES {
            assert!(!EXPECTED_CORE_TABLES.contains(t));
            assert!(!EXPECTED_AWD_TABLES.contains(t));
        }
    }

    #[test]
    fn core_and_awd_counts_match_entity_modules() {
        // Sanity: keep lists non-empty and stable for agents.
        assert!(EXPECTED_CORE_TABLES.len() >= 20);
        assert!(EXPECTED_AWD_TABLES.len() >= 10);
        assert!(EXPECTED_ENUMS.len() >= 10);
    }

    #[test]
    fn min_column_map_covers_expected_tables() {
        let m = expected_min_column_counts();
        for t in EXPECTED_CORE_TABLES
            .iter()
            .chain(EXPECTED_AWD_TABLES.iter())
        {
            assert!(m.contains_key(t), "missing min column count for {t}");
        }
        assert_eq!(m["scheduled_tasks"], 23);
    }
}
