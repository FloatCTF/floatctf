//! 管理端赛事应用服务。
//!
//! 业务逻辑自 `api/admin/events` HTTP 处理器抽出。
//! 处理器保留鉴权、日志副作用与 UniResponse 包装。

use std::io::Write;
use std::str::FromStr;

use aws_sdk_s3::primitives::ByteStream;
use chrono::{DateTime, FixedOffset};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait,
    DatabaseConnection, EntityTrait, IntoActiveModel, ModelTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zip::write::FileOptions;

use crate::{
    api::{AppError, FilterMapping, prelude::*},
    entity::{
        awd_events, challenges, event_team_members, event_teams, event_users, event_writeup,
        events, jeopardy_challenge_solves, jeopardy_event_challenges,
        sea_orm_active_enums::{EventFamily, EventPurpose, EventTeamMemberRole, ParticipantMode},
        users,
    },
    infrastructure::{WebDb, WebRustfs},
    modules::event::common::application::player_service::{get_scoreboard, get_trend},
    modules::event::common::domain::event_mode::EventMode,
    modules::event::jeopardy::domain::scoring::calculate_next_dynamic_score,
    modules::event::jeopardy::domain::{scoreboard::ScoreboardItem, trend::TrendItem},
};

fn generate_safe_name(original: &str) -> String {
    original
        .chars()
        .map(|c| {
            if c.is_ascii() {
                if c.is_ascii_alphanumeric() || c == ' ' || c == '.' || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            } else {
                c
            }
        })
        .collect()
}

