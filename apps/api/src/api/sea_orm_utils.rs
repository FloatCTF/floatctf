use sea_orm::DbErr;
use sea_orm::entity::prelude::Uuid;
use sea_orm::{Condition, DatabaseConnection, EntityTrait, QueryFilter, Select};
pub struct FilterMapping {
    pub key: &'static str,
    pub column: Box<dyn Fn(&str) -> Condition>,
}

pub fn apply_filters<E: EntityTrait>(
    mut stmt: Select<E>,
    filter: Option<String>,
    mappings: &[FilterMapping],
) -> Select<E> {
    if let Some(f) = filter {
        let tokens: Vec<&str> = f.split_whitespace().collect();
        let mut i = 0;
        let mut or_conditions: Vec<Condition> = vec![];
        let mut and_conditions: Vec<Condition> = vec![];

        while i < tokens.len() {
            let token = tokens[i];

            match token {
                "&" => {
                    i += 1;
                    continue;
                }
                "|" => {
                    if !and_conditions.is_empty() {
                        // fold 累加多个 Condition
                        let combined = and_conditions
                            .drain(..)
                            .fold(Condition::all(), |c, cond| c.add(cond));
                        or_conditions.push(combined);
                    }
                    i += 1;
                }
                _ => {
                    if let Some(pos) = token.find(':') {
                        let key = &token[..pos];
                        let mut value = token[pos + 1..].to_string();

                        // 拼接后续 token，直到遇到逻辑符号
                        i += 1;
                        while i < tokens.len() && tokens[i] != "&" && tokens[i] != "|" {
                            value.push(' ');
                            value.push_str(tokens[i]);
                            i += 1;
                        }

                        if let Some(m) = mappings.iter().find(|m| m.key == key) {
                            and_conditions.push((m.column)(&value));
                        }
                    } else {
                        i += 1;
                    }
                }
            }
        }

        // 剩余 AND 条件加入 OR 条件
        if !and_conditions.is_empty() {
            let combined = and_conditions
                .drain(..)
                .fold(Condition::all(), |c, cond| c.add(cond));
            or_conditions.push(combined);
        }

        // 最终 OR 条件累加到 stmt
        if !or_conditions.is_empty() {
            let combined = or_conditions
                .into_iter()
                .fold(Condition::any(), |c, cond| c.add(cond));
            stmt = stmt.filter(combined);
        }
    }

    stmt
}
use sea_orm::PaginatorTrait;

use crate::api::QueryParams;

pub async fn paginate_query<E: EntityTrait>(
    stmt: Select<E>,
    db: &DatabaseConnection,
    limit: u64,
    page: u64,
) -> Result<(Vec<E::Model>, usize), DbErr>
where
    E: EntityTrait,
    E::Model: Send + Sync,
{
    if limit == 0 || page == 0 {
        return Ok((vec![], 0)); // 不分页或参数无效，直接返回空
    }

    let total_items = stmt.clone().count(db).await? as usize;

    // 计算分页索引
    let total_pages = (total_items + limit as usize - 1) / limit as usize;
    let page_index = page
        .saturating_sub(1)
        .min(total_pages.saturating_sub(1) as u64);

    // 分页查询
    let items = stmt.paginate(db, limit).fetch_page(page_index).await?;

    Ok((items, total_items))
}

pub async fn query_query<E>(
    db: &DatabaseConnection,
    mappings: &[FilterMapping],
    query_params: &QueryParams,
    order_fn: Option<Box<dyn FnOnce(Select<E>) -> Select<E>>>,
) -> Result<(Vec<E::Model>, usize), DbErr>
where
    E: EntityTrait,
    E::Model: Send + Sync,
{
    let stmt = apply_filters(E::find(), query_params.filter.clone(), &mappings);
    let stmt = match order_fn {
        Some(f) => f(stmt),
        None => stmt,
    };

    let (items, total_items) =
        if let (Some(limit), Some(page)) = (query_params.limit, query_params.page) {
            let d = paginate_query(stmt, db, limit, page).await?;
            d
        } else {
            let items = stmt.all(db).await?;
            (items.clone(), items.len())
        };

    Ok((items, total_items))
}

