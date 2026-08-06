//! Cross-cutting request shapes shared by multiple modules.

use sea_orm::entity::prelude::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteItemsRequest {
    pub id_list: Vec<Uuid>,
}
