//! 赛事仓储：集中赛事相关数据库查询。
//!
//! 自分散的处理器级查询抽出，提供可复用的数据访问模式。
//! 处理器应调用本模块函数，避免各自手写 SeaORM 查询。

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};
use uuid::Uuid;

use crate::entity::{event_team_members, event_teams};

/// 用户在某赛事中的战队成员信息。
#[derive(Debug, Clone)]
pub struct TeamMembership {
    pub team_id: Uuid,
    pub role: String,
}

/// 查找用户在指定赛事中的战队成员关系。
///
/// 若用户属于该赛事某战队则返回 `Some(TeamMembership)`，否则 `None`。
pub async fn find_user_team_membership(
    db: &DatabaseConnection,
    event_id: Uuid,
    user_id: Uuid,
) -> Result<Option<TeamMembership>, sea_orm::DbErr> {
    let membership = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::UserId.eq(user_id))
        .one(db)
        .await?;

    Ok(membership.map(|m| TeamMembership {
        team_id: m.team_id,
        role: format!("{:?}", m.role),
    }))
}

/// 按赛事与名称查找战队。
pub async fn find_team_by_name(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_name: &str,
) -> Result<Option<event_teams::Model>, sea_orm::DbErr> {
    event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .filter(event_teams::Column::Name.eq(team_name))
        .one(db)
        .await
}

/// 检查战队名在该赛事中是否已被占用。
pub async fn is_team_name_taken(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_name: &str,
) -> Result<bool, sea_orm::DbErr> {
    let exists = event_teams::Entity::find()
        .filter(event_teams::Column::EventId.eq(event_id))
        .filter(event_teams::Column::Name.eq(team_name))
        .one(db)
        .await?;
    Ok(exists.is_some())
}

/// 统计某赛事中某战队的成员数。
pub async fn count_team_members(
    db: &DatabaseConnection,
    event_id: Uuid,
    team_id: Uuid,
) -> Result<i64, sea_orm::DbErr> {
    use sea_orm::{PaginatorTrait, QuerySelect};
    let count = event_team_members::Entity::find()
        .filter(event_team_members::Column::EventId.eq(event_id))
        .filter(event_team_members::Column::TeamId.eq(team_id))
        .count(db)
        .await?;
    Ok(count as i64)
}
