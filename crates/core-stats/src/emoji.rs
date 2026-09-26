//! Detecção de emojis por grapheme cluster: tons de pele, bandeiras e sequências ZWJ
//! (👨‍👩‍👧) contam como um único emoji.

use crate::emoji_table::EXTENDED_PICTOGRAPHIC;
use unicode_segmentation::UnicodeSegmentation;

fn is_extended_pictographic(c: char) -> bool {
    let c = c as u32;
    EXTENDED_PICTOGRAPHIC
        .binary_search_by(|&(lo, hi)| {
            if hi < c {
                std::cmp::Ordering::Less
            } else if lo > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

fn is_regional_indicator(c: char) -> bool {
    ('\u{1F1E6}'..='\u{1F1FF}').contains(&c)
}

fn is_skin_tone(c: char) -> bool {
    ('\u{1F3FB}'..='\u{1F3FF}').contains(&c)
}

pub fn is_emoji(grapheme: &str) -> bool {
    let mut regional = 0;
    for c in grapheme.chars() {
        if is_extended_pictographic(c) || c == '\u{20E3}' {
            return true;
        }
        if is_regional_indicator(c) {
            regional += 1;
        }
    }
    regional == 2
}

/// Emojis de um texto como `(chave, forma de exibição)`. A chave ignora seletores de
/// variação (❤ e ❤️ contam juntos) e, opcionalmente, os tons de pele (👍🏽 conta como 👍).
pub fn emojis(text: &str, group_skin_tones: bool) -> impl Iterator<Item = (String, String)> + '_ {
    text.graphemes(true).filter(|g| is_emoji(g)).map(move |g| {
        let display: String = if group_skin_tones {
            g.chars().filter(|&c| !is_skin_tone(c)).collect()
        } else {
            g.to_string()
        };
        let key = display
            .chars()
            .filter(|&c| c != '\u{FE0F}' && c != '\u{FE0E}')
            .collect();
        (key, display)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(s: &str, group: bool) -> Vec<String> {
        emojis(s, group).map(|(k, _)| k).collect()
    }

    #[test]
    fn clusters() {
        assert_eq!(keys("oi 👨‍👩‍👧 kk 🇧🇷👍🏽", false), vec!["👨‍👩‍👧", "🇧🇷", "👍🏽"]);
        assert_eq!(keys("👍🏽👍", true), vec!["👍", "👍"]);
        assert_eq!(keys("❤️ ❤", false), vec!["❤", "❤"]);
        assert!(keys("texto normal 123 ?!", false).is_empty());
    }
}
