//! Background sync service.
//! Checks TTL for each category, fetches from AniList if stale, writes to SQLite.

use crate::{
    api::anilist::AniListClient,
    db::{cache, sync as meta},
    error::Result,
    ui::home::HomeData,
};
use sqlx::SqlitePool;

// ─── Season helpers ───────────────────────────────────────────────────────────

/// Derive the current anime season and year from a unix timestamp.
/// Season boundaries: WINTER=Jan-Mar, SPRING=Apr-Jun, SUMMER=Jul-Sep, FALL=Oct-Dec.
pub fn season_from_timestamp(unix_ts: i64) -> (String, i64) {
    // Approximate: seconds → days → years. Good enough for seasonal display.
    const SECS_PER_DAY:  i64 = 86_400;
    const DAYS_PER_YEAR: i64 = 365;

    let total_days   = unix_ts / SECS_PER_DAY;
    let year         = 1970 + total_days / DAYS_PER_YEAR;
    let day_of_year  = total_days % DAYS_PER_YEAR;

    // Rough month from day-of-year
    let month = match day_of_year {
        0..=30   => 1,
        31..=58  => 2,
        59..=89  => 3,
        90..=119 => 4,
        120..=150 => 5,
        151..=180 => 6,
        181..=211 => 7,
        212..=242 => 8,
        243..=272 => 9,
        273..=303 => 10,
        304..=333 => 11,
        _         => 12,
    };

    let season = match month {
        1 | 2 | 3  => "WINTER",
        4 | 5 | 6  => "SPRING",
        7 | 8 | 9  => "SUMMER",
        _           => "FALL",
    };

    (season.to_string(), year)
}

// ─── Sync one category ────────────────────────────────────────────────────────

/// Fetch a single category from AniList if stale, cache in SQLite, return list.
/// If fresh, reads directly from SQLite (no network call).
pub async fn sync_category(
    pool:     &SqlitePool,
    client:   &AniListClient,
    category: &str,
    ttl:      u64,
    now:      i64,
) -> Result<Vec<cache::Anime>> {
    if meta::is_stale(pool, category, ttl, now).await? {
        let anime_list = match category {
            meta::TRENDING  => client.trending(now).await?,
            meta::POPULAR   => client.popular(now).await?,
            meta::TOP_RATED => client.top_rated(now).await?,
            meta::SEASONAL  => {
                let (season, year) = season_from_timestamp(now);
                client.seasonal(&season, year, now).await?
            }
            other => {
                return Err(crate::error::AppError::Parse(
                    format!("Unknown category: {}", other),
                ))
            }
        };

        // Persist each anime and the category ordering
        for anime in &anime_list {
            cache::upsert_anime(pool, anime).await?;
        }
        let ids: Vec<i64> = anime_list.iter().map(|a| a.id).collect();
        cache::upsert_category(pool, category, &ids, now).await?;
        meta::mark_synced(pool, category, now).await?;
    }

    cache::get_category(pool, category).await
}

// ─── Sync all + build HomeData ────────────────────────────────────────────────

/// Sync every category (respecting TTLs) and return a fully populated HomeData.
/// Called once on startup and again when the user explicitly refreshes.
pub async fn sync_all(
    pool:             &SqlitePool,
    client:           &AniListClient,
    trending_ttl:     u64,
    stable_ttl:       u64,
    now:              i64,
) -> Result<HomeData> {
    // Run all category syncs — each is independent (one failure doesn't block others)
    let (trending, popular, top_rated, seasonal) = tokio::join!(
        sync_category(pool, client, meta::TRENDING,  trending_ttl, now),
        sync_category(pool, client, meta::POPULAR,   stable_ttl,   now),
        sync_category(pool, client, meta::TOP_RATED, stable_ttl,   now),
        sync_category(pool, client, meta::SEASONAL,  trending_ttl, now),
    );

    let trending  = trending.unwrap_or_default();
    let popular   = popular.unwrap_or_default();
    let top_rated = top_rated.unwrap_or_default();
    let seasonal  = seasonal.unwrap_or_default();
    let featured  = trending.first().cloned();

    Ok(HomeData {
        featured,
        continue_watching: vec![], // populated separately from user data
        trending,
        popular,
        top_rated,
        seasonal,
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── season_from_timestamp ─────────────────────────────────────────────────

    #[test]
    fn test_winter_january() {
        // 2026-01-15 = 20469 days from epoch → unix 1_768_521_600
        let ts = 1_768_521_600i64;
        let (season, year) = season_from_timestamp(ts);
        assert_eq!(season, "WINTER");
        assert_eq!(year, 2026);
    }

    #[test]
    fn test_spring_april() {
        // 2026-04-10 = 20554 days from epoch → unix 1_775_865_600
        let ts = 1_775_865_600i64;
        let (season, _) = season_from_timestamp(ts);
        assert_eq!(season, "SPRING");
    }

    #[test]
    fn test_summer_july() {
        // 2026-07-15 = 20650 days from epoch → unix 1_784_160_000
        let ts = 1_784_160_000i64;
        let (season, _) = season_from_timestamp(ts);
        assert_eq!(season, "SUMMER");
    }

    #[test]
    fn test_fall_october() {
        // 2026-10-20 = 20747 days from epoch → unix 1_792_540_800
        let ts = 1_792_540_800i64;
        let (season, _) = season_from_timestamp(ts);
        assert_eq!(season, "FALL");
    }

    #[test]
    fn test_season_year_is_reasonable() {
        // 2026-03-01 ≈ unix 1_740_787_200 → should be in 2025-2026 range
        let ts = 1_740_787_200i64;
        let (_, year) = season_from_timestamp(ts);
        assert!(year >= 2025 && year <= 2026);
    }

    // ── sync_category (SQLite path, no HTTP) ─────────────────────────────────

    #[tokio::test]
    async fn test_sync_category_returns_empty_when_no_cache_and_no_client() {
        // When cache is empty and category is fresh (never stale), returns empty.
        // We can't easily mock AniListClient without trait objects — test the fresh path.
        use crate::db::init;
        let pool = init(":memory:").await.unwrap();
        let now  = 1_700_000_000i64;

        // Mark as already synced (so is_stale = false → skip API call)
        meta::mark_synced(&pool, meta::TRENDING, now).await.unwrap();

        // Fresh path: reads from SQLite (empty) → returns []
        let client  = AniListClient::new();
        let results = sync_category(&pool, &client, meta::TRENDING, 86_400, now + 60).await.unwrap();
        assert!(results.is_empty(), "Cache is empty, fresh sync returns empty vec");
    }

    #[tokio::test]
    async fn test_sync_category_reads_from_sqlite_when_fresh() {
        use crate::db::{cache::upsert_anime, init};
        let pool = init(":memory:").await.unwrap();
        let now  = 1_700_000_000i64;

        // Insert anime and category
        let anime = cache::Anime {
            id:            1,
            title_english: Some("Cached Anime".into()),
            title_romaji:  "Cached Anime".into(),
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
            updated_at:    now,
        };
        upsert_anime(&pool, &anime).await.unwrap();
        cache::upsert_category(&pool, meta::TRENDING, &[1], now).await.unwrap();
        meta::mark_synced(&pool, meta::TRENDING, now).await.unwrap();

        // Should return cached data without hitting network
        let client  = AniListClient::new();
        let results = sync_category(&pool, &client, meta::TRENDING, 86_400, now + 60).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, 1);
    }
}
