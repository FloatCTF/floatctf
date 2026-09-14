//! 平台文件上传/下载处理器。

pub mod download;
pub mod upload;

pub use download::presign_private_download_url;
