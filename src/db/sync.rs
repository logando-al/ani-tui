//! TTL-based cache sync metadata.
//! Checks whether a category is stale and needs re-fetching from AniList.

use crate::error::Result;
use sqlx::SqlitePool;

/// Known category keys
pub const TRENDING:  &str = "trending";
pub const POPULAR:   &str = "popular";
pub const TOP_RATED: &str = "top_rated";
pub const SEASONAL:  &str = "seasonal";

#[allow(dead_code)]
pub const ALL_CATEGORIES: &[&str] = &[TRENDING, POPULAR, TOP_RATED, SEASONAL];

/// Record that a category was just synced.
pub async fn mark_synced(pool: &SqlitePool, key: &str, now: i64) -> Result<()> {
    sqlx::query(
        "INSERT INTO sync_meta (key, synced_at) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET synced_at = excluded.synced_at",
    )
    .bind(key)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get the last sync timestamp for a category (None if never synced).
pub async fn last_synced(pool: &SqlitePool, key: &str) -> Result<Option<i64>> {
    let ts: Option<i64> =
        sqlx::query_scalar("SELECT synced_at FROM sync_meta WHERE key = ?")
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(ts)
}

/// Returns true if the category is stale (never synced OR synced_at + ttl < now).
pub async fn is_stale(pool: &SqlitePool, key: &str, ttl: u64, now: i64) -> Result<bool> {
    match last_synced(pool, key).await? {
        None     => Ok(true),
        Some(ts) => Ok(now - ts > ttl as i64),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init;

    #[tokio::test]
    async fn test_never_synced_is_stale() {
        let pool = init(":memory:").await.unwrap();
        let stale = is_stale(&pool, TRENDING, 86_400, 1_000_000).await.unwrap();
        assert!(stale);
    }

    #[tokio::test]
    async fn test_recently_synced_is_not_stale() {
        let pool = init(":memory:").await.unwrap();
        let now  = 1_700_000_000i64;
        mark_synced(&pool, TRENDING, now).await.unwrap();

        let stale = is_stale(&pool, TRENDING, 86_400, now + 3600).await.unwrap();
        assert!(!stale, "Synced 1h ago with 24h TTL — should not be stale");
    }

    #[tokio::test]
    async fn test_old_sync_is_stale() {
        let pool = init(":memory:").await.unwrap();
        let then = 1_700_000_000i64;
        mark_synced(&pool, TRENDING, then).await.unwrap();

        // 25 hours later
        let now = then + 60 * 60 * 25;
        let stale = is_stale(&pool, TRENDING, 86_400, now).await.unwrap();
        assert!(stale, "Synced 25h ago with 24h TTL — should be stale");
    }

    #[tokio::test]
    async fn test_mark_synced_updates_existing() {
        let pool = init(":memory:").await.unwrap();
        mark_synced(&pool, POPULAR, 1_000).await.unwrap();
        mark_synced(&pool, POPULAR, 2_000).await.unwrap();

        let ts = last_synced(&pool, POPULAR).await.unwrap().unwrap();
        assert_eq!(ts, 2_000);
    }

    #[tokio::test]
    async fn test_last_synced_none_for_unknown_key() {
        let pool = init(":memory:").await.unwrap();
        let ts = last_synced(&pool, "nonexistent").await.unwrap();
        assert!(ts.is_none());
    }
}
