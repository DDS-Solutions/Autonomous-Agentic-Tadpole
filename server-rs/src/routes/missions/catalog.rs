//! @docs ARCHITECTURE:Storage
//!
//! ### AI Assist Note
//! **Missions Catalog**: Provides paginated, searchable, status-filtered,
//! and pinned-prioritized listings of historical agent swarms with atomic counts.
//!
//! ### 🔍 Debugging & Observability
//! - **Failure Path**: Invalid filter parameter, pagination integer overflow, or database connection timeout.
//! - **Telemetry Link**: Search `[missions_catalog]` in tracing logs.
//! - **Trace Scope**: `server-rs::routes::missions::catalog`

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, Sqlite};
use std::sync::Arc;

use crate::error::AppError;
use crate::state::AppState;
use super::models::{
    CatalogQuery, CatalogResponse, MissionSummaryItem, VALID_MISSION_STATUSES,
};

/// Escapes SQL LIKE wildcards (% and _) and backslashes for exact substring matching.
pub fn escape_like_pattern(input: &str) -> String {
    let escaped = input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{}%", escaped)
}

pub struct MissionCatalogFilters {
    pub status: Option<String>,
    pub is_pinned: Option<bool>,
    pub search_pattern: Option<String>,
}

impl MissionCatalogFilters {
    pub fn from_query(query: &CatalogQuery) -> Result<Self, AppError> {
        let raw_status = query.status.as_deref().unwrap_or("").trim();
        let status = if raw_status.is_empty() || raw_status.eq_ignore_ascii_case("all") {
            None
        } else {
            let normalized = raw_status.to_ascii_lowercase();
            if !VALID_MISSION_STATUSES.contains(&normalized.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "Invalid mission status filter '{}'. Expected 'all' or one of: {}",
                    raw_status,
                    VALID_MISSION_STATUSES.join(", ")
                )));
            }
            Some(normalized)
        };

        let is_pinned = query.is_pinned.or(match query.pinned_only {
            Some(true) => Some(true),
            _ => None,
        });

        let raw_search = query.search.as_deref().unwrap_or("").trim();
        let search_pattern = if raw_search.is_empty() {
            None
        } else {
            Some(escape_like_pattern(raw_search))
        };

        Ok(Self {
            status,
            is_pinned,
            search_pattern,
        })
    }

    pub fn apply(&self, qb: &mut QueryBuilder<'_, Sqlite>) {
        if let Some(ref st) = self.status {
            qb.push(" AND status = ");
            qb.push_bind(st.clone());
        }
        if let Some(pinned) = self.is_pinned {
            qb.push(" AND COALESCE(is_pinned, 0) = ");
            qb.push_bind(if pinned { 1 } else { 0 });
        }
        if let Some(ref pattern) = self.search_pattern {
            qb.push(" AND (title LIKE ");
            qb.push_bind(pattern.clone());
            qb.push(" ESCAPE '\\' OR id LIKE ");
            qb.push_bind(pattern.clone());
            qb.push(" ESCAPE '\\' OR agent_id LIKE ");
            qb.push_bind(pattern.clone());
            qb.push(" ESCAPE '\\')");
        }
    }
}

/// Decodes a mission summary row strictly for mandatory identity/state columns.
pub fn decode_mission_summary(r: &SqliteRow) -> Result<MissionSummaryItem, AppError> {
    let is_pinned_raw: i64 = r.try_get("is_pinned").unwrap_or(0);
    let created_at: Option<String> = r.try_get("created_at_iso")?;
    let updated_at: Option<String> = r.try_get("updated_at_iso")?;

    Ok(MissionSummaryItem {
        id: r.try_get("id")?,
        agent_id: r.try_get("agent_id")?,
        title: r.try_get("title")?,
        status: r.try_get("status")?,
        budget_usd: r.try_get("budget_usd").unwrap_or(0.0),
        cost_usd: r.try_get("cost_usd").unwrap_or(0.0),
        is_pinned: is_pinned_raw == 1,
        created_at: created_at.unwrap_or_default(),
        updated_at: updated_at.unwrap_or_default(),
        log_count: r.try_get("log_count").unwrap_or(0),
        diff_count: r.try_get("diff_count").unwrap_or(0),
    })
}

/// GET /v1/missions
///
/// Retrieves a paginated catalog of all missions with aggregated log and diff counts.
#[tracing::instrument(skip(state), name = "missions::get_missions_catalog")]
pub async fn get_missions_catalog(
    State(state): State<Arc<AppState>>,
    Query(query): Query<CatalogQuery>,
) -> Result<(StatusCode, Json<CatalogResponse>), AppError> {
    let pool = &state.resources.pool;
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(25).clamp(1, 100);
    let offset = (page.saturating_sub(1) as i64).saturating_mul(per_page as i64);

    let filters = MissionCatalogFilters::from_query(&query)?;

    // Run COUNT(*) and page SELECT in a single read transaction for snapshot consistency
    let mut tx = pool.begin().await?;

    let mut count_qb =
        QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM mission_history WHERE 1=1");
    filters.apply(&mut count_qb);

    let total: i64 = count_qb
        .build_query_scalar()
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to execute mission catalog count query");
            AppError::from(e)
        })?;

    let mut sel_qb = QueryBuilder::<Sqlite>::new(
        r#"SELECT 
            id, 
            agent_id, 
            title, 
            status, 
            budget_usd, 
            cost_usd, 
            COALESCE(is_pinned, 0) as is_pinned,
            strftime('%Y-%m-%dT%H:%M:%SZ', created_at) as created_at_iso,
            strftime('%Y-%m-%dT%H:%M:%SZ', updated_at) as updated_at_iso,
            (SELECT COUNT(*) FROM mission_logs l WHERE l.mission_id = mission_history.id) as log_count,
            (SELECT COUNT(DISTINCT r.file_path) FROM file_revisions r WHERE r.mission_id = mission_history.id) as diff_count
        FROM mission_history
        WHERE 1=1"#,
    );
    filters.apply(&mut sel_qb);

    sel_qb.push(" ORDER BY is_pinned DESC, created_at DESC, id DESC LIMIT ");
    sel_qb.push_bind(per_page as i64);
    sel_qb.push(" OFFSET ");
    sel_qb.push_bind(offset);

    let rows = sel_qb.build().fetch_all(&mut *tx).await?;
    tx.commit().await?;

    let mut data = Vec::with_capacity(rows.len());
    for r in rows {
        data.push(decode_mission_summary(&r)?);
    }

    Ok((
        StatusCode::OK,
        Json(CatalogResponse {
            success: true,
            data,
            total,
            page,
            per_page,
        }),
    ))
}

// Metadata: [missions_catalog]