// ── Request / response DTOs ───────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateEventRequest {
    pub family: EventFamily,
    pub participant_mode: ParticipantMode,
    /// 用途：默认 Competition；awdp family 允许显式 Practice（有界练习，end_time 必填）。
    #[serde(default)]
    pub purpose: Option<EventPurpose>,
    pub title: String,
    pub description: Option<String>,
    pub hidden: bool,
    pub allow_join: bool,
    pub rules: String,
    pub flag_prefix: Option<String>,
    pub start_time: DateTime<FixedOffset>,
    pub end_time: DateTime<FixedOffset>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchEventRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub hidden: Option<bool>,
    pub allow_join: Option<bool>,
    pub rules: Option<String>,
    pub flag_prefix: Option<String>,
    pub start_time: Option<DateTime<FixedOffset>>,
    pub end_time: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataEventChallenge {
    pub name: String,
    pub category: String,
    pub points: f64,
    pub solved_count: u64,
    pub solved_percent: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataEventChallengeSolve {
    pub user_nickname: String,
    pub challenge_name: String,
    pub challenge_category: String,
    pub created_at: DateTime<FixedOffset>,
    pub bonus_points: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DataPresent {
    pub event: events::Model,
    pub user_count: u64,
    pub team_count: u64,
    pub solved_recent_15: Vec<DataEventChallengeSolve>,
    pub event_challenges: Vec<DataEventChallenge>,
    pub scoreboard_top10: Vec<ScoreboardItem>,
    pub trend: Vec<TrendItem>,
}

/// 嵌入 Writeup 报告的战队成员行（与管理端 event_teams DTO 对齐）。
#[derive(Debug, Serialize, Deserialize)]
pub struct ReportTeamMember {
    pub username: String,
    pub nickname: String,
    pub role: EventTeamMemberRole,
    pub points: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportTeam {
    pub team: event_teams::Model,
    pub writeup_url: String,
    pub members: Vec<ReportTeamMember>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportUser {
    pub username: String,
    pub nickname: String,
    pub points: f64,
    pub writeup_url: String,
    pub banned: bool,
}

// ── CRUD ──────────────────────────────────────────────────────────────────

pub async fn create_event(
    db: &DatabaseConnection,
    req: CreateEventRequest,
) -> Result<events::Model, AppError> {
    // 普通赛事创建固定 Competition（Practice 仅系统托管 / AWDP 显式允许）。
    // AWDP practice 是有界练习（end_time 必填，与 break+fix 时长一致由 awdp 配置同步）。
    let purpose = match req.purpose {
        Some(p) if req.family == EventFamily::Awdp => p,
        Some(_) => {
            return Err(AppError::Validation(
                "purpose 仅 awdp family 允许显式指定（其余固定 competition）".into(),
            ));
        }
        None => EventPurpose::Competition,
    };
    let mode = EventMode::new(
        req.family.clone(),
        purpose.clone(),
        req.participant_mode.clone(),
    )
    .map_err(|e| AppError::Validation(e.to_string()))?;
    if req.start_time >= req.end_time {
        return Err(AppError::Validation(
            "event start_time must be before end_time".into(),
        ));
    }
    let new_event = events::ActiveModel {
        is_virtual: Set(false),
        family: Set(mode.family),
        purpose: Set(mode.purpose),
        participant_mode: Set(mode.participant_mode),
        system_key: Set(None),
        title: Set(req.title),
        description: Set(req.description),
        start_time: Set(req.start_time),
        hidden: Set(req.hidden),
        allow_join: Set(req.allow_join),
        end_time: Set(Some(req.end_time)),
        flag_prefix: Set(req.flag_prefix),
        rules: Set(req.rules),
        ..Default::default()
    };
    Ok(new_event.insert(db).await?)
}

pub async fn patch_event<C>(
    db: &C,
    event_id: Uuid,
    req: PatchEventRequest,
) -> Result<events::Model, AppError>
where
    C: ConnectionTrait + TransactionTrait + Send,
{
    use sea_orm::sea_query::LockType;

    let txn = db.begin().await?;
    // 与 AWD Configure 共用 events → awd_events → scheduled_tasks 锁序。
    let event = events::Entity::find_by_id(event_id)
        .lock(LockType::Update)
        .one(&txn)
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;
    if event.system_key.is_some() {
        return Err(AppError::Validation(
            "SystemManagedEvent: system-managed events cannot be patched via ordinary admin API"
                .into(),
        ));
    }
    let schedule_may_change = req.start_time.is_some() || req.end_time.is_some();
    let mut m_event = event.into_active_model();

    if let Some(t) = req.title {
        m_event.title = Set(t);
    }
    if let Some(d) = req.description {
        m_event.description = Set(d.into());
    }
    if let Some(s) = req.start_time {
        m_event.start_time = Set(s);
    }
    if let Some(e) = req.end_time {
        m_event.end_time = Set(Some(e));
    }
    if let Some(h) = req.hidden {
        m_event.hidden = Set(h);
    }
    if let Some(a) = req.allow_join {
        m_event.allow_join = Set(a);
    }
    if let Some(r) = req.rules {
        m_event.rules = Set(r.into());
    }
    if let Some(f) = req.flag_prefix {
        m_event.flag_prefix = Set(f.into());
    }

    let updated = m_event.update(&txn).await?;
    if updated.purpose == EventPurpose::Competition {
        let end = updated
            .end_time
            .ok_or_else(|| AppError::Validation("competition event end_time is required".into()))?;
        if updated.start_time >= end {
            return Err(AppError::Validation(
                "event start_time must be before end_time".into(),
            ));
        }
    }

    if schedule_may_change {
        let awd_configured = awd_events::Entity::find()
            .filter(awd_events::Column::EventId.eq(event_id))
            .lock(LockType::Update)
            .one(&txn)
            .await?
            .is_some();
        if awd_configured {
            let planned_start =
                crate::modules::event::awd::scheduler::find_event_start_schedule(&txn, event_id)
                    .await?;
            let end = updated.end_time.ok_or_else(|| {
                AppError::Validation("competition event end_time is required".into())
            })?;
            if planned_start.is_some_and(|start_at| start_at >= end) {
                return Err(AppError::Validation(
                    "event end_time must be after the AWD planned_start_at".into(),
                ));
            }
            let effective_start = planned_start.unwrap_or(updated.start_time);
            crate::modules::event::awd::scheduler::replace_auto_precheck_schedule(
                &txn,
                event_id,
                effective_start,
                chrono::Utc::now(),
            )
            .await
            .map_err(AppError::from)?;
        }
    }

    txn.commit().await?;
    Ok(updated)
}

pub async fn get_event(db: &DatabaseConnection, event_id: Uuid) -> Result<events::Model, AppError> {
    events::Entity::find_by_id(event_id)
        .one(db)
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))
}

pub async fn delete_events(db: &DatabaseConnection, id_list: Vec<Uuid>) -> Result<u64, AppError> {
    let system_count = events::Entity::find()
        .filter(events::Column::Id.is_in(id_list.clone()))
        .filter(events::Column::SystemKey.is_not_null())
        .count(db)
        .await?;
    if system_count > 0 {
        return Err(AppError::Validation(
            "system-managed events (system_key set) cannot be deleted".into(),
        ));
    }
    let deleted = events::Entity::delete_many()
        .filter(events::Column::Id.is_in(id_list))
        .exec(db)
        .await?
        .rows_affected;
    Ok(deleted)
}

pub fn admin_event_filter_mappings() -> [FilterMapping; 8] {
    [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(events::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "family",
            column: Box::new(|v| {
                Condition::all().add(
                    events::Column::Family
                        .eq(serde_json::from_str(v).unwrap_or(EventFamily::Jeopardy)),
                )
            }),
        },
        FilterMapping {
            key: "purpose",
            column: Box::new(|v| {
                Condition::all().add(
                    events::Column::Purpose
                        .eq(serde_json::from_str(v).unwrap_or(EventPurpose::Competition)),
                )
            }),
        },
        FilterMapping {
            key: "participant_mode",
            column: Box::new(|v| {
                Condition::all().add(
                    events::Column::ParticipantMode
                        .eq(serde_json::from_str(v).unwrap_or(ParticipantMode::Individual)),
                )
            }),
        },
        FilterMapping {
            key: "title",
            column: Box::new(|v| Condition::all().add(events::Column::Title.contains(v))),
        },
        FilterMapping {
            key: "hidden",
            column: Box::new(|v| {
                Condition::all().add(events::Column::Hidden.eq(v.parse::<bool>().unwrap_or(true)))
            }),
        },
        FilterMapping {
            key: "is_virtual",
            column: Box::new(|v| {
                Condition::all()
                    .add(events::Column::IsVirtual.eq(v.parse::<bool>().unwrap_or(false)))
            }),
        },
        FilterMapping {
            key: "allow_join",
            column: Box::new(|v| {
                Condition::all()
                    .add(events::Column::AllowJoin.eq(v.parse::<bool>().unwrap_or(false)))
            }),
        },
    ]
}

// ── Dashboard data ────────────────────────────────────────────────────────

pub async fn get_data_present(db: WebDb, event_id: Uuid) -> Result<DataPresent, AppError> {
    let event = events::Entity::find_by_id(event_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", event_id)))?;

    let user_count = event_users::Entity::find()
        .filter(event_users::Column::EventId.eq(event_id))
        .count(db.get_ref())
        .await?;

    let team_count = {
        if event.participant_mode == ParticipantMode::Team {
            event_teams::Entity::find()
                .filter(event_teams::Column::EventId.eq(event_id))
                .count(db.get_ref())
                .await?
        } else {
            0
        }
    };

    let solved_recent_15 = jeopardy_challenge_solves::Entity::find()
        .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
        .order_by_desc(jeopardy_challenge_solves::Column::CreatedAt)
        .limit(15)
        .find_also_related(users::Entity)
        .find_also_related(challenges::Entity)
        .all(db.get_ref())
        .await?
        .into_iter()
        .map(|(solve, user, challenge)| DataEventChallengeSolve {
            user_nickname: user.map(|u| u.nickname).unwrap_or_default(),
            challenge_name: challenge.clone().map(|c| c.name).unwrap_or_default(),
            challenge_category: challenge.map(|c| c.category).unwrap_or_default(),
            created_at: solve.created_at,
            bonus_points: solve.bonus_points,
        })
        .collect::<Vec<_>>();

    let event_challenges_rows = jeopardy_event_challenges::Entity::find()
        .filter(jeopardy_event_challenges::Column::EventId.eq(event_id))
        .find_also_related(challenges::Entity)
        .all(db.get_ref())
        .await?;

    let mut data_event_challenges = Vec::new();
    for (event_challenge, challenge) in event_challenges_rows {
        let solved_count = jeopardy_challenge_solves::Entity::find()
            .filter(jeopardy_challenge_solves::Column::EventId.eq(event_id))
            .filter(jeopardy_challenge_solves::Column::ChallengeId.eq(event_challenge.challenge_id))
            .count(db.get_ref())
            .await?;

        let solved_percent = {
            if event.participant_mode == ParticipantMode::Team {
                if team_count == 0 {
                    0.0
                } else {
                    solved_count as f64 / team_count as f64
                }
            } else if user_count == 0 {
                0.0
            } else {
                solved_count as f64 / user_count as f64
            }
        };

        let points =
            calculate_next_dynamic_score(db.get_ref(), event_challenge.points, solved_count)
                .await
                .map_err(|e| {
                    AppError::BadRequest(format!("calculate_next_dynamic_score error: {}", e))
                })?;

        data_event_challenges.push(DataEventChallenge {
            name: challenge.clone().map(|c| c.name).unwrap_or_default(),
            category: challenge.map(|c| c.category).unwrap_or_default(),
            points,
            solved_count,
            solved_percent,
        });
    }
    data_event_challenges.sort_by(|a, b| b.solved_count.cmp(&a.solved_count));

    let scoreboard = get_scoreboard(db.clone(), event_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("{}", e)))?;

    let trend_items = get_trend(db, event_id)
        .await
        .map_err(|e| AppError::BadRequest(format!("{}", e)))?;

    let scoreboard_top10 = scoreboard.into_iter().take(10).collect::<Vec<_>>();

    Ok(DataPresent {
        event,
        user_count,
        team_count,
        solved_recent_15,
        event_challenges: data_event_challenges,
        scoreboard_top10,
        trend: trend_items,
    })
}

// ── Writeup report zip ────────────────────────────────────────────────────

const REPORT_TEMPLATE: &str = r#"
<html lang="zh-CN">
<head>
  <style>
    body {
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI",
        "Microsoft YaHei", "Helvetica Neue", Helvetica, Arial, sans-serif;
      line-height: 1.6;
      color: #333;
      max-width: 800px;
      margin: 20px auto;
      padding: 0 20px;
    }
    h1,
    h2,
    h3 {
      border-bottom: 1px solid #eaecef;
      padding-bottom: 0.3em;
    }
    h1 {
      font-size: 2em;
    }
    h2 {
      font-size: 1.5em;
    }
    h3 {
      font-size: 1.25em;
    }
    code {
      font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo,
        Courier, monospace;
      background-color: rgba(27, 31, 35, 0.05);
      padding: 0.2em 0.4em;
      font-size: 85%;
      border-radius: 3px;
    }
    table {
      width: 100%;   /* 跟随整个浏览器宽度 */
      max-width: 100%;
      border-collapse: collapse;
      margin-top: 1em;
    }
    th,
    td {
      border: 1px solid #ddd;
      padding: 0.6em;
      text-align: left;
    }
    thead {
      background-color: #f3f3f3;
    }
  </style>
  <meta http-equiv="Content-Type" content="text/html; charset=utf-8" />
  <meta charset="utf-8" />
  <title>{{ event.title }}' Writeup Report</title>
</head>
<body>
  <h1>{{ event.title }}' Writeup Report</h1>
  <p>Event ID：<code>{{ event.id }}</code></p>
  <p>Event Mode：<code>{{ event.family }} / {{ event.purpose }} / {{ event.participant_mode }}</code></p>
  <p> Event Date：<code>{{ event.start_time }} - {{ event.end_time }}</code>
  </p> {% if event_teams_results %} <h2>Event Teams</h2>
  <table>
    <thead>
      <tr>
        <th>No.</th>
        <th>Team ID</th>
        <th>Name</th>
        <th>Points</th>
        <th>Member</th>
        <th>Writeup</th>
        <th>banned</th>
      </tr>
    </thead>
    <tbody> {% for team_result in event_teams_results %} <tr>
        <td>{{ loop.index }}</td>
        <td>{{ team_result.team.id}}</td>
        <td>{{ team_result.team.name }}</td>
        <td>{{ team_result.team.points }}</td>
        <td>
          <table>
            <thead>
              <tr>
                <th>Username</th>
                <th>Nickname</th>
                <th>Role</th>
                <th>Points</th>
              </tr>
            </thead>
            <tbody> {% for member in team_result.members%} <tr>
                <td>{{ member.username }}</td>
                <td>{{ member.nickname }}</td>
                <td>{{ member.role }}</td>
                <td>{{ member.points }}</td>
              </tr> {% endfor %} </tbody>
          </table>
        </td>
        <td><a href="{{ team_result.writeup_url }}" target="_blank">{{ team_result.writeup_url }}</a></td>
        <td>{{ team_result.team.banned }}</td>
      </tr> {% endfor %} </tbody>
  </table> {% endif %} {% if event_users %} <h2>Event users::Entity</h2>
  <table>
    <thead>
      <tr>
        <th>No.</th>
        <th>Username</th>
        <th>Nickname</th>
        <th>Points</th>
        <th>Writeup</th>
        <th>Banned</th>
      </tr>
    </thead>
    <tbody> {% for user in event_users %} <tr>
        <td>{{ loop.index }}</td>
        <td>{{ user.username }}</td>
        <td>{{ user.nickname }}</td>
        <td>{{ user.points }}</td>
        <td><a href="{{ user.writeup_url }}" target="_blank">{{ user.writeup_url }}</a></td>
        <td>{{ user.banned }}</td>
      </tr> {% endfor %} </tbody>
  </table> {% endif %}
</html>
"#;

/// 生成 Writeup 报告 zip，上传对象存储，返回对象键。
pub async fn export_writeup_report(
    db: &WebDb,
    rustfs: &WebRustfs,
    event_id: Uuid,
) -> Result<(events::Model, String), AppError> {
    let event = events::Entity::find_by_id(event_id)
        .one(db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!("Event {} not exist", event_id)))?;

    let event_writeups = event_writeup::Entity::find()
        .filter(event_writeup::Column::EventId.eq(event_id))
        .all(db.get_ref())
        .await?;

    let mut zip_buffer = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(std::io::Write::by_ref(&mut zip_buffer));

        for writeup in event_writeups {
            let s3_key = writeup.file_url;
            let obj = rustfs
                .get_object()
                .bucket("floatctf-private")
                .key(&s3_key)
                .send()
                .await
                .map_err(|e| {
                    AppError::BadRequest(format!("Failed to get writeup from S3: {}", e))
                })?;

            let body = obj
                .body
                .collect()
                .await
                .map_err(|e| AppError::BadRequest(format!("Failed to read S3 body: {}", e)))?;
            let file_bytes: Vec<u8> = body.to_vec();

            zip.start_file(&s3_key, FileOptions::<()>::default())
                .map_err(|e| AppError::BadRequest(e.to_string()))?;
            zip.write_all(&file_bytes)
                .map_err(|e| AppError::BadRequest(e.to_string()))?;
        }

        let env = minijinja::Environment::new();
        let tmpl = env
            .template_from_str(REPORT_TEMPLATE)
            .map_err(|e| AppError::BadRequest(format!("Failed to create template: {}", e)))?;

        let ctx = match event.participant_mode {
            ParticipantMode::Individual => {
                let event_users_rows = event_users::Entity::find()
                    .filter(event_users::Column::EventId.eq(event_id))
                    .find_also_related(users::Entity)
                    .all(db.get_ref())
                    .await?;
                let event_users_results = {
                    let mut event_users_results = Vec::new();
                    let mut has_writeup = false;

                    for (event_user, user) in event_users_rows {
                        if let Some(user) = user {
                            let writeup = event_writeup::Entity::find()
                                .filter(event_writeup::Column::UserId.eq(user.id))
                                .one(db.get_ref())
                                .await?;

                            if writeup.is_some() {
                                has_writeup = true;
                            }

                            event_users_results.push(ReportUser {
                                username: user.username,
                                nickname: user.nickname,
                                points: event_user.points,
                                writeup_url: writeup.map(|w| w.file_url).unwrap_or_default(),
                                banned: event_user.banned,
                            });
                        }
                    }

                    if has_writeup {
                        event_users_results.retain(|u| !u.writeup_url.is_empty());
                    }

                    event_users_results.sort_by(|a, b| b.points.partial_cmp(&a.points).unwrap());
                    event_users_results
                };

                minijinja::context! {
                    event,
                    event_users => event_users_results,
                }
            }

            ParticipantMode::Team => {
                let event_teams_rows = event_teams::Entity::find()
                    .inner_join(event_writeup::Entity)
                    .filter(event_writeup::Column::EventId.eq(event_id))
                    .all(db.get_ref())
                    .await?;
                let event_teams_results = {
                    let mut event_teams_results = Vec::new();
                    for team in event_teams_rows {
                        let members = team
                            .find_related(event_team_members::Entity)
                            .find_also_related(users::Entity)
                            .all(db.get_ref())
                            .await?;
                        let mut team_members = Vec::new();

                        for (member, user) in members {
                            if let Some(user) = user {
                                let event_user = event_users::Entity::find()
                                    .filter(event_users::Column::EventId.eq(event.id))
                                    .filter(event_users::Column::UserId.eq(user.id))
                                    .one(db.get_ref())
                                    .await?
                                    .ok_or(AppError::NotFound(format!(
                                        "EventUser {} not exist",
                                        user.id
                                    )))?;

                                team_members.push(ReportTeamMember {
                                    username: user.username,
                                    nickname: user.nickname,
                                    role: member.role,
                                    points: event_user.points,
                                });
                            }
                        }

                        let writeup = event_writeup::Entity::find()
                            .filter(event_writeup::Column::TeamId.eq(team.id))
                            .one(db.get_ref())
                            .await?;
                        let writeup_url = writeup.map(|w| w.file_url).unwrap_or_default();
                        event_teams_results.push(ReportTeam {
                            team,
                            writeup_url,
                            members: team_members,
                        });
                    }
                    event_teams_results
                };
                minijinja::context! {
                    event,
                    event_teams_results,
                }
            }
            _ => minijinja::context! {
                event,
            },
        };
        let rendered = tmpl
            .render(ctx)
            .map_err(|e| AppError::BadRequest(format!("Failed to render template: {}", e)))?;
        zip.start_file("report.html", FileOptions::<()>::default())
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        zip.write_all(rendered.as_bytes())
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
        zip.finish()
            .map_err(|e| AppError::BadRequest(e.to_string()))?;
    }

    let s3_key = format!(
        "writeups/{}/{}_{}.zip",
        event_id,
        generate_safe_name(&event.title),
        event_id
    );

    let body = ByteStream::from(zip_buffer.into_inner());
    rustfs
        .put_object()
        .bucket("floatctf-private")
        .key(&s3_key)
        .body(body)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to upload report to S3: {}", e)))?;

    Ok((event, s3_key))
}
