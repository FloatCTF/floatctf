#[actix_web::main]
async fn main() -> std::io::Result<()> {
    floatctf::bootstrap::run().await
}
