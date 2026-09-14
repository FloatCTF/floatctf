//! 共享、非领域的响应辅助类型。
//!
//! 业务 DTO 放在各自所属的 `modules/*/` 下。

mod common;
pub use common::DeleteItemsRequest;

/// 将模型 `Vec` 映射为响应 DTO。
pub fn map_dto_vec<M, D>(items: Vec<M>) -> Vec<D>
where
    D: From<M>,
{
    items.into_iter().map(Into::into).collect()
}
