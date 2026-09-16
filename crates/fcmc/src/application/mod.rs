//! fcmc 应用用例：构建、检查、生成、AWD、手工操作。

pub mod awd;
pub mod build;
pub mod check;
pub mod generate;
pub mod manual;
pub mod package;

pub use package::{has_dockerfile, resolve_content_id};
