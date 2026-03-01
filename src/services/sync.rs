//! Background sync service.
//! Checks TTL for each category, fetches from AniList if stale, writes to SQLite.

use crate::{
    api::anilist::AniListClient,
    db::{cache, sync as meta, user},
    error::Result,
    ui::home::HomeData,
};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

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
        let fetch_result = match category {
            meta::TRENDING  => client.trending(now).await,
            meta::POPULAR   => client.popular(now).await,
            meta::TOP_RATED => client.top_rated(now).await,
            meta::SEASONAL  => {
                let (season, year) = season_from_timestamp(now);
                client.seasonal(&season, year, now).await
            }
            other => {
                return Err(crate::error::AppError::Parse(
                    format!("Unknown category: {}", other),
                ))
            }
        };

        let anime_list = match fetch_result {
            Ok(anime_list) => anime_list,
            Err(_) => return cache::get_category(pool, category).await,
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

// ─── User-data helpers ────────────────────────────────────────────────────────

/// Resolve Continue Watching entries into full Anime structs (max 20, most recent first).
pub async fn load_continue_watching(pool: &SqlitePool) -> Result<Vec<cache::Anime>> {
    let entries = user::get_continue_watching(pool).await?;
    let mut result = Vec::new();
    for entry in entries.iter().take(20) {
        if let Some(anime) = cache::get_anime(pool, entry.anime_id).await? {
            result.push(anime);
        }
    }
    Ok(result)
}

/// Resolve Watchlist IDs into full Anime structs (max 20, most recently added first).
pub async fn load_watchlist(pool: &SqlitePool) -> Result<Vec<cache::Anime>> {
    let ids = user::get_watchlist(pool).await?;
    let mut result = Vec::new();
    for id in ids.iter().take(20) {
        if let Some(anime) = cache::get_anime(pool, *id).await? {
            result.push(anime);
        }
    }
    Ok(result)
}

fn collect_candidate_pool(
    trending:  &[cache::Anime],
    popular:   &[cache::Anime],
    top_rated: &[cache::Anime],
    seasonal:  &[cache::Anime],
) -> Vec<cache::Anime> {
    let mut candidates = Vec::new();
    let mut seen_ids = HashSet::new();
    for row in [trending, popular, top_rated, seasonal] {
        for anime in row {
            if seen_ids.insert(anime.id) {
                candidates.push(anime.clone());
            }
        }
    }

    candidates
}

fn best_reason(
    anime:          &cache::Anime,
    shared_genres:  i64,
    same_format:    bool,
    trending_ids:   &HashSet<i64>,
    popular_ids:    &HashSet<i64>,
    top_rated_ids:  &HashSet<i64>,
    seasonal_ids:   &HashSet<i64>,
) -> String {
    if shared_genres > 0 {
        "Shared genres".to_string()
    } else if same_format {
        format!("More {} {}", anime.format.as_deref().unwrap_or("anime"), "energy")
    } else if trending_ids.contains(&anime.id) {
        "Trending now".to_string()
    } else if top_rated_ids.contains(&anime.id) {
        "Top rated".to_string()
    } else if seasonal_ids.contains(&anime.id) {
        "Seasonal pick".to_string()
    } else if popular_ids.contains(&anime.id) {
        "Popular now".to_string()
    } else {
        "Picked for you".to_string()
    }
}

fn score_candidates(
    seeds:          &[cache::Anime],
    excluded_ids:   &HashSet<i64>,
    candidates:     Vec<cache::Anime>,
    trending:       &[cache::Anime],
    popular:        &[cache::Anime],
    top_rated:      &[cache::Anime],
    seasonal:       &[cache::Anime],
) -> Vec<(cache::Anime, String)> {
    if seeds.is_empty() {
        return Vec::new();
    }

    let seed_limit = seeds.len();

    let trending_ids: HashSet<i64> = trending.iter().map(|anime| anime.id).collect();
    let popular_ids: HashSet<i64> = popular.iter().map(|anime| anime.id).collect();
    let top_rated_ids: HashSet<i64> = top_rated.iter().map(|anime| anime.id).collect();
    let seasonal_ids: HashSet<i64> = seasonal.iter().map(|anime| anime.id).collect();

    let mut scored = Vec::new();
    for anime in candidates {
        if excluded_ids.contains(&anime.id) {
            continue;
        }

        let candidate_genres: HashSet<String> = anime.genre_list().into_iter().collect();
        let mut score = anime.score.unwrap_or(0) / 10;
        let mut best_shared_genres = 0i64;
        let mut best_same_format = false;

        for (idx, seed) in seeds.iter().take(seed_limit).enumerate() {
            let weight = (seed_limit - idx) as i64;
            let seed_genres: HashSet<String> = seed.genre_list().into_iter().collect();
            let shared_genres = seed_genres.intersection(&candidate_genres).count() as i64;
            best_shared_genres = best_shared_genres.max(shared_genres);
            score += shared_genres * weight * 3;

            if seed.format == anime.format && anime.format.is_some() {
                best_same_format = true;
                score += weight * 2;
            }
            if seed.season == anime.season && anime.season.is_some() {
                score += weight;
            }
            if let (Some(seed_year), Some(candidate_year)) = (seed.season_year, anime.season_year) {
                if (seed_year - candidate_year).abs() <= 1 {
                    score += weight;
                }
            }
        }

        if trending_ids.contains(&anime.id) {
            score += 4;
        }
        if popular_ids.contains(&anime.id) {
            score += 3;
        }
        if top_rated_ids.contains(&anime.id) {
            score += 3;
        }
        if seasonal_ids.contains(&anime.id) {
            score += 2;
        }

        if score > 0 {
            let reason = best_reason(
                &anime,
                best_shared_genres,
                best_same_format,
                &trending_ids,
                &popular_ids,
                &top_rated_ids,
                &seasonal_ids,
            );
            scored.push((score, anime, reason));
        }
    }

    scored.sort_by(|(left_score, left_anime, _), (right_score, right_anime, _)| {
        right_score
            .cmp(left_score)
            .then_with(|| right_anime.score.unwrap_or(0).cmp(&left_anime.score.unwrap_or(0)))
            .then_with(|| left_anime.display_title().cmp(right_anime.display_title()))
    });

    scored
        .into_iter()
        .take(20)
        .map(|(_, anime, reason)| (anime, reason))
        .collect()
}

/// Build a lightweight "Because You Watched" row from recent watch behavior.
fn build_recommendations(
    continue_watching: &[cache::Anime],
    watchlist:         &[cache::Anime],
    trending:          &[cache::Anime],
    popular:           &[cache::Anime],
    top_rated:         &[cache::Anime],
    seasonal:          &[cache::Anime],
) -> (Vec<cache::Anime>, std::collections::HashMap<i64, String>) {
    let seeds: Vec<cache::Anime> = continue_watching.iter().take(5).cloned().collect();
    let mut excluded_ids: HashSet<i64> = continue_watching.iter().map(|anime| anime.id).collect();
    excluded_ids.extend(watchlist.iter().map(|anime| anime.id));

    let scored = score_candidates(
        &seeds,
        &excluded_ids,
        collect_candidate_pool(trending, popular, top_rated, seasonal),
        trending,
        popular,
        top_rated,
        seasonal,
    );

    let mut reasons = std::collections::HashMap::new();
    let mut items = Vec::new();
    for (anime, reason) in scored {
        reasons.insert(anime.id, reason);
        items.push(anime);
    }
    (items, reasons)
}

fn build_progress_labels(
    anime_rows: &[&[cache::Anime]],
    watched_counts: &HashMap<i64, usize>,
    resume_next: &HashMap<i64, u32>,
) -> HashMap<i64, String> {
    let mut labels = HashMap::new();
    let mut seen = HashSet::new();

    for row in anime_rows {
        for anime in *row {
            if !seen.insert(anime.id) {
                continue;
            }

            if let Some(next_ep) = resume_next.get(&anime.id) {
                labels.insert(anime.id, format!("E{} next", next_ep));
                continue;
            }

            let watched = watched_counts.get(&anime.id).copied().unwrap_or(0);
            if watched == 0 {
                continue;
            }

            let label = match anime.episodes {
                Some(total) => format!("{}/{}", watched, total),
                None => format!("{} seen", watched),
            };
            labels.insert(anime.id, label);
        }
    }

    labels
}

/// Build "More Like This" from the currently selected anime against the cached catalog.
pub async fn load_more_like_this(
    pool:  &SqlitePool,
    anime: &cache::Anime,
) -> Result<Vec<(cache::Anime, String)>> {
    let trending = cache::get_category(pool, meta::TRENDING).await.unwrap_or_default();
    let popular = cache::get_category(pool, meta::POPULAR).await.unwrap_or_default();
    let top_rated = cache::get_category(pool, meta::TOP_RATED).await.unwrap_or_default();
    let seasonal = cache::get_category(pool, meta::SEASONAL).await.unwrap_or_default();

    let mut excluded_ids = HashSet::from([anime.id]);
    excluded_ids.extend(
        user::get_continue_watching(pool)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|entry| entry.anime_id),
    );
    excluded_ids.extend(user::get_watchlist(pool).await.unwrap_or_default());

    Ok(score_candidates(
        std::slice::from_ref(anime),
        &excluded_ids,
        collect_candidate_pool(&trending, &popular, &top_rated, &seasonal),
        &trending,
        &popular,
        &top_rated,
        &seasonal,
    ))
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
    // Run all syncs concurrently — each is independent
    let (trending, popular, top_rated, seasonal, continue_entries, continue_watching, watchlist) = tokio::join!(
        sync_category(pool, client, meta::TRENDING,  trending_ttl, now),
        sync_category(pool, client, meta::POPULAR,   stable_ttl,   now),
        sync_category(pool, client, meta::TOP_RATED, stable_ttl,   now),
        sync_category(pool, client, meta::SEASONAL,  trending_ttl, now),
        user::get_continue_watching(pool),
        load_continue_watching(pool),
        load_watchlist(pool),
    );

    let trending          = trending.unwrap_or_default();
    let popular           = popular.unwrap_or_default();
    let top_rated         = top_rated.unwrap_or_default();
    let seasonal          = seasonal.unwrap_or_default();
    let continue_entries  = continue_entries.unwrap_or_default();
    let continue_watching = continue_watching.unwrap_or_default();
    let watchlist         = watchlist.unwrap_or_default();
    let featured          = trending.first().cloned();
    let (recommended, recommended_reasons) = build_recommendations(
        &continue_watching,
        &watchlist,
        &trending,
        &popular,
        &top_rated,
        &seasonal,
    );
    let resume_next: HashMap<i64, u32> = continue_entries
        .into_iter()
        .map(|entry| {
            let next = continue_watching
                .iter()
                .find(|anime| anime.id == entry.anime_id)
                .map(|anime| {
                    let next = (entry.last_episode as u32).saturating_add(1);
                    match anime.episodes {
                        Some(total) => next.min(total as u32).max(1),
                        None => next.max(1),
                    }
                })
                .unwrap_or_else(|| (entry.last_episode as u32).saturating_add(1).max(1));
            (entry.anime_id, next)
        })
        .collect();
    let mut all_ids = Vec::new();
    for row in [
        &continue_watching,
        &watchlist,
        &recommended,
        &trending,
        &popular,
        &top_rated,
        &seasonal,
    ] {
        all_ids.extend(row.iter().map(|anime| anime.id));
    }
    all_ids.sort_unstable();
    all_ids.dedup();
    let watched_counts = user::get_watched_counts(pool, &all_ids).await.unwrap_or_default();
    let progress_labels = build_progress_labels(
        &[
            &continue_watching,
            &watchlist,
            &recommended,
            &trending,
            &popular,
            &top_rated,
            &seasonal,
        ],
        &watched_counts,
        &resume_next,
    );

    Ok(HomeData {
        featured,
        continue_watching,
        watchlist,
        recommended,
        recommended_reasons,
        progress_labels,
        resume_next,
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

    // ── load_continue_watching ────────────────────────────────────────────────

    fn make_anime(_pool_ref: &sqlx::SqlitePool, id: i64) -> cache::Anime {
        cache::Anime {
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
        }
    }

    fn scored_anime(id: i64, title: &str, genres: &[&str], format: &str, score: i64) -> cache::Anime {
        cache::Anime {
            id,
            title_english: Some(title.to_string()),
            title_romaji:  title.to_string(),
            title_native:  None,
            description:   None,
            episodes:      Some(12),
            status:        Some("FINISHED".into()),
            season:        Some("SPRING".into()),
            season_year:   Some(2024),
            score:         Some(score),
            format:        Some(format.into()),
            genres:        serde_json::to_string(&genres).unwrap(),
            cover_url:     None,
            cover_blob:    None,
            has_dub:       0,
            updated_at:    1_000_000,
        }
    }

    #[tokio::test]
    async fn test_load_continue_watching_returns_anime() {
        use crate::db::{init, cache::upsert_anime};
        let pool = init(":memory:").await.unwrap();
        let anime = make_anime(&pool, 1);
        upsert_anime(&pool, &anime).await.unwrap();
        user::record_watched(&pool, 1, 3, 1_000_000).await.unwrap();

        let result = load_continue_watching(&pool).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
    }

    #[tokio::test]
    async fn test_load_continue_watching_empty_when_no_history() {
        use crate::db::init;
        let pool = init(":memory:").await.unwrap();
        let result = load_continue_watching(&pool).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_load_continue_watching_ordered_by_recency() {
        use crate::db::{init, cache::upsert_anime};
        let pool = init(":memory:").await.unwrap();
        for id in [1i64, 2i64] {
            upsert_anime(&pool, &make_anime(&pool, id)).await.unwrap();
        }
        user::record_watched(&pool, 1, 1, 1_000).await.unwrap();
        user::record_watched(&pool, 2, 1, 2_000).await.unwrap(); // more recent

        let result = load_continue_watching(&pool).await.unwrap();
        assert_eq!(result[0].id, 2); // most recent first
        assert_eq!(result[1].id, 1);
    }

    // ── load_watchlist ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_load_watchlist_returns_anime() {
        use crate::db::{init, cache::upsert_anime};
        let pool = init(":memory:").await.unwrap();
        upsert_anime(&pool, &make_anime(&pool, 1)).await.unwrap();
        user::add_to_watchlist(&pool, 1, 1_000_000).await.unwrap();

        let result = load_watchlist(&pool).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
    }

    #[tokio::test]
    async fn test_load_watchlist_empty_when_none_added() {
        use crate::db::init;
        let pool = init(":memory:").await.unwrap();
        let result = load_watchlist(&pool).await.unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_build_recommendations_prefers_shared_genres() {
        let continue_watching = vec![scored_anime(1, "Seed", &["Action", "Drama"], "TV", 85)];
        let watchlist = Vec::new();
        let trending = vec![
            scored_anime(2, "Strong Match", &["Action", "Drama"], "TV", 80),
            scored_anime(3, "Weak Match", &["Slice of Life"], "TV", 95),
        ];

        let (recommended, reasons) = build_recommendations(
            &continue_watching,
            &watchlist,
            &trending,
            &[],
            &[],
            &[],
        );

        assert_eq!(recommended.first().map(|anime| anime.id), Some(2));
        assert!(!recommended.iter().any(|anime| anime.id == 1));
        assert_eq!(reasons.get(&2).map(String::as_str), Some("Shared genres"));
    }

    #[test]
    fn test_build_recommendations_skips_watchlist_items() {
        let continue_watching = vec![scored_anime(1, "Seed", &["Action"], "TV", 85)];
        let watchlist = vec![scored_anime(2, "Saved", &["Action"], "TV", 90)];
        let trending = vec![scored_anime(2, "Saved", &["Action"], "TV", 90)];

        let (recommended, _reasons) = build_recommendations(
            &continue_watching,
            &watchlist,
            &trending,
            &[],
            &[],
            &[],
        );

        assert!(recommended.is_empty());
    }
}
