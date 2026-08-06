//! Shared, non-domain response helpers only.
//!
//! Business DTOs live in their owning modules under `modules/*/`.

mod common;
pub use common::DeleteItemsRequest;

/// Map a `Vec` of models into response DTOs.
pub fn map_dto_vec<M, D>(items: Vec<M>) -> Vec<D>
where
    D: From<M>,
{
    items.into_iter().map(Into::into).collect()
}
