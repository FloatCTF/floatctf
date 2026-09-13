use crate::api::prelude::*;

use crate::{
    entity::{event_team_members, event_teams, event_users, event_writeup, events},
    modules::event::{
        common::domain::practice_event::require_practice_jeopardy_event,
        jeopardy::application::{
            context::{EventContextBuilder, SubmitFlagRequest as ModeSubmitFlag},
            submit as jeopardy_submit,
        },
    },
};
use actix_multipart::form::{MultipartForm, tempfile::TempFile, text::Text};
use aws_sdk_s3::primitives::ByteStream;
use tokio::io::AsyncReadExt;

#[derive(Debug, Deserialize, Serialize)]
pub struct SubmitFlagRequest {
    pub event_id: Option<Uuid>,
    // single
    pub instance_id: Option<Uuid>,
    // value
    pub flag: String,
}

/// POST /api/submit/flag
#[post("/flag")]
pub async fn submit_flag(
    user: UserJwtGuard,
    ctx: ReqCtx,
    sfr: Json<SubmitFlagRequest>,
) -> UniResult<()> {
    let user = user.into_inner();
    let mut sfr = sfr.into_inner();
    sfr.flag = sfr.flag.trim().to_string();

    // 练习提交可省略 event_id；显式解析系统练习赛事（Context 不再自动回落）。
    let event = match sfr.event_id {
        Some(event_id) => events::Entity::find_by_id(event_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound("no event".into()))?,
        None => require_practice_jeopardy_event(ctx.db.get_ref())
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?,
    };

    let event_ctx = EventContextBuilder::new()
        .db(ctx.db)
        .docker(ctx.docker)
        .user(user)
        .event(event)
        .resolve_team()
        .build()
        .await
        .map_err(|e| AppError::BadRequest(format!("build event context error: {}", e)))?;

    jeopardy_submit::submit_flag(
        &event_ctx,
        ModeSubmitFlag {
            instance_id: sfr.instance_id,
            flag: sfr.flag.clone(),
        },
    )
    .await
    .map_err(|e| AppError::BadRequest(format!("submit flag error: {}", e)))?;

    if let Some(_event_id) = sfr.event_id {
        ctx.log
            .add_event_log(
                &event_ctx.event,
                "INFO",
                "SUBMIT_FLAG",
                json!({"instance_id": sfr.instance_id, "accepted": true}),
                Some(event_ctx.user.id),
                event_ctx.team.as_ref().map(|t| t.id),
                Some(&ctx.req),
            )
            .await;
    } else {
        ctx.log
            .add_log(
                "INFO",
                "SUBMIT",
                "SUBMIT_FLAG",
                "提交 Flag 成功",
                json!({"instance_id": sfr.instance_id, "accepted": true}),
                event_ctx.user.id.into(),
                None,
                Some(&ctx.req),
            )
            .await;
    }

    UniResponse::ok_none().into()
}

#[derive(Debug, MultipartForm)]
pub struct WriteupForm {
    #[multipart(limit = "50MB")]
    writeup_pdf: TempFile,
    event_id: Text<Uuid>,
    /// 兼容旧前端。授权主体始终由服务端根据 event_id + current user 解析。
    team_id: Option<Text<Uuid>>,
}

// now just for the event
/// POST /api/submit/writeup
#[post("writeup")]
pub async fn submit_writeup(
    user: UserJwtGuard,
    ctx: ReqCtx,
    MultipartForm(form): MultipartForm<WriteupForm>,
) -> UniResult<()> {
    let user = user.into_inner();

    let event_id = form.event_id.into_inner();
    let requested_team_id = form.team_id.map(|x| x.into_inner());

    // 所有权只由服务端解析，避免客户端伪造 team_id 覆盖其他队伍对象。
    let event = events::Entity::find_by_id(event_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound("event not found".to_string()))?;
    if event.purpose != crate::entity::sea_orm_active_enums::EventPurpose::Competition {
        return Err(AppError::BadRequest(
            "Writeup submission is only available for competition events".into(),
        ));
    }

    let event_user = event_users::Entity::find_by_id((event_id, user.id))
        .one(ctx.db.get_ref())
        .await?
        .ok_or_else(|| AppError::Forbidden("User has not joined this event".into()))?;
    if event_user.banned {
        return Err(AppError::Forbidden("User is banned from this event".into()));
    }

    let (team_id, writeup_file_name) = if event.participant_mode
        == crate::entity::sea_orm_active_enums::ParticipantMode::Team
    {
        let membership = event_team_members::Entity::find()
            .filter(event_team_members::Column::EventId.eq(event_id))
            .filter(event_team_members::Column::UserId.eq(user.id))
            .one(ctx.db.get_ref())
            .await?
            .ok_or_else(|| AppError::Forbidden("User is not a member of an event team".into()))?;

        if requested_team_id.is_some_and(|id| id != membership.team_id) {
            return Err(AppError::Forbidden(
                "Requested team does not match the authenticated user's event team".into(),
            ));
        }

        let team = event_teams::Entity::find_by_id(membership.team_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or_else(|| AppError::NotFound("event team not found".into()))?;
        if team.event_id != event_id {
            return Err(AppError::Forbidden(
                "Team does not belong to this event".into(),
            ));
        }
        if team.banned {
            return Err(AppError::Forbidden("Team is banned from this event".into()));
        }

        (
            Some(team.id),
            format!("{}/{}/{}.pdf", event_id, team.id, team.name),
        )
    } else {
        if requested_team_id.is_some() {
            return Err(AppError::BadRequest(
                "team_id is invalid for an individual event".into(),
            ));
        }
        (
            None,
            format!("{}/{}/{}.pdf", event_id, user.id, user.nickname),
        )
    };

    let writeup_file = form.writeup_pdf;
    let path = writeup_file.file.path();

    // 只读取文件头做类型校验，避免把整个文件加载进内存。
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to open writeup file: {e}")))?;
    let mut magic = [0_u8; 5];
    file.read_exact(&mut magic)
        .await
        .map_err(|_| AppError::BadRequest("Writeup must be a valid PDF file".into()))?;
    if &magic != b"%PDF-" {
        return Err(AppError::BadRequest(
            "Writeup must be a valid PDF file".into(),
        ));
    }
    drop(file);

    let s3_key = format!("writeups/{writeup_file_name}");
    let body = ByteStream::from_path(path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to stream writeup file: {e}")))?;

    ctx.rustfs
        .put_object()
        .bucket("floatctf-private")
        .key(&s3_key)
        .body(body)
        .content_type("application/pdf")
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to upload writeup to S3: {}", e)))?;

    // 插入或更新数据库
    use sea_orm::sea_query::OnConflict;

    event_writeup::Entity::insert(event_writeup::ActiveModel {
        event_id: Set(event_id),
        user_id: Set(user.id),
        team_id: Set(team_id),
        file_url: Set(s3_key),
        ..Default::default()
    })
    .on_conflict(
        OnConflict::columns([
            event_writeup::Column::EventId,
            event_writeup::Column::UserId,
        ])
        .update_columns([
            event_writeup::Column::FileUrl,
            event_writeup::Column::TeamId,
        ])
        .to_owned(),
    )
    .exec(ctx.db.get_ref())
    .await?;

    ctx.log
        .add_event_log(
            &event,
            "INFO",
            "SUBMIT_WRITEUP",
            json!({"team_id": team_id}),
            Some(user.id),
            team_id,
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok_none().into()
}
