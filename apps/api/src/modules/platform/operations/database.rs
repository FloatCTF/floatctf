use actix_web::rt::time::Instant;
use base64::Engine;
use sea_orm::sqlx::{self, Column, Row, TypeInfo, postgres::PgRow};
use serde_json::{Value, json};

use crate::api::prelude::*;

/// Maximum allowed SQL statement length (characters).
const MAX_SQL_LENGTH: usize = 10_000;

/// Maximum allowed execution time (seconds).
const SQL_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Serialize, Deserialize)]
pub struct SqlStatement {
    sql: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SqlResult {
    pub sql_type: String, // exec , query
    pub rows: Vec<Value>,
    pub count: usize,
    pub rows_affected: u64,
    pub elapsed_ms: u128,
}

#[post("/exec_sql")]
pub async fn exec_sql(
    ctx: ReqCtx,
    user: SuperAdminJwtGuard,
    ss: Json<SqlStatement>,
) -> UniResult<SqlResult> {
    // Gate: require unsafe_sql_admin in the static TOML configuration.
    if !ctx.config.features.enable_unsafe_sql_admin {
        return Err(AppError::NotFound(
            "SQL execution is disabled. Set [features].unsafe_sql_admin = true in the TOML config to enable.".into(),
        ));
    }

    let user = user.into_inner();
    let sql = ss.into_inner().sql;

    // Explicit audit when the dangerous path is actually used (no SQL body stored).
    crate::infrastructure::audit::AuditService::new(ctx.log.get_ref().clone())
        .record(
            crate::infrastructure::audit::service::AuditAction::UnsafeSqlExecuted,
            &format!("{} invoked admin SQL endpoint", user.username),
            serde_json::json!({
                "sql_len": sql.len(),
                "sql_prefix": sql.chars().take(32).collect::<String>(),
            }),
            None,
            Some(user.id),
            Some(&ctx.req),
        )
        .await;

    // ── Protections when enabled ──

    // 1. Length limit
    if sql.len() > MAX_SQL_LENGTH {
        return Err(AppError::BadRequest(format!(
            "SQL exceeds maximum length ({} > {})",
            sql.len(),
            MAX_SQL_LENGTH
        )));
    }

    // 2. No multi-statement (reject embedded semicolons outside trailing whitespace)
    let trimmed = sql.trim_end();
    if trimmed.len() > 1 {
        let without_trailing = trimmed.trim_end_matches(';').trim_end();
        if without_trailing.contains(';') {
            return Err(AppError::BadRequest(
                "Multi-statement SQL is not allowed".into(),
            ));
        }
    }

    // 3. Parse SQL command type
    let sql_command = |sql: &str| {
        sql.trim_start()
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase()
    };
    let sql_cmd = sql_command(&sql);

    // 4. Read-only enforcement: only SELECT/SHOW/DESCRIBE/EXPLAIN/WITH allowed
    let is_read_only = matches!(
        sql_cmd.as_str(),
        "select" | "show" | "describe" | "explain" | "with"
    );

    if !is_read_only {
        // Write statements: require explicit confirmation via SQL prefix
        if !sql
            .trim_start()
            .to_uppercase()
            .starts_with("/* ADMIN_CONFIRMED */")
        {
            return Err(AppError::BadRequest(format!(
                "Write statements must be prefixed with /* ADMIN_CONFIRMED */. \
                 Statement type '{}' is not read-only.",
                sql_cmd
            )));
        }
    }

    // 5. Strip the confirmation prefix if present
    let sql = if sql
        .trim_start()
        .to_uppercase()
        .starts_with("/* ADMIN_CONFIRMED */")
    {
        sql.trim_start()
            .trim_start_matches("/* ADMIN_CONFIRMED */")
            .trim_start()
            .to_string()
    } else {
        sql
    };

    // 6. Execute with timeout
    let start_time = Instant::now();
    let timeout = std::time::Duration::from_secs(SQL_TIMEOUT_SECS);

    let result = match sql_cmd.as_str() {
        "select" | "show" | "describe" | "explain" | "with" => {
            match tokio::time::timeout(
                timeout,
                sqlx::query(&sql).fetch_all(ctx.db.get_postgres_connection_pool()),
            )
            .await
            {
                Ok(Ok(rows)) => {
                    let elapsed = start_time.elapsed().as_millis();
                    let data = rows_to_json(rows);

                    // Audit read queries too
                    ctx.log
                        .add_log(
                            "INFO",
                            "DATABASE",
                            "EXEC_SQL",
                            format!(
                                "{} executed read query ({} rows)",
                                user.username,
                                data.len()
                            )
                            .as_str(),
                            json!({"rows": data.len(), "elapsed_ms": elapsed, "sql_type": "query"}),
                            None,
                            user.id.into(),
                            Some(&ctx.req),
                        )
                        .await;

                    Ok(SqlResult {
                        sql_type: "query".to_string(),
                        rows: data.clone(),
                        count: data.len(),
                        rows_affected: 0,
                        elapsed_ms: elapsed,
                    })
                }
                Ok(Err(e)) => Err(AppError::BadRequest(e.to_string())),
                Err(_) => Err(AppError::BadRequest(format!(
                    "SQL execution timed out ({}s limit)",
                    SQL_TIMEOUT_SECS
                ))),
            }
        }
        _ => match tokio::time::timeout(
            timeout,
            sqlx::query(&sql).execute(ctx.db.get_postgres_connection_pool()),
        )
        .await
        {
            Ok(Ok(res)) => {
                let elapsed = start_time.elapsed().as_millis();
                let rows_affected = res.rows_affected();

                ctx.log
                    .add_log(
                        "WARN",
                        "DATABASE",
                        "EXEC_SQL",
                        format!(
                            "{} executed write statement, affected {} rows",
                            user.username, rows_affected
                        )
                        .as_str(),
                        json!({"rows_affected": rows_affected, "elapsed_ms": elapsed, "sql_type": "exec"}),
                        None,
                        user.id.into(),
                        Some(&ctx.req),
                    )
                    .await;

                Ok(SqlResult {
                    sql_type: "exec".to_string(),
                    rows: vec![],
                    count: 0,
                    rows_affected,
                    elapsed_ms: elapsed,
                })
            }
            Ok(Err(e)) => Err(AppError::BadRequest(e.to_string())),
            Err(_) => Err(AppError::BadRequest(format!(
                "SQL execution timed out ({}s limit)",
                SQL_TIMEOUT_SECS
            ))),
        },
    };

    result.map(|r| UniResponse::ok(r.into()).into())
}

pub fn rows_to_json(rows: Vec<PgRow>) -> Vec<Value> {
    let mut out = Vec::new();
    for row in rows {
        let mut obj = serde_json::Map::new();
        for col in row.columns() {
            let name = col.name();
            let type_name = col.type_info().name();

            let value = match type_name {
                "TEXT" | "VARCHAR" | "CHAR" | "BPCHAR" | "NAME" | "CITEXT" => row
                    .try_get::<String, _>(name)
                    .map_or(Value::Null, |v| json!(v)),
                "INT2" | "INT4" | "INT8" | "OID" => row
                    .try_get::<i64, _>(name)
                    .map_or(Value::Null, |v| json!(v)),
                "FLOAT4" | "FLOAT8" | "NUMERIC" => row
                    .try_get::<f64, _>(name)
                    .map_or(Value::Null, |v| json!(v)),
                "BOOL" => row
                    .try_get::<bool, _>(name)
                    .map_or(Value::Null, |v| json!(v)),
                "UUID" => row
                    .try_get::<uuid::Uuid, _>(name)
                    .map(|v| json!(v.to_string()))
                    .unwrap_or(Value::Null),
                "JSON" | "JSONB" => row.try_get::<Value, _>(name).unwrap_or(Value::Null),
                "DATE" => row
                    .try_get::<chrono::NaiveDate, _>(name)
                    .map(|v| json!(v.to_string()))
                    .unwrap_or(Value::Null),
                "TIMESTAMP" | "TIMESTAMPTZ" => row
                    .try_get::<chrono::NaiveDateTime, _>(name)
                    .map(|v| json!(v.to_string()))
                    .unwrap_or(Value::Null),
                "BYTEA" => row
                    .try_get::<Vec<u8>, _>(name)
                    .map(|v| json!(base64::engine::general_purpose::STANDARD.encode(v)))
                    .unwrap_or(Value::Null),
                _ => row
                    .try_get::<String, _>(name)
                    .map_or(Value::Null, |v| json!(v)),
            };
            obj.insert(name.to_string(), json!(value));
        }
        out.push(Value::Object(obj));
    }
    out
}
