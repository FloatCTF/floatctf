//! 平台文件上传/下载处理器。

pub mod download;
pub mod upload;

pub use download::generate_presigned_download_url;
