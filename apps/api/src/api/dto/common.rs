//! 通用 HTTP 响应辅助 DTO。

use sea_orm::entity::prelude::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteItemsRequest {
    pub id_list: Vec<Uuid>,
}
