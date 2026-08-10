//! CLI argument definitions.

use clap::{Parser, ValueEnum};

#[derive(Debug, Clone, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "snake_case")]
pub enum GenFormat {
    #[value(alias = "c")]
    Challenge,
    #[value(alias = "g")]
    Gamebox,
}

#[derive(Parser, Debug)]
#[command(name = "fcmc", about = "FloatCTF 题目配置检查和管理工具")]
#[command(version = env!("CARGO_PKG_VERSION"))]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Parser, Debug, Clone)]
#[command(rename_all = "snake_case")]
pub enum Commands {
    /// 检查题目配置文件是否合法
    Check {
        /// 配置文件目录 (里面需要包含 meta.toml)
        #[arg(short, long)]
        path: Option<String>,

        /// 额外连接 Docker 运行时验证
        #[arg(long, default_value = "false")]
        runtime: bool,
    },
    /// 构建题目镜像
    Build {
        /// 配置文件目录 (里面需要包含 meta.toml)
        #[arg(short, long)]
        path: Option<String>,
        /// 构建模板类型: challenge (c) | gamebox (g)。缺省不传时按 meta.toml 内容自动识别
        /// （含 [gamebox] 段按 gamebox，否则按 challenge）。
        #[arg(short, long)]
        format: Option<GenFormat>,
        /// 镜像 tag（gamebox 推荐显式传入；缺省为 floatctf/gameboxes/<safe_name>:<version>）
        /// Challenge 默认 <prefix>/challenges/<safe_name>:<version>。
        #[arg(short = 't', long = "tag")]
        tag: Option<String>,
        /// 构建代理 [ip:]port（缺省 ip 用 host.docker.internal）。设置后注入
        /// --add-host=host.docker.internal:host-gateway 与 HTTP_PROXY/HTTPS_PROXY/ALL_PROXY；
        /// 未设置则不注入。
        #[arg(long)]
        proxy: Option<String>,
    },
    /// 生成新的题目模板
    Gen {
        /// 新题目的名称
        #[arg(short, long)]
        name: String,

        /// 输出目录
        #[arg(short, long, default_value = ".")]
        output: String,

        /// 生成模板类型: challenge (c) | gamebox (g)
        #[arg(short, long, default_value = "challenge")]
        format: GenFormat,

        /// gamebox 基础模板 (仅 format=gamebox 时生效)
        #[arg(short, long, default_value = "false")]
        template: bool,
    },
}

impl Commands {
    /// Get the path for check/build commands.
    pub fn path(&self) -> Option<&str> {
        match self {
            Commands::Check { path, .. } => path.as_deref(),
            Commands::Build { path, .. } => path.as_deref(),
            Commands::Gen { .. } => None,
        }
    }
}
