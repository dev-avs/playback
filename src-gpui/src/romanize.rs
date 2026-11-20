//! Romanization for GPUI: Japanese kana → modified Hepburn, Korean hangul →
//! Revised Romanization, with Latin-only skip guard so English lines are never
//! duplicated or mangled.

pub fn has_non_latin(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c as u32,
            0x3040..=0x309F // Hiragana
            | 0x30A0..=0x30FF // Katakana
            | 0xAC00..=0xD7A3 // Hangul
            | 0x4E00..=0x9FFF // CJK Unified Ideographs
            | 0x3400..=0x4DBF // CJK Extension A
        )
    })
}

fn katakana_to_hiragana(text: &str) -> String {
    text.chars()
        .map(|c| {
            let u = c as u32;
            if (0x30A1..=0x30F6).contains(&u) {
                char::from_u32(u - 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

fn digraph(pair: &str) -> Option<&'static str> {
    Some(match pair {
        "きゃ" => "kya",
        "きゅ" => "kyu",
        "きょ" => "kyo",
        "ぎゃ" => "gya",
        "ぎゅ" => "gyu",
        "ぎょ" => "gyo",
        "しゃ" => "sha",
        "しゅ" => "shu",
        "しょ" => "sho",
        "しぇ" => "she",
        "じゃ" => "ja",
        "じゅ" => "ju",
        "じょ" => "jo",
        "じぇ" => "je",
        "ちゃ" => "cha",
        "ちゅ" => "chu",
        "ちょ" => "cho",
        "ちぇ" => "che",
        "ぢゃ" => "ja",
        "ぢゅ" => "ju",
        "ぢょ" => "jo",
        "にゃ" => "nya",
        "にゅ" => "nyu",
        "にょ" => "nyo",
        "ひゃ" => "hya",
        "ひゅ" => "hyu",
        "ひょ" => "hyo",
        "びゃ" => "bya",
        "びゅ" => "byu",
        "びょ" => "byo",
        "ぴゃ" => "pya",
        "ぴゅ" => "pyu",
        "ぴょ" => "pyo",
        "みゃ" => "mya",
        "みゅ" => "myu",
        "みょ" => "myo",
        "りゃ" => "rya",
        "りゅ" => "ryu",
        "りょ" => "ryo",
        "ふぁ" => "fa",
        "ふぃ" => "fi",
        "ふぇ" => "fe",
        "ふぉ" => "fo",
        "うぃ" => "wi",
        "うぇ" => "we",
        "うぉ" => "wo",
        "てぃ" => "ti",
        "でぃ" => "di",
        _ => return None,
    })
}

fn kana(c: char) -> Option<&'static str> {
    Some(match c {
        'あ' | 'ぁ' => "a",
        'い' | 'ぃ' => "i",
        'う' | 'ぅ' => "u",
        'え' | 'ぇ' => "e",
        'お' | 'ぉ' => "o",
        'か' => "ka",
        'き' => "ki",
        'く' => "ku",
        'け' => "ke",
        'こ' => "ko",
        'が' => "ga",
        'ぎ' => "gi",
        'ぐ' => "gu",
        'げ' => "ge",
        'ご' => "go",
        'さ' => "sa",
        'し' => "shi",
        'す' => "su",
        'せ' => "se",
        'そ' => "so",
        'ざ' => "za",
        'じ' => "ji",
        'ず' => "zu",
        'ぜ' => "ze",
        'ぞ' => "zo",
        'た' => "ta",
        'ち' => "chi",
        'つ' => "tsu",
        'て' => "te",
        'と' => "to",
        'だ' => "da",
        'ぢ' => "ji",
        'づ' => "zu",
        'で' => "de",
        'ど' => "do",
        'な' => "na",
        'に' => "ni",
        'ぬ' => "nu",
        'ね' => "ne",
        'の' => "no",
        'は' => "ha",
        'ひ' => "hi",
        'ふ' => "fu",
        'へ' => "he",
        'ほ' => "ho",
        'ば' => "ba",
        'び' => "bi",
        'ぶ' => "bu",
        'べ' => "be",
        'ぼ' => "bo",
        'ぱ' => "pa",
        'ぴ' => "pi",
        'ぷ' => "pu",
        'ぺ' => "pe",
        'ぽ' => "po",
        'ま' => "ma",
        'み' => "mi",
        'む' => "mu",
        'め' => "me",
        'も' => "mo",
        'や' | 'ゃ' => "ya",
        'ゆ' | 'ゅ' => "yu",
        'よ' | 'ょ' => "yo",
        'ら' => "ra",
        'り' => "ri",
        'る' => "ru",
        'れ' => "re",
        'ろ' => "ro",
        'わ' => "wa",
        'ゐ' => "wi",
        'ゑ' => "we",
        'を' => "o",
        'ん' => "n",
        'ゔ' => "vu",
        _ => return None,
    })
}

fn romanize_japanese(input: &str) -> String {
    let hira = katakana_to_hiragana(input);
    let chars: Vec<char> = hira.chars().collect();
    let mut out = String::new();
    let mut sokuon = false;
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        let mut roma = None;
        if i + 1 < chars.len() {
            let pair: String = [ch, chars[i + 1]].into_iter().collect();
            if let Some(d) = digraph(&pair) {
                roma = Some(d);
                i += 1;
            }
        }
        if roma.is_none() {
            if ch == 'っ' {
                sokuon = true;
                i += 1;
                continue;
            } else if ch == 'ー' {
                if let Some(last) = out.chars().last() {
                    if matches!(last, 'a' | 'i' | 'u' | 'e' | 'o') {
                        out.push(last);
                    }
                }
                i += 1;
                continue;
            } else {
                roma = kana(ch);
            }
        }
        let Some(r) = roma else {
            out.push(ch);
            sokuon = false;
            i += 1;
            continue;
        };
        if sokuon {
            out.push(if r.starts_with("ch") {
                't'
            } else {
                r.chars().next().unwrap_or('t')
            });
            sokuon = false;
        }
        out.push_str(r);
        i += 1;
    }
    out
}

const INITIALS: &[&str] = &[
    "g", "kk", "n", "d", "tt", "r", "m", "b", "pp", "s", "ss", "", "j", "jj", "ch", "k", "t", "p",
    "h",
];
const MEDIALS: &[&str] = &[
    "a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wae", "oe", "yo", "u", "wo", "we",
    "wi", "yu", "eu", "ui", "i",
];
const FINALS: &[&str] = &[
    "", "k", "k", "ks", "n", "nj", "nh", "t", "l", "lg", "lm", "lb", "ls", "lt", "lp", "lh", "m",
    "p", "ps", "s", "ss", "ng", "j", "ch", "k", "t", "p", "h",
];

fn romanize_korean(input: &str) -> String {
    let mut out = String::new();
    for ch in input.chars() {
        let code = ch as u32;
        if (0xAC00..=0xD7A3).contains(&code) {
            let n = (code - 0xAC00) as usize;
            out.push_str(INITIALS[n / 588]);
            out.push_str(MEDIALS[(n % 588) / 28]);
            out.push_str(FINALS[n % 28]);
        } else {
            out.push(ch);
        }
    }
    out
}

/// Returns `Some(romanized)` only when the line actually contains non-Latin
/// script and romanizes to something different; Latin-only lines return `None`.
pub fn romanize_line(text: &str) -> Option<String> {
    if !has_non_latin(text) {
        return None;
    }
    let out = romanize_japanese(&romanize_korean(text));
    if out.trim() == text.trim() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::romanize_line;

    #[test]
    fn skips_latin_only_lines() {
        assert_eq!(romanize_line("Close your eyes, you'll be here soon"), None);
    }

    #[test]
    fn romanizes_japanese_and_korean() {
        assert_eq!(
            romanize_line("でもこのワードできない").as_deref(),
            Some("demokonowaadodekinai")
        );
        assert_eq!(romanize_line("사랑해").as_deref(), Some("saranghae"));
    }
}
