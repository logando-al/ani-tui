//! Read/write the AniList cache (anime metadata, category lists, genre lists).

use crate::error::Result;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Represents a single anime entry — the canonical data model used throughout the app.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Anime {
    pub id:            i64,
    pub title_english: Option<String>,
    pub title_romaji:  String,
    pub title_native:  Option<String>,
    pub description:   Option<String>,
    pub episodes:      Option<i64>,
    pub status:        Option<String>,
    pub season:        Option<String>,
    pub season_year:   Option<i64>,
    pub score:         Option<i64>,
    pub format:        Option<String>,
    /// JSON-encoded Vec<String>: ["Action", "Drama"]
    pub genres:        String,
    pub cover_url:     Option<String>,
    pub cover_blob:    Option<Vec<u8>>,
    pub has_dub:       i64,  // 0 or 1 (SQLite has no bool)
    pub updated_at:    i64,  // unix timestamp
}

impl Anime {
    /// Decoded genre list.
    pub fn genre_list(&self) -> Vec<String> {
        serde_json::from_str(&self.genres).unwrap_or_default()
    }

    /// Display title — prefers English, falls back to romaji.
    pub fn display_title(&self) -> &str {
        self.title_english
            .as_deref()
            .filter(|t| !t.is_empty())
            .unwrap_or(&self.title_romaji)
    }

    /// Short title for UI cards (max 20 visible chars, ellipsis if truncated).
    pub fn short_title(&self) -> String {
        let title = self.display_title();
        let chars: Vec<char> = title.chars().collect();
        if chars.len() <= 20 {
            title.to_string()
        } else {
            let truncated: String = chars[..19].iter().collect();
            format!("{}…", truncated)
        }
    }

    /// Returns true if dub is available.
    pub fn has_dub(&self) -> bool {
        self.has_dub != 0
    }
}

