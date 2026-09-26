//! Normalização de texto compartilhada por "palavras mais usadas" e "palavras exclusivas".

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;
use unicode_segmentation::UnicodeSegmentation;

static URL_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\b(?:https?://|www\.)\S+").unwrap());

/// Remove URLs antes da segmentação (senão viram várias "palavras").
pub fn without_urls(text: &str) -> std::borrow::Cow<'_, str> {
    URL_RE.replace_all(text, " ")
}

/// Palavras de uma mensagem, pela segmentação de palavras Unicode.
pub fn words(text: &str) -> impl Iterator<Item = &str> {
    text.unicode_words()
}

/// Colapsa sequências de mais de 2 letras iguais: `siiiim` → `siim`, `kkkkkk` → `kk`.
pub fn collapse_repeats(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let (mut prev, mut run) = (None, 0);
    for c in s.chars() {
        if Some(c) == prev {
            run += 1;
        } else {
            prev = Some(c);
            run = 1;
        }
        if run <= 2 {
            out.push(c);
        }
    }
    out
}

pub fn strip_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            _ => c,
        })
        .collect()
}

/// Normaliza uma palavra para contagem. Retorna `None` para números e palavras de 1 letra.
pub fn normalize(word: &str, accents: bool) -> Option<String> {
    if word.chars().any(|c| c.is_numeric()) {
        return None;
    }
    let mut w = collapse_repeats(&word.to_lowercase());
    if accents {
        w = strip_accents(&w);
    }
    (w.chars().count() >= 2).then_some(w)
}

pub struct Normalizer {
    stopwords: HashSet<String>,
    strip_accents: bool,
}

impl Normalizer {
    pub fn new(stopwords: &[String], strip_accents: bool) -> Self {
        let stopwords = stopwords
            .iter()
            .filter_map(|s| normalize(s.trim(), strip_accents))
            .collect();
        Self {
            stopwords,
            strip_accents,
        }
    }

    /// Palavras de conteúdo da mensagem: normalizadas, sem stopwords, URLs ou números.
    pub fn content_words<'a>(&'a self, text: &'a str) -> impl Iterator<Item = String> + 'a {
        words(text)
            .filter_map(|w| normalize(w, self.strip_accents))
            .filter(|w| !self.stopwords.contains(w))
    }
}

/// Stopwords em português (editáveis nas configurações).
pub const DEFAULT_STOPWORDS: &[&str] = &[
    "a", "à", "ao", "aos", "aquela", "aquelas", "aquele", "aqueles", "aquilo", "as", "às", "até", "com", "como", "da",
    "das", "de", "dela", "delas", "dele", "deles", "depois", "do", "dos", "e", "é", "ela", "elas", "ele", "eles", "em",
    "entre", "era", "eram", "essa", "essas", "esse", "esses", "esta", "está", "estão", "estas", "estava", "estavam",
    "este", "estes", "estou", "eu", "foi", "for", "foram", "há", "isso", "isto", "já", "lhe", "lhes", "mais", "mas",
    "me", "mesmo", "meu", "meus", "minha", "minhas", "muito", "na", "nas", "nem", "no", "nos", "nós", "nossa",
    "nossas", "nosso", "nossos", "num", "numa", "não", "o", "os", "ou", "para", "pela", "pelas", "pelo", "pelos",
    "por", "qual", "quando", "que", "quem", "se", "sem", "ser", "seu", "seus", "só", "sua", "suas", "também", "te",
    "tem", "têm", "tinha", "tu", "tua", "tuas", "um", "uma", "umas", "uns", "você", "vocês", "vos", "vai", "vou",
    "ter", "tá", "ta", "tô", "to", "pra", "pro", "pras", "pros", "vc", "vcs", "q", "pq", "porque", "tb", "tbm", "né",
    "ne", "aí", "ai", "lá", "la", "aqui", "então", "entao", "agora", "ainda", "tão", "tipo", "fazer", "faz", "vez",
    "bem", "onde", "cê", "ce", "num", "dá", "da", "deu", "ia", "sei", "acho", "assim",
];
