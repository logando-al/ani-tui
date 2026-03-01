//! AniList GraphQL API client.
//! Fetches trending, popular, top-rated, seasonal, and search results.

use crate::{
    db::cache::Anime,
    error::{AppError, Result},
};
use serde::{Deserialize, Serialize};

const ANILIST_API: &str = "https://graphql.anilist.co";

/// The GraphQL query for fetching a page of anime with full metadata.
const MEDIA_FIELDS: &str = r#"
    id
    title { english romaji native }
    description(asHtml: false)
    episodes
    status
    season
    seasonYear
    averageScore
    format
    genres
    coverImage { large }
    countryOfOrigin
"#;

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct GraphQLRequest<'a> {
    query:     &'a str,
    variables: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct PageResponse {
    data: PageData,
}

#[derive(Debug, Deserialize)]
struct PageData {
    #[serde(rename = "Page")]
    page: PagePayload,
}

#[derive(Debug, Deserialize)]
struct PagePayload {
    media: Vec<RawAnime>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(rename = "data")]
    _data: SearchData,
}

#[derive(Debug, Deserialize)]
struct SearchData {
    #[serde(rename = "Page")]
    _page: PagePayload,
}

/// Raw anime as returned by AniList — mapped to our `Anime` model.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawAnime {
    id:            i64,
    title:         RawTitle,
    description:   Option<String>,
    episodes:      Option<i64>,
    status:        Option<String>,
    season:        Option<String>,
    season_year:   Option<i64>,
    average_score: Option<i64>,
    format:        Option<String>,
    genres:        Vec<String>,
    cover_image:   Option<RawCoverImage>,
}

#[derive(Debug, Deserialize)]
struct RawTitle {
    english: Option<String>,
    romaji:  Option<String>,
    native:  Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawCoverImage {
    large: Option<String>,
}

impl RawAnime {
    /// Convert AniList response into our canonical Anime model.
    fn into_anime(self, now: i64) -> Anime {
        Anime {
            id:            self.id,
            title_english: self.title.english,
            title_romaji:  self.title.romaji.unwrap_or_else(|| "Unknown".to_string()),
            title_native:  self.title.native,
            description:   self.description,
            episodes:      self.episodes,
            status:        self.status,
            season:        self.season,
            season_year:   self.season_year,
            score:         self.average_score,
            format:        self.format,
            genres:        serde_json::to_string(&self.genres).unwrap_or_else(|_| "[]".to_string()),
            cover_url:     self.cover_image.and_then(|c| c.large),
            cover_blob:    None,
            has_dub:       0, // AniList doesn't expose dub info reliably
            updated_at:    now,
        }
    }
}

// ─── Client ───────────────────────────────────────────────────────────────────

/// AniList API client. Wraps reqwest and converts results to our Anime model.
pub struct AniListClient {
    http: reqwest::Client,
}

impl AniListClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    /// Low-level GraphQL POST. Returns raw JSON value.
    async fn gql(&self, query: &str, variables: serde_json::Value) -> Result<serde_json::Value> {
        let body = GraphQLRequest { query, variables };
        let resp = self
            .http
            .post(ANILIST_API)
            .json(&body)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        Ok(resp)
    }

    /// Fetch trending anime (updated daily).
    pub async fn trending(&self, now: i64) -> Result<Vec<Anime>> {
        let query = format!(
            r#"query {{ Page(page: 1, perPage: 30) {{ media(sort: TRENDING_DESC, type: ANIME, isAdult: false) {{ {} }} }} }}"#,
            MEDIA_FIELDS
        );
        self.fetch_page(&query, serde_json::json!({}), now).await
    }

    /// Fetch all-time popular anime.
    pub async fn popular(&self, now: i64) -> Result<Vec<Anime>> {
        let query = format!(
            r#"query {{ Page(page: 1, perPage: 30) {{ media(sort: POPULARITY_DESC, type: ANIME, isAdult: false) {{ {} }} }} }}"#,
            MEDIA_FIELDS
        );
        self.fetch_page(&query, serde_json::json!({}), now).await
    }

    /// Fetch top-rated anime of all time.
    pub async fn top_rated(&self, now: i64) -> Result<Vec<Anime>> {
        let query = format!(
            r#"query {{ Page(page: 1, perPage: 30) {{ media(sort: SCORE_DESC, type: ANIME, isAdult: false) {{ {} }} }} }}"#,
            MEDIA_FIELDS
        );
        self.fetch_page(&query, serde_json::json!({}), now).await
    }

