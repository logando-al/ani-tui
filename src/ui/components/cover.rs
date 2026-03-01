//! Anime cover art rendering.
//!
//! - Ghostty / kitty-protocol terminals → real image via ratatui-image
//! - COSMIC / other terminals           → colored halfblock with per-show unique color

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Widget,
};

// ─── Color generation ─────────────────────────────────────────────────────────

/// Convert HSL to RGB.  h ∈ [0,360), s ∈ [0,1], l ∈ [0,1].
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r1 + m) * 255.0) as u8,
        ((g1 + m) * 255.0) as u8,
        ((b1 + m) * 255.0) as u8,
    )
}

/// Deterministic color pair from anime ID.
/// Same ID → same colors every time (no flicker, no storage needed).
pub fn color_from_id(anime_id: i64) -> (Color, Color) {
    // Knuth multiplicative hash → hue in [0, 360)
    let hash    = (anime_id.unsigned_abs().wrapping_mul(2_654_435_761)) as u32;
    let hue     = (hash % 360) as f32;
    let hue2    = (hue + 40.0) % 360.0;

    let (r, g, b)    = hsl_to_rgb(hue,  0.65, 0.55); // primary — vibrant
    let (r2, g2, b2) = hsl_to_rgb(hue2, 0.50, 0.30); // secondary — darker complement

    (Color::Rgb(r, g, b), Color::Rgb(r2, g2, b2))
}

// ─── Halfblock cover widget ───────────────────────────────────────────────────

/// A colored halfblock cover widget for terminals without image protocol support.
/// Renders a two-tone block with the show's abbreviated title centered.
pub struct HalfblockCover<'a> {
    pub anime_id: i64,
    pub title:    &'a str,
}

impl<'a> Widget for HalfblockCover<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let (primary, secondary) = color_from_id(self.anime_id);
        let split                = area.height / 2;
        let title_row            = split.saturating_sub(1);

        for row in 0..area.height {
            let bg = if row < split { primary } else { secondary };

            for col in 0..area.width {
                let x = area.x + col;
                let y = area.y + row;

                if x < buf.area.width && y < buf.area.height {
                    let cell = buf.cell_mut((x, y)).unwrap();
                    cell.set_char('▓');
                    cell.set_style(Style::default().fg(bg).bg(bg));
                }
            }
        }

        // Centered title abbreviation (up to 8 chars)
        let abbrev   = abbreviated_title(self.title, area.width as usize);
        let text_len = abbrev.chars().count() as u16;
        let text_x   = area.x + area.width.saturating_sub(text_len) / 2;
        let text_y   = area.y + title_row;

        if text_y < buf.area.height {
            let (r, g, b) = hsl_to_rgb(0.0, 0.0, 1.0); // white text
            for (i, ch) in abbrev.chars().enumerate() {
                let cx = text_x + i as u16;
                if cx < buf.area.width {
                    let cell = buf.cell_mut((cx, text_y)).unwrap();
                    cell.set_char(ch);
                    cell.set_style(
                        Style::default()
                            .fg(Color::Rgb(r, g, b))
                            .bg(primary),
                    );
                }
            }
        }
    }
}

/// Produce a short title label for the cover (max `width` chars, uppercase initials).
fn abbreviated_title(title: &str, max_width: usize) -> String {
    // Try full title if it fits
    if title.len() <= max_width.saturating_sub(2) {
        return title.to_string();
    }

    // Initials of each word (max 4 letters)
    let initials: String = title
        .split_whitespace()
        .filter_map(|word| word.chars().next())
        .take(4)
        .collect::<String>()
        .to_uppercase();

    initials
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_from_id_is_deterministic() {
        let (a1, b1) = color_from_id(1535);
        let (a2, b2) = color_from_id(1535);
        assert_eq!(a1, a2);
        assert_eq!(b1, b2);
    }

    #[test]
    fn test_color_from_id_differs_per_show() {
        let (a1, _) = color_from_id(1);
        let (a2, _) = color_from_id(2);
        // Different IDs should (almost certainly) produce different colors
        assert_ne!(a1, a2);
    }

    #[test]
    fn test_color_from_id_handles_zero() {
        // Should not panic
        let _ = color_from_id(0);
    }

    #[test]
    fn test_color_from_id_handles_negative() {
        // Should not panic
        let _ = color_from_id(-999);
    }

    #[test]
    fn test_hsl_to_rgb_red() {
        let (r, g, b) = hsl_to_rgb(0.0, 1.0, 0.5);
        assert_eq!(r, 255);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
    }

    #[test]
    fn test_hsl_to_rgb_white() {
        let (r, g, b) = hsl_to_rgb(0.0, 0.0, 1.0);
        assert_eq!(r, 255);
        assert_eq!(g, 255);
        assert_eq!(b, 255);
    }

    #[test]
    fn test_hsl_to_rgb_black() {
        let (r, g, b) = hsl_to_rgb(0.0, 0.0, 0.0);
        assert_eq!(r, 0);
        assert_eq!(g, 0);
        assert_eq!(b, 0);
    }

    #[test]
    fn test_abbreviated_title_short_title_unchanged() {
        assert_eq!(abbreviated_title("Naruto", 20), "Naruto");
    }

    #[test]
    fn test_abbreviated_title_long_title_uses_initials() {
        // "Attack on Titan" → "AOT"
        let abbrev = abbreviated_title("Attack on Titan", 6);
        assert_eq!(abbrev, "AOT");
    }

    #[test]
    fn test_abbreviated_title_max_four_initials() {
        let abbrev = abbreviated_title("A Very Long Anime Title With Many Words", 4);
        assert!(abbrev.len() <= 4);
    }

    #[test]
    fn test_abbreviated_title_uppercase() {
        let abbrev = abbreviated_title("fullmetal alchemist brotherhood", 5);
        assert_eq!(abbrev, abbrev.to_uppercase());
    }
}
