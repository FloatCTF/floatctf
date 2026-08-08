//! Admin challenge import / check / docker image build.

use crate::api::dto::map_dto_vec;

use crate::api::prelude::*;
use crate::entity::{challenges, prelude::Challenges};
use crate::modules::challenge::catalog::ChallengesDto;
use crate::modules::challenge::metadata::generate_safe_name;
use actix_multipart::form::{MultipartForm, tempfile::TempFile, text::Text};
use base64::Engine;
use fcmc::{ChallengeMeta, DockerContainerRuntime};

use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

#[derive(Debug, MultipartForm)]
struct UploadForm {
    /// 归一化导入字段：单题（根目录 meta.toml）或批量（目录树递归发现 meta.toml）统一走这里
    #[multipart(limit = "10240MB")]
    import_challenges_zip: Option<TempFile>,
    toml_str_b64: Option<Text<String>>,
}
/// POST /api/admin/challenges/import
#[post("/import")]
pub async fn web_import_challenge(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    MultipartForm(form): MultipartForm<UploadForm>,
) -> UniResult<Vec<ChallengesDto>> {
    let user = user.into_inner();
    let mut will_insert_toml_strs = Vec::new();
    let mut inserted_challenges = Vec::new();

    if let Some(s) = form.toml_str_b64 {
        let toml_str = base64::prelude::BASE64_STANDARD
            .decode(s.0)
            .map_err(|e| AppError::BadRequest(format!("base64 decode error: {}", e)))?;

        let toml_str = String::from_utf8(toml_str)
            .map_err(|e| AppError::BadRequest(format!("utf8 decode error: {}", e)))?;

        will_insert_toml_strs.push(toml_str);
    }

    if let Some(zip) = form.import_challenges_zip {
        let models = import_challenges_zip(&ctx.db, zip.file)
            .await
            .map_err(|e| AppError::BadRequest(format!("import challenges zip error: {}", e)))?;

        inserted_challenges.extend(models);
    }

    for toml_str in will_insert_toml_strs {
        let challenge = import_challenge(ctx.db.get_ref(), toml_str)
            .await
            .map_err(|e| AppError::BadRequest(format!("import challenge error: {}", e)))?;

        inserted_challenges.push(challenge);
    }

    ctx.log
        .add_log(
            "INFO",
            "CHALLENGES",
            "IMPORT",
            format!(
                "{} 导入 {} 道题目",
                user.username,
                inserted_challenges.len()
            )
            .as_str(),
            json!({"count": inserted_challenges.len()}),
            None,
            user.id.into(),
            Some(&ctx.req),
        )
        .await;

    UniResponse::ok(Some(map_dto_vec(inserted_challenges))).into()
}

pub async fn import_challenge(
    db: &DatabaseConnection,
    challenge_toml_str: String,
) -> anyhow::Result<challenges::Model> {
    let c = ChallengeMeta::from_toml_str(&challenge_toml_str)?;

    // 同名已存在 → 覆盖导入（保留原有 safe_name，避免与其它行撞 UNIQUE）
    if let Some(existing) = Challenges::find()
        .filter(challenges::Column::Name.eq(&c.name))
        .one(db)
        .await?
    {
        let mut am: challenges::ActiveModel = existing.into();
        am.category = Set(c.category);
        am.description = Set(c.description);
        am.attachment = Set(c.attachment);
        am.toml_str = Set(challenge_toml_str);
        am.update(db).await?;

        return Challenges::find()
            .filter(challenges::Column::Name.eq(&c.name))
            .one(db)
            .await?
            .ok_or_else(|| anyhow::anyhow!("challenge not found after update"));
    }

    // 新题：safe_name 若被其它行占用则追加 -2/-3 去重（challenges_safe_name_key UNIQUE）
    let safe_name = unique_safe_name(db, &generate_safe_name(&c.name)).await?;

    let new_challenge = challenges::ActiveModel {
        name: Set(c.name.clone()),
        category: Set(c.category),
        description: Set(c.description),
        attachment: Set(c.attachment),
        safe_name: Set(safe_name),
        toml_str: Set(challenge_toml_str),
        ..Default::default()
    };
    let res = challenges::Entity::insert(new_challenge).exec(db).await?;
    challenges::Entity::find_by_id(res.last_insert_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("challenge not found after insert"))
}

/// 判断 safe_name 是否已被占用（challenges_safe_name_key UNIQUE）。
async fn safe_name_taken(db: &DatabaseConnection, safe_name: &str) -> anyhow::Result<bool> {
    Ok(Challenges::find()
        .filter(challenges::Column::SafeName.eq(safe_name))
        .one(db)
        .await?
        .is_some())
}

