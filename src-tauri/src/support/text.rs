//! Comparing titles and names that different services spell differently.

use regex::Regex;
use std::sync::LazyLock;

static DECORATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*[(\[].*$").unwrap());
static NOT_ALNUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\p{L}\p{N}]+").unwrap());

/// Lower case, letters and digits only, without a trailing "(feat. …)" or
/// "[Remastered]".
fn normalize(s: &str) -> String {
    let lower = s.to_lowercase();
    NOT_ALNUM
        .replace_all(&DECORATION.replace(&lower, ""), "")
        .into_owned()
}

/// The same title or name, give or take decoration: equal, or one inside the
/// other once normalized. Two empty strings do not match.
pub fn loose_match(a: &str, b: &str) -> bool {
    let (x, y) = (normalize(a), normalize(b));
    !x.is_empty() && !y.is_empty() && (x.contains(&y) || y.contains(&x))
}

/// Equal ignoring case and runs of whitespace.
pub fn same_text(a: &str, b: &str) -> bool {
    let squash = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    squash(a) == squash(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loose_match_ignores_decoration() {
        assert!(loose_match("Sunflower (feat. Swae Lee)", "sunflower"));
        assert!(loose_match("The Cure", "Cure"));
        assert!(!loose_match("", ""));
        assert!(!loose_match("Abba", "Queen"));
    }

    #[test]
    fn same_text_squashes_spaces() {
        assert!(same_text("A  Song ", "a song"));
        assert!(!same_text("A Song", "A Song 2"));
    }
}