    /// Fetch currently airing seasonal anime.
    pub async fn seasonal(&self, season: &str, year: i64, now: i64) -> Result<Vec<Anime>> {
        let query = format!(
            r#"query($season: MediaSeason, $year: Int) {{
                Page(page: 1, perPage: 30) {{
                    media(season: $season, seasonYear: $year, sort: POPULARITY_DESC, type: ANIME, isAdult: false) {{
                        {}
                    }}
                }}
            }}"#,
            MEDIA_FIELDS
        );
        let vars = serde_json::json!({ "season": season, "year": year });
        self.fetch_page(&query, vars, now).await
    }

    /// Search anime by title.
    pub async fn search(&self, query_str: &str, now: i64) -> Result<Vec<Anime>> {
        let query = format!(
            r#"query($search: String) {{
                Page(page: 1, perPage: 30) {{
                    media(search: $search, type: ANIME, isAdult: false, sort: SEARCH_MATCH) {{
                        {}
                    }}
                }}
            }}"#,
            MEDIA_FIELDS
        );
        let vars = serde_json::json!({ "search": query_str });
        self.fetch_page(&query, vars, now).await
    }

    /// Internal: POST query → parse Page response → convert to Vec<Anime>.
    async fn fetch_page(
        &self,
        query:     &str,
        variables: serde_json::Value,
        now:       i64,
    ) -> Result<Vec<Anime>> {
        let raw  = self.gql(query, variables).await?;
        let page: PageResponse = serde_json::from_value(raw)
            .map_err(|e| AppError::Parse(format!("AniList page parse error: {e}")))?;
        let anime = page
            .data
            .page
            .media
            .into_iter()
            .map(|r| r.into_anime(now))
            .collect();
        Ok(anime)
    }
}

impl Default for AniListClient {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Pure unit tests (no HTTP) ────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_anime_fixture(id: i64, english: Option<&str>, romaji: &str) -> RawAnime {
        RawAnime {
            id,
            title:         RawTitle {
                english: english.map(str::to_string),
                romaji:  Some(romaji.to_string()),
                native:  None,
            },
            description:   Some("A test anime".to_string()),
            episodes:      Some(24),
            status:        Some("FINISHED".to_string()),
            season:        Some("FALL".to_string()),
            season_year:   Some(2023),
            average_score: Some(87),
            format:        Some("TV".to_string()),
            genres:        vec!["Action".to_string(), "Drama".to_string()],
            cover_image:   Some(RawCoverImage {
                large: Some("https://example.com/cover.jpg".to_string()),
            }),
        }
    }

    #[test]
    fn test_raw_anime_into_anime_with_english_title() {
        let raw   = raw_anime_fixture(1535, Some("Sword Art Online"), "Sword Art Online");
        let anime = raw.into_anime(1_700_000_000);

        assert_eq!(anime.id, 1535);
        assert_eq!(anime.title_english, Some("Sword Art Online".to_string()));
        assert_eq!(anime.title_romaji,  "Sword Art Online");
        assert_eq!(anime.score,         Some(87));
        assert_eq!(anime.episodes,      Some(24));
    }

    #[test]
    fn test_raw_anime_into_anime_without_english_title() {
        let raw   = raw_anime_fixture(1, None, "Sousou no Frieren");
        let anime = raw.into_anime(0);

        assert!(anime.title_english.is_none());
        assert_eq!(anime.title_romaji, "Sousou no Frieren");
    }

    #[test]
    fn test_raw_anime_genres_serialized_to_json() {
        let raw   = raw_anime_fixture(1, None, "Test");
        let anime = raw.into_anime(0);

        let genres: Vec<String> = serde_json::from_str(&anime.genres).unwrap();
        assert_eq!(genres, vec!["Action", "Drama"]);
    }

    #[test]
    fn test_raw_anime_cover_url_extracted() {
        let raw   = raw_anime_fixture(1, None, "Test");
        let anime = raw.into_anime(0);
        assert_eq!(anime.cover_url, Some("https://example.com/cover.jpg".to_string()));
    }

    #[test]
    fn test_raw_anime_no_cover_image() {
        let mut raw      = raw_anime_fixture(1, None, "Test");
        raw.cover_image  = None;
        let anime        = raw.into_anime(0);
        assert!(anime.cover_url.is_none());
    }

    #[test]
    fn test_raw_anime_empty_romaji_uses_unknown() {
        let raw = RawAnime {
            id:            99,
            title:         RawTitle { english: None, romaji: None, native: None },
            description:   None,
            episodes:      None,
            status:        None,
            season:        None,
            season_year:   None,
            average_score: None,
            format:        None,
            genres:        vec![],
            cover_image:   None,
        };
        let anime = raw.into_anime(0);
        assert_eq!(anime.title_romaji, "Unknown");
    }

    #[test]
    fn test_raw_anime_has_dub_defaults_to_zero() {
        let raw   = raw_anime_fixture(1, None, "Test");
        let anime = raw.into_anime(0);
        assert_eq!(anime.has_dub, 0);
    }

    #[test]
    fn test_raw_anime_updated_at_is_set() {
        let raw   = raw_anime_fixture(1, None, "Test");
        let now   = 1_700_123_456i64;
        let anime = raw.into_anime(now);
        assert_eq!(anime.updated_at, now);
    }
}