// ─────────────────────────────────────────────────────────────────────────────
// 跨表过滤支持
// ─────────────────────────────────────────────────────────────────────────────

/// 跨表过滤映射
///
/// 当 filter 中的某些 key 属于关联表而非主查询表时使用。
/// 例如：主查询在 `event_challenges` 上，但需要按 `challenges.name` 过滤。
///
/// 配合 [`resolve_cross_filters`] 使用。
pub struct CrossFilterMapping {
    pub key: &'static str,
    pub column: Box<dyn Fn(&str) -> Condition>,
}

/// 解析跨表过滤条件，返回匹配的外键 ID 列表。
///
/// 当 `filter` 中包含 `cross_mappings` 里定义的 key 时，会查询对应的关联表，
/// 返回 `Some(ids)` 供调用方用 `is_in` 约束主表的外键列。
/// 如果 filter 中不包含任何跨表 key，返回 `None`。
///
/// # 示例
///
/// 主表 `event_challenges`，需要按 `challenges` 表的 `name` / `category` 过滤：
///
/// ```ignore
/// use crate::api::sea_orm_utils::{CrossFilterMapping, resolve_cross_filters};
///
/// let ids = resolve_cross_filters::<challenges::Entity>(
///     db.get_ref(),
///     &query_params.filter,
///     &[
///         CrossFilterMapping {
///             key: "name",
///             column: Box::new(|v| {
///                 Condition::all().add(challenges::Column::Name.contains(v))
///             }),
///         },
///         CrossFilterMapping {
///             key: "category",
///             column: Box::new(|v| {
///                 Condition::all().add(challenges::Column::Category.contains(v))
///             }),
///         },
///     ],
///     |m| m.id,   // 从 challenges::Model 提取 id
/// ).await?;
///
/// let mut stmt = event.find_related(event_challenges::Entity);
/// if let Some(ids) = ids {
///     stmt = stmt.filter(event_challenges::Column::ChallengeId.is_in(ids));
/// }
/// ```
pub async fn resolve_cross_filters<E>(
    db: &DatabaseConnection,
    filter: &Option<String>,
    cross_mappings: &[CrossFilterMapping],
    extract_id: impl Fn(E::Model) -> Uuid,
) -> Result<Option<Vec<Uuid>>, DbErr>
where
    E: EntityTrait,
    E::Model: Send + Sync,
{
    let filter_str = match filter {
        Some(f) if !f.is_empty() => f.as_str(),
        _ => return Ok(None),
    };

    // 从 filter 字符串中收集匹配的跨表条件
    let mut conditions: Vec<Condition> = Vec::new();
    let tokens: Vec<&str> = filter_str.split_whitespace().collect();
    let mut i = 0;

    while i < tokens.len() {
        let token = tokens[i];
        if token == "&" || token == "|" {
            i += 1;
            continue;
        }

        if let Some(pos) = token.find(':') {
            let key = &token[..pos];
            let mut value = token[pos + 1..].to_string();

            // 拼接后续 token，直到遇到逻辑符号（与 apply_filters 同逻辑）
            i += 1;
            while i < tokens.len() && tokens[i] != "&" && tokens[i] != "|" {
                value.push(' ');
                value.push_str(tokens[i]);
                i += 1;
            }

            if let Some(m) = cross_mappings.iter().find(|m| m.key == key) {
                conditions.push((m.column)(&value));
            }
        } else {
            i += 1;
        }
    }

    if conditions.is_empty() {
        return Ok(None);
    }

    // 将所有条件合并为 AND，查询关联表并提取 ID
    let combined = conditions
        .into_iter()
        .fold(Condition::all(), |c, cond| c.add(cond));
    let items = E::find().filter(combined).all(db).await?;

    Ok(Some(items.into_iter().map(extract_id).collect()))
}
