//! User data: watch history, continue watching, watchlist.

use crate::error::Result;
use sqlx::SqlitePool;

/// A single history entry: which episode of which show was watched.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct HistoryEntry {
    pub anime_id:   i64,
    pub episode:    i64,
    pub watched_at: i64, // unix timestamp
}

/// The "continue watching" row entry.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ContinueEntry {
    pub anime_id:     i64,
    pub last_episode: i64,
    pub last_watched: i64,
}

// ─── History ──────────────────────────────────────────────────────────────────

/// Record that an episode was watched (upsert: re-watching updates timestamp).
pub async fn record_watched(
    pool:     &SqlitePool,
    anime_id: i64,
    episode:  i64,
    now:      i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO history (anime_id, episode, watched_at)
         VALUES (?, ?, ?)
         ON CONFLICT(anime_id, episode) DO UPDATE SET watched_at = excluded.watched_at",
    )
    .bind(anime_id)
    .bind(episode)
    .bind(now)
    .execute(pool)
    .await?;

    // Also update or insert continue_watching
    sqlx::query(
        "INSERT INTO continue_watching (anime_id, last_episode, last_watched)
         VALUES (?, ?, ?)
         ON CONFLICT(anime_id) DO UPDATE SET
             last_episode = excluded.last_episode,
             last_watched = excluded.last_watched",
    )
    .bind(anime_id)
    .bind(episode)
    .bind(now)
    .execute(pool)
    .await?;

    Ok(())
}

