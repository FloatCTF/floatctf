use crate::api::dto::map_dto_vec;

use super::scheduled_tasks_dto::ScheduledTasksDto;
use crate::{
    api::{FilterMapping, dto::DeleteItemsRequest, prelude::*, sea_orm_utils::query_query},
    entity::scheduled_tasks,
};
use sea_orm::Condition;
use std::str::FromStr;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateScheduledTaskRequest {
    pub task_name: String,
    pub task_key: String,
    pub trigger_type: String,
    pub group_id: Option<Uuid>,
    pub cron_expr: Option<String>,
    pub execute_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub expires_at: Option<chrono::DateTime<chrono::FixedOffset>>,
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    pub description: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub protected: bool,
}

/// POST /api/admin/scheduled_tasks
#[post("")]
pub async fn create_scheduled_task(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    ctr: Json<CreateScheduledTaskRequest>,
) -> UniResult<ScheduledTasksDto> {
    let user = user.into_inner();
    let ctr = ctr.into_inner();
    let now = Utc::now();

    let new_task = scheduled_tasks::ActiveModel {
        task_name: Set(ctr.task_name),
        task_key: Set(ctr.task_key),
        trigger_type: Set(ctr.trigger_type),
        group_id: Set(ctr.group_id),
        cron_expr: Set(ctr.cron_expr),
        execute_at: Set(ctr.execute_at.map(|t| t.into())),
        expires_at: Set(ctr.expires_at.map(|t| t.into())),
        payload: Set(ctr.payload),
        description: Set(ctr.description),
        enabled: Set(ctr.enabled),
        protected: Set(ctr.protected),
        status: Set("pending".to_string()),
        created_at: Set(now.into()),
        updated_at: Set(now.into()),
        ..Default::default()
    };

    let task = new_task.insert(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "SCHEDULED_TASKS",
            "CREATE",
            format!("{} 创建定时任务: {}", user.username, task.task_name).as_str(),
            json!({"task_name": task.task_name, "task_key": task.task_key}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(task.into())).into()
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PatchScheduledTaskRequest {
    pub task_name: Option<String>,
    pub task_key: Option<String>,
    pub trigger_type: Option<String>,
    pub group_id: Option<Option<Uuid>>,
    pub cron_expr: Option<Option<String>>,
    pub execute_at: Option<Option<chrono::DateTime<chrono::FixedOffset>>>,
    pub expires_at: Option<Option<chrono::DateTime<chrono::FixedOffset>>>,
    pub payload: Option<Option<serde_json::Value>>,
    pub description: Option<Option<String>>,
    pub enabled: Option<bool>,
    pub protected: Option<bool>,
    pub status: Option<String>,
    pub error_msg: Option<Option<String>>,
}

/// PATCH /api/admin/scheduled_tasks/{task_id}
#[patch("/{task_id}")]
pub async fn patch_scheduled_task(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    task_id: Path<Uuid>,
    ptr: Json<PatchScheduledTaskRequest>,
) -> UniResult<ScheduledTasksDto> {
    let user = user.into_inner();
    let task_id = task_id.into_inner();
    let ptr = ptr.into_inner();

    let task = scheduled_tasks::Entity::find_by_id(task_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", task_id)))?;

    let mut m_task = task.into_active_model();

    if let Some(task_name) = ptr.task_name {
        m_task.task_name = Set(task_name);
    }
    if let Some(task_key) = ptr.task_key {
        m_task.task_key = Set(task_key);
    }
    if let Some(trigger_type) = ptr.trigger_type {
        m_task.trigger_type = Set(trigger_type);
    }
    if let Some(group_id) = ptr.group_id {
        m_task.group_id = Set(group_id);
    }
    if let Some(cron_expr) = ptr.cron_expr {
        m_task.cron_expr = Set(cron_expr);
    }
    if let Some(execute_at) = ptr.execute_at {
        m_task.execute_at = Set(execute_at.map(|t| t.into()));
    }
    if let Some(expires_at) = ptr.expires_at {
        m_task.expires_at = Set(expires_at.map(|t| t.into()));
    }
    if let Some(payload) = ptr.payload {
        m_task.payload = Set(payload);
    }
    if let Some(description) = ptr.description {
        m_task.description = Set(description);
    }
    if let Some(enabled) = ptr.enabled {
        m_task.enabled = Set(enabled);
    }
    if let Some(protected) = ptr.protected {
        m_task.protected = Set(protected);
    }
    if let Some(status) = ptr.status {
        m_task.status = Set(status);
    }
    if let Some(error_msg) = ptr.error_msg {
        m_task.error_msg = Set(error_msg);
    }

    m_task.updated_at = Set(Utc::now().into());

    let task = m_task.update(ctx.db.get_ref()).await?;
    UniResponse::ok(Some(task.into())).into()
}

/// GET /api/admin/scheduled_tasks
#[get("")]
pub async fn get_scheduled_tasks(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    query_params: Query<QueryParams>,
) -> UniResult<Vec<ScheduledTasksDto>> {
    let mut query_params = query_params.0;

    let mappings = [
        FilterMapping {
            key: "id",
            column: Box::new(|v| {
                Condition::all()
                    .add(scheduled_tasks::Column::Id.eq(Uuid::from_str(&v).unwrap_or(Uuid::nil())))
            }),
        },
        FilterMapping {
            key: "task_name",
            column: Box::new(|v| {
                Condition::all().add(scheduled_tasks::Column::TaskName.contains(v))
            }),
        },
        FilterMapping {
            key: "task_key",
            column: Box::new(|v| {
                Condition::all().add(scheduled_tasks::Column::TaskKey.contains(v))
            }),
        },
        FilterMapping {
            key: "trigger_type",
            column: Box::new(|v| {
                Condition::all().add(scheduled_tasks::Column::TriggerType.eq(v.to_string()))
            }),
        },
        FilterMapping {
            key: "status",
            column: Box::new(|v| {
                Condition::all().add(scheduled_tasks::Column::Status.eq(v.to_string()))
            }),
        },
        FilterMapping {
            key: "enabled",
            column: Box::new(|v| {
                Condition::all()
                    .add(scheduled_tasks::Column::Enabled.eq(v.parse::<bool>().unwrap_or(false)))
            }),
        },
        FilterMapping {
            key: "protected",
            column: Box::new(|v| {
                Condition::all()
                    .add(scheduled_tasks::Column::Protected.eq(v.parse::<bool>().unwrap_or(false)))
            }),
        },
    ];

    let (items, total_items) = query_query::<scheduled_tasks::Entity>(
        ctx.db.get_ref(),
        &mappings,
        &query_params,
        Some(Box::new(|stmt| {
            stmt.order_by_desc(scheduled_tasks::Column::UpdatedAt)
        })),
    )
    .await?;

    query_params.total = Some(total_items);

    UniResponse::ok_meta(Some(map_dto_vec(items)), query_params.into()).into()
}

/// GET /api/admin/scheduled_tasks/{task_id}
#[get("/{task_id}")]
pub async fn get_scheduled_task(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    task_id: Path<Uuid>,
) -> UniResult<ScheduledTasksDto> {
    let task_id = task_id.into_inner();
    let model = scheduled_tasks::Entity::find_by_id(task_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", task_id)))?;

    UniResponse::ok(Some(model.into())).into()
}

/// POST /api/admin/scheduled_tasks/{task_id}/run
#[post("/{task_id}/run")]
pub async fn run_scheduled_task(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    task_id: Path<Uuid>,
) -> UniResult<ScheduledTasksDto> {
    let user = user.into_inner();
    let task_id = task_id.into_inner();

    let task = scheduled_tasks::Entity::find_by_id(task_id)
        .one(ctx.db.get_ref())
        .await?
        .ok_or(AppError::NotFound(format!(" {} not exist", task_id)))?;

    let mut m_task = task.into_active_model();
    m_task.status = Set("pending".to_string());
    m_task.execute_at = Set(Some(Utc::now().into()));
    m_task.updated_at = Set(Utc::now().into());

    let task = m_task.update(ctx.db.get_ref()).await?;

    ctx.log
        .add_log(
            "INFO",
            "SCHEDULED_TASKS",
            "RUN",
            format!("{} 手动执行定时任务: {}", user.username, task.task_name).as_str(),
            json!({"task_id": task.id}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(task.into())).into()
}

/// DELETE /api/admin/scheduled_tasks
#[delete("")]
pub async fn delete_scheduled_task(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    dir: Json<DeleteItemsRequest>,
) -> UniResult<u64> {
    let user = user.into_inner();
    let dir = dir.into_inner();
    let mut deleted_count = 0;
    for task_id in dir.id_list {
        let task = scheduled_tasks::Entity::find_by_id(task_id)
            .one(ctx.db.get_ref())
            .await?;
        if let Some(task) = task {
            if task.protected {
                return Err(AppError::BadRequest(format!(
                    "protected scheduled task can not be deleted: {}",
                    task.task_name
                )));
            }
            let r = task.delete(ctx.db.get_ref()).await?;
            deleted_count += r.rows_affected;
        }
    }

    ctx.log
        .add_log(
            "INFO",
            "SCHEDULED_TASKS",
            "DELETE",
            format!("{} 删除 {} 个定时任务", user.username, deleted_count).as_str(),
            json!({"deleted_count": deleted_count}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(deleted_count.into()).into()
}
