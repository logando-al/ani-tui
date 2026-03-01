-- ─── CACHE LAYER ─────────────────────────────────────────────────────────────

CREATE TABLE IF NOT EXISTS anime (
    id              INTEGER PRIMARY KEY,   -- AniList ID
    title_english   TEXT,
    title_romaji    TEXT NOT NULL,
    title_native    TEXT,
    description     TEXT,
    episodes        INTEGER,
    status          TEXT,                  -- RELEASING, FINISHED, NOT_YET_RELEASED, CANCELLED
    season          TEXT,                  -- WINTER, SPRING, SUMMER, FALL
    season_year     INTEGER,
    score           INTEGER,               -- 0-100
    format          TEXT,                  -- TV, MOVIE, OVA, ONA, SPECIAL
    genres          TEXT NOT NULL DEFAULT '[]',  -- JSON array: ["Action","Drama"]
    cover_url       TEXT,
    cover_blob      BLOB,                  -- cached image bytes (downloaded once)
    has_dub         INTEGER NOT NULL DEFAULT 0,
    updated_at      INTEGER NOT NULL       -- unix timestamp
);

-- Category membership: which anime belong to which browsing row
CREATE TABLE IF NOT EXISTS category_entries (
    category        TEXT    NOT NULL,  -- 'trending' | 'popular' | 'top_rated' | 'seasonal'
    anime_id        INTEGER NOT NULL REFERENCES anime(id),
    position        INTEGER NOT NULL,
    refreshed_at    INTEGER NOT NULL,  -- unix timestamp (TTL source of truth)
    PRIMARY KEY (category, anime_id)
);

-- Genre membership: which anime belong to which genre row
CREATE TABLE IF NOT EXISTS genre_entries (
    genre           TEXT    NOT NULL,
    anime_id        INTEGER NOT NULL REFERENCES anime(id),
    position        INTEGER NOT NULL,
    refreshed_at    INTEGER NOT NULL,
    PRIMARY KEY (genre, anime_id)
);

-- ─── USER DATA ────────────────────────────────────────────────────────────────

-- Full watch history (one row per episode watched)
CREATE TABLE IF NOT EXISTS history (
    anime_id        INTEGER NOT NULL REFERENCES anime(id),
    episode         INTEGER NOT NULL,
    watched_at      INTEGER NOT NULL,      -- unix timestamp
    PRIMARY KEY (anime_id, episode)
);

-- Continue watching: latest episode per show
CREATE TABLE IF NOT EXISTS continue_watching (
    anime_id        INTEGER PRIMARY KEY REFERENCES anime(id),
    last_episode    INTEGER NOT NULL,
    last_watched    INTEGER NOT NULL       -- unix timestamp, used for row ordering
);

-- Watchlist / saved for later
CREATE TABLE IF NOT EXISTS watchlist (
    anime_id        INTEGER PRIMARY KEY REFERENCES anime(id),
    added_at        INTEGER NOT NULL
);

-- Preferred playback search query per anime (used to skip ambiguous ani-cli picks)
CREATE TABLE IF NOT EXISTS playback_prefs (
    anime_id        INTEGER PRIMARY KEY REFERENCES anime(id),
    query           TEXT    NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- Preferred audio mode per anime ("sub" / "dub")
CREATE TABLE IF NOT EXISTS audio_prefs (
    anime_id        INTEGER PRIMARY KEY REFERENCES anime(id),
    audio_mode      TEXT    NOT NULL,
    updated_at      INTEGER NOT NULL
);

-- ─── SYNC META ────────────────────────────────────────────────────────────────

-- Tracks when each category was last fetched from AniList
CREATE TABLE IF NOT EXISTS sync_meta (
    key             TEXT    PRIMARY KEY,   -- e.g. 'trending', 'popular', 'top_rated'
    synced_at       INTEGER NOT NULL       -- unix timestamp
);