/// Upsert a single anime into the cache.
pub async fn upsert_anime(pool: &SqlitePool, anime: &Anime) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO anime (
            id, title_english, title_romaji, title_native,
            description, episodes, status, season, season_year,
            score, format, genres, cover_url, cover_blob, has_dub, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            title_english = excluded.title_english,
            title_romaji  = excluded.title_romaji,
            title_native  = excluded.title_native,
            description   = excluded.description,
            episodes      = excluded.episodes,
            status        = excluded.status,
            season        = excluded.season,
            season_year   = excluded.season_year,
            score         = excluded.score,
            format        = excluded.format,
            genres        = excluded.genres,
            cover_url     = excluded.cover_url,
            has_dub       = excluded.has_dub,
            updated_at    = excluded.updated_at
        "#,
    )
    .bind(anime.id)
    .bind(&anime.title_english)
    .bind(&anime.title_romaji)
    .bind(&anime.title_native)
    .bind(&anime.description)
    .bind(anime.episodes)
    .bind(&anime.status)
    .bind(&anime.season)
    .bind(anime.season_year)
    .bind(anime.score)
    .bind(&anime.format)
    .bind(&anime.genres)
    .bind(&anime.cover_url)
    .bind(&anime.cover_blob)
    .bind(anime.has_dub)
    .bind(anime.updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

/// Store the cover image blob for an anime (separate upsert to avoid re-downloading).
pub async fn store_cover_blob(pool: &SqlitePool, anime_id: i64, blob: &[u8]) -> Result<()> {
    sqlx::query("UPDATE anime SET cover_blob = ? WHERE id = ?")
        .bind(blob)
        .bind(anime_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Fetch a single anime by AniList ID.
pub async fn get_anime(pool: &SqlitePool, id: i64) -> Result<Option<Anime>> {
    let row = sqlx::query_as::<_, Anime>("SELECT * FROM anime WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// Upsert ordered anime IDs for a category row (e.g. "trending").
pub async fn upsert_category(
    pool:      &SqlitePool,
    category:  &str,
    anime_ids: &[i64],
    now:       i64,
) -> Result<()> {
    // Clear existing positions for this category
    sqlx::query("DELETE FROM category_entries WHERE category = ?")
        .bind(category)
        .execute(pool)
        .await?;

    for (pos, &id) in anime_ids.iter().enumerate() {
        sqlx::query(
            "INSERT OR REPLACE INTO category_entries (category, anime_id, position, refreshed_at)
             VALUES (?, ?, ?, ?)",
        )
        .bind(category)
        .bind(id)
        .bind(pos as i64)
        .bind(now)
        .execute(pool)
        .await?;
    }
    Ok(())
}

/// Fetch ordered anime list for a given category.
pub async fn get_category(pool: &SqlitePool, category: &str) -> Result<Vec<Anime>> {
    let rows = sqlx::query_as::<_, Anime>(
        r#"
        SELECT a.*
        FROM anime a
        JOIN category_entries ce ON a.id = ce.anime_id
        WHERE ce.category = ?
        ORDER BY ce.position ASC
        "#,
    )
    .bind(category)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Search cached anime by title (case-insensitive, partial match).
pub async fn search_cache(pool: &SqlitePool, query: &str) -> Result<Vec<Anime>> {
    let pattern = format!("%{}%", query);
    let rows = sqlx::query_as::<_, Anime>(
        r#"
        SELECT * FROM anime
        WHERE title_english LIKE ? OR title_romaji LIKE ?
        ORDER BY score DESC
        LIMIT 50
        "#,
    )
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init;

    fn sample_anime(id: i64, title: &str) -> Anime {
        Anime {
            id,
            title_english: Some(title.to_string()),
            title_romaji:  title.to_string(),
            title_native:  None,
            description:   Some("Test description".to_string()),
            episodes:      Some(24),
            status:        Some("FINISHED".to_string()),
            season:        Some("FALL".to_string()),
            season_year:   Some(2023),
            score:         Some(85),
            format:        Some("TV".to_string()),
            genres:        r#"["Action","Drama"]"#.to_string(),
            cover_url:     Some("https://example.com/cover.jpg".to_string()),
            cover_blob:    None,
            has_dub:       0,
            updated_at:    1_700_000_000,
        }
    }

    #[tokio::test]
    async fn test_upsert_and_get_anime() {
        let pool   = init(":memory:").await.unwrap();
        let anime  = sample_anime(1535, "Sword Art Online");
        upsert_anime(&pool, &anime).await.unwrap();

        let fetched = get_anime(&pool, 1535).await.unwrap().unwrap();
        assert_eq!(fetched.id,            1535);
        assert_eq!(fetched.title_romaji, "Sword Art Online");
        assert_eq!(fetched.score,         Some(85));
    }

    #[tokio::test]
    async fn test_upsert_anime_is_idempotent() {
        let pool  = init(":memory:").await.unwrap();
        let anime = sample_anime(1, "Test Anime");
        upsert_anime(&pool, &anime).await.unwrap();
        upsert_anime(&pool, &anime).await.unwrap(); // second upsert should not error
        let fetched = get_anime(&pool, 1).await.unwrap().unwrap();
        assert_eq!(fetched.id, 1);
    }

    #[tokio::test]
    async fn test_upsert_updates_existing() {
        let pool  = init(":memory:").await.unwrap();
        let anime = sample_anime(1, "Original Title");
        upsert_anime(&pool, &anime).await.unwrap();

        let mut updated       = anime.clone();
        updated.score         = Some(99);
        updated.title_english = Some("Updated Title".to_string());
        upsert_anime(&pool, &updated).await.unwrap();

        let fetched = get_anime(&pool, 1).await.unwrap().unwrap();
        assert_eq!(fetched.score,         Some(99));
        assert_eq!(fetched.title_english, Some("Updated Title".to_string()));
    }

    #[tokio::test]
    async fn test_get_nonexistent_anime_returns_none() {
        let pool = init(":memory:").await.unwrap();
        let result = get_anime(&pool, 9999).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_upsert_and_get_category() {
        let pool  = init(":memory:").await.unwrap();
        let anime = [sample_anime(1, "Anime A"), sample_anime(2, "Anime B")];
        for a in &anime {
            upsert_anime(&pool, a).await.unwrap();
        }

        upsert_category(&pool, "trending", &[1, 2], 1_700_000_000).await.unwrap();

        let results = get_category(&pool, "trending").await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, 1);
        assert_eq!(results[1].id, 2);
    }

    #[tokio::test]
    async fn test_category_upsert_replaces_previous() {
        let pool = init(":memory:").await.unwrap();
        for a in [sample_anime(1, "A"), sample_anime(2, "B"), sample_anime(3, "C")] {
            upsert_anime(&pool, &a).await.unwrap();
        }
        upsert_category(&pool, "trending", &[1, 2, 3], 100).await.unwrap();
        upsert_category(&pool, "trending", &[3, 1],    200).await.unwrap(); // replace

        let results = get_category(&pool, "trending").await.unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, 3); // position 0
        assert_eq!(results[1].id, 1); // position 1
    }

    #[tokio::test]
    async fn test_search_cache_by_title() {
        let pool = init(":memory:").await.unwrap();
        upsert_anime(&pool, &sample_anime(1, "Naruto")).await.unwrap();
        upsert_anime(&pool, &sample_anime(2, "Naruto Shippuden")).await.unwrap();
        upsert_anime(&pool, &sample_anime(3, "One Piece")).await.unwrap();

        let results = search_cache(&pool, "naruto").await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|a| a.id == 1));
        assert!(results.iter().any(|a| a.id == 2));
    }

    #[tokio::test]
    async fn test_store_cover_blob() {
        let pool  = init(":memory:").await.unwrap();
        let anime = sample_anime(1, "Frieren");
        upsert_anime(&pool, &anime).await.unwrap();

        let blob = vec![0xDE, 0xAD, 0xBE, 0xEF];
        store_cover_blob(&pool, 1, &blob).await.unwrap();

        let fetched = get_anime(&pool, 1).await.unwrap().unwrap();
        assert_eq!(fetched.cover_blob, Some(blob));
    }

    #[test]
    fn test_anime_display_title_prefers_english() {
        let mut anime = sample_anime(1, "Sousou no Frieren");
        anime.title_english = Some("Frieren: Beyond Journey's End".to_string());
        assert_eq!(anime.display_title(), "Frieren: Beyond Journey's End");
    }

    #[test]
    fn test_anime_display_title_falls_back_to_romaji() {
        let mut anime = sample_anime(1, "Sousou no Frieren");
        anime.title_english = None;
        assert_eq!(anime.display_title(), "Sousou no Frieren");
    }

    #[test]
    fn test_anime_short_title_truncates() {
        let mut anime = sample_anime(1, "A Very Long Anime Title That Should Be Truncated");
        anime.title_english = None;
        let short       = anime.short_title();
        let char_count  = short.chars().count();
        assert!(char_count <= 20, "Expected ≤20 chars, got {}", char_count); // 19 + '…'
        assert!(short.ends_with('…'));
    }

    #[test]
    fn test_anime_genre_list_decodes() {
        let anime = sample_anime(1, "Test");
        assert_eq!(anime.genre_list(), vec!["Action", "Drama"]);
    }

    #[test]
    fn test_anime_has_dub_flag() {
        let mut anime = sample_anime(1, "Test");
        anime.has_dub = 0;
        assert!(!anime.has_dub());
        anime.has_dub = 1;
        assert!(anime.has_dub());
    }
}