/// 生成未被占用的 safe_name：base 被占用则依次尝试 base-2、base-3…
async fn unique_safe_name(db: &DatabaseConnection, base: &str) -> anyhow::Result<String> {
    if !safe_name_taken(db, base).await? {
        return Ok(base.to_string());
    }
    for i in 2..=999 {
        let candidate = format!("{}-{}", base, i);
        if !safe_name_taken(db, &candidate).await? {
            return Ok(candidate);
        }
    }
    anyhow::bail!("无法为挑战生成唯一 safe_name: {}", base)
}

/// 导入题目压缩包（单题/批量统一逻辑）：
/// 1. 解压到临时目录；
/// 2. 先检查解压根目录是否有 meta.toml —— 有则视为单题导入；
/// 3. 没有则递归查找目录树下所有 meta.toml，每个 meta.toml 所在目录视为一道题目；
/// 4. 每道题先落 DB（同名覆盖 / safe_name 去重），拿到最终 safe_name 后
///    再把 meta.toml 所在目录覆盖复制到 {CHALLENGES_DIR}/{safe_name}；
/// 5. 返回导入/更新后的 challenges 行。
pub async fn import_challenges_zip(
    db: &DatabaseConnection,
    challenges_zip: tempfile::NamedTempFile,
) -> anyhow::Result<Vec<challenges::Model>> {
    let output_root = get_setting(&db, "CHALLENGES_DIR")
        .await
        .map_err(|e| AppError::BadRequest(format!("get setting error: {}", e)))?;

    let tmp_dir = tempfile::tempdir()?;
    let mut archive = zip::ZipArchive::new(challenges_zip)?;
    // zip crate 的 extract 自带 zip-slip 路径穿越防护，越界条目直接报错
    archive.extract(tmp_dir.path())?;

    let meta_paths = find_meta_tomls(tmp_dir.path());
    if meta_paths.is_empty() {
        anyhow::bail!("压缩包内未找到 meta.toml");
    }

    let mut imported = Vec::with_capacity(meta_paths.len());
    for meta_path in meta_paths {
        let meta_toml = std::fs::read_to_string(&meta_path)?;
        // 先落 DB（同名覆盖 / safe_name 去重），返回带最终 safe_name 的行
        let model = import_challenge(db, meta_toml).await?;

        // 再按 DB 中的 safe_name 覆盖复制 meta.toml 所在目录，目录与行始终一致
        let dest = std::path::Path::new(&output_root).join(&model.safe_name);
        if dest.exists() {
            // 覆盖导入：整目录重建
            std::fs::remove_dir_all(&dest)?;
        }
        let src_dir = meta_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("meta.toml 缺少父目录"))?;
        copy_dir_all(src_dir, &dest)?;

        imported.push(model);
    }
    Ok(imported)
}