/// Fetch full history for a show, ordered by episode ascending.
pub async fn get_history(pool: &SqlitePool, anime_id: i64) -> Result<Vec<HistoryEntry>> {
    let rows = sqlx::query_as::<_, HistoryEntry>(
        "SELECT anime_id, episode, watched_at FROM history
         WHERE anime_id = ? ORDER BY episode ASC",
    )
    .bind(anime_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Check whether a specific episode has been watched.
pub async fn is_watched(pool: &SqlitePool, anime_id: i64, episode: i64) -> Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM history WHERE anime_id = ? AND episode = ?")
            .bind(anime_id)
            .bind(episode)
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

// ─── Continue Watching ────────────────────────────────────────────────────────

/// Fetch all continue-watching entries, ordered by most recently watched first.
pub async fn get_continue_watching(pool: &SqlitePool) -> Result<Vec<ContinueEntry>> {
    let rows = sqlx::query_as::<_, ContinueEntry>(
        "SELECT anime_id, last_episode, last_watched FROM continue_watching
         ORDER BY last_watched DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Fetch the continue entry for a single show (if any).
pub async fn get_continue_entry(
    pool:     &SqlitePool,
    anime_id: i64,
) -> Result<Option<ContinueEntry>> {
    let row = sqlx::query_as::<_, ContinueEntry>(
        "SELECT anime_id, last_episode, last_watched FROM continue_watching WHERE anime_id = ?",
    )
    .bind(anime_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

// ─── Watchlist ────────────────────────────────────────────────────────────────

/// Add a show to the watchlist.
pub async fn add_to_watchlist(pool: &SqlitePool, anime_id: i64, now: i64) -> Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO watchlist (anime_id, added_at) VALUES (?, ?)",
    )
    .bind(anime_id)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

/// Remove a show from the watchlist.
pub async fn remove_from_watchlist(pool: &SqlitePool, anime_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM watchlist WHERE anime_id = ?")
        .bind(anime_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Check if a show is in the watchlist.
pub async fn is_in_watchlist(pool: &SqlitePool, anime_id: i64) -> Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM watchlist WHERE anime_id = ?")
            .bind(anime_id)
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

/// Get all distinct episode numbers watched for a specific anime, ordered ascending.
pub async fn get_watched_episodes(pool: &SqlitePool, anime_id: i64) -> Result<Vec<i64>> {
    let rows = sqlx::query_scalar::<_, i64>(
        "SELECT DISTINCT episode FROM history WHERE anime_id = ? ORDER BY episode ASC",
    )
    .bind(anime_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Fetch all watchlist anime IDs, ordered by date added.
pub async fn get_watchlist(pool: &SqlitePool) -> Result<Vec<i64>> {
    let ids: Vec<i64> =
        sqlx::query_scalar("SELECT anime_id FROM watchlist ORDER BY added_at DESC")
            .fetch_all(pool)
            .await?;
    Ok(ids)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{cache::{upsert_anime, Anime}, init};

    async fn setup() -> SqlitePool {
        let pool = init(":memory:").await.unwrap();
        // Insert a dummy anime so FK constraints pass
        let anime = Anime {
            id:            1,
            title_english: Some("Test Anime".into()),
            title_romaji:  "Test Anime".into(),
            title_native:  None,
            description:   None,
            episodes:      Some(12),
            status:        Some("FINISHED".into()),
            season:        None,
            season_year:   None,
            score:         Some(80),
            format:        Some("TV".into()),
            genres:        "[]".into(),
            cover_url:     None,
            cover_blob:    None,
            has_dub:       0,
            updated_at:    1_000_000,
        };
        upsert_anime(&pool, &anime).await.unwrap();
        pool
    }

    // ── History ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_record_watched_creates_history_entry() {
        let pool = setup().await;
        record_watched(&pool, 1, 3, 1_700_000_000).await.unwrap();

        let history = get_history(&pool, 1).await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].episode, 3);
    }

    #[tokio::test]
    async fn test_record_watched_updates_timestamp() {
        let pool = setup().await;
        record_watched(&pool, 1, 1, 100).await.unwrap();
        record_watched(&pool, 1, 1, 200).await.unwrap(); // re-watch

        let history = get_history(&pool, 1).await.unwrap();
        assert_eq!(history.len(), 1); // still only one entry
        assert_eq!(history[0].watched_at, 200);
    }

    #[tokio::test]
    async fn test_is_watched_true_and_false() {
        let pool = setup().await;
        record_watched(&pool, 1, 5, 100).await.unwrap();

        assert!(is_watched(&pool, 1, 5).await.unwrap());
        assert!(!is_watched(&pool, 1, 6).await.unwrap());
    }

    // ── Continue Watching ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_record_watched_updates_continue_watching() {
        let pool = setup().await;
        record_watched(&pool, 1, 4, 1_000).await.unwrap();

        let entry = get_continue_entry(&pool, 1).await.unwrap().unwrap();
        assert_eq!(entry.last_episode, 4);
        assert_eq!(entry.last_watched, 1_000);
    }

    #[tokio::test]
    async fn test_continue_watching_ordered_by_recency() {
        let pool = init(":memory:").await.unwrap();

        // Insert two anime
        for id in [1i64, 2i64] {
            let anime = Anime {
                id,
                title_english: Some(format!("Anime {}", id)),
                title_romaji:  format!("Anime {}", id),
                title_native:  None,
                description:   None,
                episodes:      Some(12),
                status:        Some("FINISHED".into()),
                season:        None,
                season_year:   None,
                score:         Some(80),
                format:        Some("TV".into()),
                genres:        "[]".into(),
                cover_url:     None,
                cover_blob:    None,
                has_dub:       0,
                updated_at:    1_000_000,
            };
            upsert_anime(&pool, &anime).await.unwrap();
        }

        record_watched(&pool, 1, 1, 1_000).await.unwrap();
        record_watched(&pool, 2, 1, 2_000).await.unwrap(); // more recent

        let entries = get_continue_watching(&pool).await.unwrap();
        assert_eq!(entries[0].anime_id, 2); // most recent first
        assert_eq!(entries[1].anime_id, 1);
    }

    // ── Watchlist ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_add_to_watchlist() {
        let pool = setup().await;
        add_to_watchlist(&pool, 1, 100).await.unwrap();
        assert!(is_in_watchlist(&pool, 1).await.unwrap());
    }

    #[tokio::test]
    async fn test_remove_from_watchlist() {
        let pool = setup().await;
        add_to_watchlist(&pool, 1, 100).await.unwrap();
        remove_from_watchlist(&pool, 1).await.unwrap();
        assert!(!is_in_watchlist(&pool, 1).await.unwrap());
    }

    #[tokio::test]
    async fn test_add_watchlist_is_idempotent() {
        let pool = setup().await;
        add_to_watchlist(&pool, 1, 100).await.unwrap();
        add_to_watchlist(&pool, 1, 200).await.unwrap(); // should not error or duplicate
        let list = get_watchlist(&pool).await.unwrap();
        assert_eq!(list.len(), 1);
    }

    #[tokio::test]
    async fn test_get_watchlist_returns_ids() {
        let pool = setup().await;
        add_to_watchlist(&pool, 1, 100).await.unwrap();
        let list = get_watchlist(&pool).await.unwrap();
        assert!(list.contains(&1));
    }

    #[tokio::test]
    async fn test_not_in_watchlist_by_default() {
        let pool = setup().await;
        assert!(!is_in_watchlist(&pool, 1).await.unwrap());
    }

    // ── Watched episodes ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_watched_episodes_returns_distinct_episodes() {
        let pool = setup().await;
        record_watched(&pool, 1, 3, 1_000).await.unwrap();
        record_watched(&pool, 1, 5, 2_000).await.unwrap();
        record_watched(&pool, 1, 3, 3_000).await.unwrap(); // re-watch ep3 — still only 1 entry
        let eps = get_watched_episodes(&pool, 1).await.unwrap();
        assert_eq!(eps.len(), 2);
        assert!(eps.contains(&3));
        assert!(eps.contains(&5));
    }

    #[tokio::test]
    async fn test_get_watched_episodes_empty_for_unwatched() {
        let pool = setup().await;
        let eps = get_watched_episodes(&pool, 1).await.unwrap();
        assert!(eps.is_empty());
    }
}
