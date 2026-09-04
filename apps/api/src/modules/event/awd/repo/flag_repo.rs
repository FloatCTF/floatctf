//! Flag 发放/提交仓储。

use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, QueryFilter,
};
use uuid::Uuid;

use crate::entity::{awd_flag_issues, awd_flag_submissions};

// ── Flag Issues ──

pub async fn find_or_create_issue(
    db: &DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
    gamebox_instance_id: Uuid,
    flag_hash: &str,
) -> Result<awd_flag_issues::Model, sea_orm::DbErr> {
    let existing = awd_flag_issues::Entity::find()
        .filter(awd_flag_issues::Column::EventId.eq(event_id))
        .filter(awd_flag_issues::Column::RoundId.eq(round_id))
        .filter(awd_flag_issues::Column::GameboxInstanceId.eq(gamebox_instance_id))
        .one(db)
        .await?;

    if let Some(issue) = existing {
        return Ok(issue);
    }

    let model = awd_flag_issues::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round_id),
        gamebox_instance_id: Set(gamebox_instance_id),
        flag_hash: Set(flag_hash.to_string()),
        ..Default::default()
    };

    model.insert(db).await
}

pub async fn find_issue_by_id(
    db: &DatabaseConnection,
    id: Uuid,
) -> Result<Option<awd_flag_issues::Model>, sea_orm::DbErr> {
    awd_flag_issues::Entity::find_by_id(id).one(db).await
}

pub async fn find_issues_by_round(
    db: &DatabaseConnection,
    round_id: Uuid,
) -> Result<Vec<awd_flag_issues::Model>, sea_orm::DbErr> {
    awd_flag_issues::Entity::find()
        .filter(awd_flag_issues::Column::RoundId.eq(round_id))
        .all(db)
        .await
}

pub async fn find_issue_by_hash(
    db: &DatabaseConnection,
    event_id: Uuid,
    round_id: Uuid,
    flag_hash: &str,
) -> Result<Option<awd_flag_issues::Model>, sea_orm::DbErr> {
    awd_flag_issues::Entity::find()
        .filter(awd_flag_issues::Column::EventId.eq(event_id))
        .filter(awd_flag_issues::Column::RoundId.eq(round_id))
        .filter(awd_flag_issues::Column::FlagHash.eq(flag_hash))
        .one(db)
        .await
}

// ── Submissions ──

pub async fn create_submission(
    db: &impl ConnectionTrait,
    event_id: Uuid,
    round_id: Uuid,
    flag_issue_id: Uuid,
    attacker_team_id: Uuid,
    victim_team_id: Uuid,
    gamebox_instance_id: Uuid,
    submitted_by_user_id: Uuid,
) -> Result<awd_flag_submissions::Model, sea_orm::DbErr> {
    let model = awd_flag_submissions::ActiveModel {
        id: Set(Uuid::new_v4()),
        event_id: Set(event_id),
        round_id: Set(round_id),
        flag_issue_id: Set(flag_issue_id),
        attacker_team_id: Set(attacker_team_id),
        victim_team_id: Set(victim_team_id),
        gamebox_instance_id: Set(gamebox_instance_id),
        submitted_by_user_id: Set(submitted_by_user_id),
        ..Default::default()
    };

    model.insert(db).await
}

pub async fn has_submission(
    db: &impl ConnectionTrait,
    event_id: Uuid,
    round_id: Uuid,
    attacker_team_id: Uuid,
    gamebox_instance_id: Uuid,
) -> Result<bool, sea_orm::DbErr> {
    let exists = awd_flag_submissions::Entity::find()
        .filter(awd_flag_submissions::Column::EventId.eq(event_id))
        .filter(awd_flag_submissions::Column::RoundId.eq(round_id))
        .filter(awd_flag_submissions::Column::AttackerTeamId.eq(attacker_team_id))
        .filter(awd_flag_submissions::Column::GameboxInstanceId.eq(gamebox_instance_id))
        .one(db)
        .await?;
    Ok(exists.is_some())
}