/// 递归查找 meta.toml：根目录存在则只按单题处理；否则收集所有子目录中的 meta.toml。
fn find_meta_tomls(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    let root_meta = root.join("meta.toml");
    if root_meta.is_file() {
        return vec![root_meta];
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if entry.file_name() == "meta.toml" {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    out
}

/// 递归复制目录（meta.toml 所在目录整体搬入 CHALLENGES_DIR/{safe_name}）。
fn copy_dir_all(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// POST /api/admin/challenges/check
#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeCheckResult {
    pub id: Uuid,
    pub challenge_name: String,
    pub is_ok: bool,
    pub docker_image: bool,
    pub attachment: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ChallengeCheckRequest {
    pub challenge_id_list: Option<Vec<Uuid>>,
}

#[post("/check")]
pub async fn check_challenges(
    _user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    ccr: Json<ChallengeCheckRequest>,
) -> UniResult<Vec<ChallengeCheckResult>> {
    let ccr = ccr.into_inner();
    let mut challenge_check_results = Vec::new();
    // check docker image
    // check challenge attachment
    let challenge_dir = get_setting(&ctx.db, "CHALLENGES_DIR")
        .await
        .map_err(|e| AppError::BadRequest(format!("get setting error: {}", e)))?;

    let challenges = {
        if ccr.challenge_id_list.is_some() {
            Challenges::find()
                .filter(challenges::Column::Id.is_in(ccr.challenge_id_list.unwrap()))
                .all(ctx.db.get_ref())
                .await?
        } else {
            Challenges::find().all(ctx.db.get_ref()).await?
        }
    };

    for challenge in challenges {
        let attachment_ok = challenge.attachment.as_ref().map_or(true, |attachment| {
            let challenge_dir = std::path::Path::new(&challenge_dir).join(&challenge.safe_name);
            challenge_dir.join(attachment).exists()
        });

        let cm = ChallengeMeta::from_toml_str(&challenge.toml_str)
            .map_err(|e| AppError::BadRequest(format!("parse challenge meta error: {}", e)))?;

        let docker_image_ok = match &cm.docker {
            Some(d) => ctx.docker.inspect_image(&d.image_tag).await.is_ok(),
            None => true, // 非docker 题目 默认为true
        };

        challenge_check_results.push(ChallengeCheckResult {
            id: challenge.id,
            challenge_name: challenge.name,
            is_ok: attachment_ok && docker_image_ok,
            docker_image: docker_image_ok,
            attachment: attachment_ok,
        });
    }

    UniResponse::ok(challenge_check_results.into()).into()
}

/// POST /api/admin/challenges/build
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildChallengeRequest {
    pub challenge_id: Option<Uuid>,
    pub challenge_id_list: Option<Vec<Uuid>>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct BuildChallengeResult {
    pub challenge_name: String,
    pub is_ok: bool,
    pub message: String,
}

#[post("/build")]
pub async fn build_challenge(
    user: SuperAdminJwtGuard,
    ctx: ReqCtx,
    bcr: Json<BuildChallengeRequest>,
) -> UniResult<Vec<BuildChallengeResult>> {
    let user = user.into_inner();
    let bcr = bcr.into_inner();
    let mut res = Vec::new();
    let mut challenge_id_list = Vec::new();
    bcr.challenge_id.map(|c| {
        challenge_id_list.push(c);
    });
    bcr.challenge_id_list.map(|c| {
        challenge_id_list.extend(c);
    });

    for challenge_id in challenge_id_list {
        let challenge = Challenges::find_by_id(challenge_id)
            .one(ctx.db.get_ref())
            .await?
            .ok_or(AppError::NotFound(format!(" {} not exist", challenge_id)))?;

        let cm = ChallengeMeta::from_toml_str(&challenge.toml_str)
            .map_err(|e| AppError::BadRequest(format!("parse challenge meta error: {}", e)))?;

        if cm.docker.is_none() {
            continue;
        }

        let challenges_dir = get_setting(&ctx.db, "CHALLENGES_DIR")
            .await
            .map_err(|e| AppError::BadRequest(format!("get setting error: {}", e)))?;

        let context_path = std::path::Path::new(&challenges_dir)
            .join(&challenge.safe_name)
            .join("src");

        let image_tag = &cm
            .docker
            .as_ref()
            .expect("docker metadata checked")
            .image_tag;
        let runtime = DockerContainerRuntime::new(ctx.docker.get_ref().clone());
        let build_result = runtime.build_image(image_tag, &context_path).await;
        let is_ok = build_result.is_ok();
        let message = build_result.map_or_else(|e| e.to_string(), |_| "ok".to_string());

        res.push(BuildChallengeResult {
            challenge_name: challenge.name.clone(),
            is_ok,
            message,
        });

        ctx.log
            .add_log(
                "INFO",
                "CHALLENGES",
                "BUILD",
                format!("{} 构建题目镜像: {}", user.username, challenge.name).as_str(),
                json!({"challenge_name": challenge.name, "success": is_ok}),
                None,
                user.id.into(),
                Some(&ctx.req),
            )
            .await;
    }

    UniResponse::ok(res.into()).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_meta_tomls_root_wins() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("meta.toml"), "").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/meta.toml"), "").unwrap();
        let found = find_meta_tomls(dir.path());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0], dir.path().join("meta.toml"));
    }

    #[test]
    fn find_meta_tomls_recursive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("a/b")).unwrap();
        std::fs::create_dir_all(dir.path().join("c")).unwrap();
        std::fs::write(dir.path().join("a/meta.toml"), "").unwrap();
        std::fs::write(dir.path().join("a/b/meta.toml"), "").unwrap();
        std::fs::write(dir.path().join("c/meta.toml"), "").unwrap();
        let found = find_meta_tomls(dir.path());
        assert_eq!(found.len(), 3);
        // 排序后确定顺序：a/b/meta.toml < a/meta.toml < c/meta.toml
        assert_eq!(found[0], dir.path().join("a/b/meta.toml"));
        assert_eq!(found[1], dir.path().join("a/meta.toml"));
        assert_eq!(found[2], dir.path().join("c/meta.toml"));
    }

    #[test]
    fn find_meta_tomls_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("x")).unwrap();
        std::fs::write(dir.path().join("x/readme.txt"), "").unwrap();
        assert!(find_meta_tomls(dir.path()).is_empty());
    }

    #[test]
    fn copy_dir_all_recursive() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(src.path().join("src")).unwrap();
        std::fs::write(src.path().join("meta.toml"), "m").unwrap();
        std::fs::write(src.path().join("src/main.py"), "code").unwrap();
        copy_dir_all(src.path(), &dst.path().join("out")).unwrap();
        let out = dst.path().join("out");
        assert_eq!(std::fs::read_to_string(out.join("meta.toml")).unwrap(), "m");
        assert_eq!(
            std::fs::read_to_string(out.join("src/main.py")).unwrap(),
            "code"
        );
    }
}
