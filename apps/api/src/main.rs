#[actix_web::main]
async fn main() -> std::io::Result<()> {
    match floatctf::bootstrap::run().await {
        Ok(()) => Ok(()),
        Err(e) => {
            // Phase 0 P0-2：启动失败（含 AWD crypto 初始化失败）必须 fail-fast 非 0 退出，
            // 不允许带病继续服务。
            eprintln!("fatal: {e}");
            std::process::exit(1);
        }
    }
}
