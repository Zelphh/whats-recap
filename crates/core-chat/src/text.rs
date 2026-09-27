//! Comparação de palavras para contagem: sem diferenciar maiúsculas e acentos, e tratando
//! alongamentos ("amooor", "kkkkkk") como a palavra base.

use core_stats::text::{strip_accents, without_urls, words};

/// Palavras de um texto, em minúsculas e sem acentos.
pub fn tokens(text: &str) -> Vec<String> {
    words(&without_urls(text))
        .map(|w| strip_accents(&w.to_lowercase()))
        .collect()
}

/// Colapsa qualquer sequência de letras iguais em uma só: "amooor" → "amor", "kkkk" → "k".
fn squash(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev = None;
    for c in s.chars() {
        if Some(c) != prev {
            out.push(c);
        }
        prev = Some(c);
    }
    out
}

/// Tem uma letra repetida 3+ vezes seguidas (sinal de alongamento, não de grafia normal).
fn is_elongated(s: &str) -> bool {
    let (mut prev, mut run) = (None, 0);
    for c in s.chars() {
        run = if Some(c) == prev { run + 1 } else { 1 };
        if run >= 3 {
            return true;
        }
        prev = Some(c);
    }
    false
}

fn same_word(token: &str, query: &str) -> bool {
    token == query || ((is_elongated(token) || is_elongated(query)) && squash(token) == squash(query))
}

/// Palavra ou expressão curta ("te amo") a procurar.
pub struct WordQuery {
    parts: Vec<String>,
}

impl WordQuery {
    pub fn new(query: &str) -> Option<Self> {
        let parts = tokens(query);
        (!parts.is_empty()).then_some(Self { parts })
    }

    /// Ocorrências (sem sobreposição) numa lista de tokens.
    pub fn count_in(&self, toks: &[String]) -> u64 {
        let k = self.parts.len();
        let (mut i, mut n) = (0, 0);
        while i + k <= toks.len() {
            if toks[i..i + k].iter().zip(&self.parts).all(|(t, q)| same_word(t, q)) {
                n += 1;
                i += k;
            } else {
                i += 1;
            }
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count(q: &str, text: &str) -> u64 {
        WordQuery::new(q).unwrap().count_in(&tokens(text))
    }

    #[test]
    fn matching() {
        assert_eq!(count("amor", "Amor, te amo amor! AMOOOOR"), 3);
        assert_eq!(count("amor", "amores amora"), 0);
        assert_eq!(count("você", "voce vc VOCÊ"), 2);
        // Risada: "kkk" alongado casa com qualquer tamanho.
        assert_eq!(count("kkk", "kkkkkkk kk k"), 3);
        assert_eq!(count("carro", "caro carro"), 1, "grafias normais não se confundem");
        assert_eq!(count("te amo", "eu te amo, te amooo muito"), 2);
        assert_eq!(count("te amo", "te adoro amo"), 0);
        assert_eq!(count("site", "olha https://site.com"), 0, "URLs ficam de fora");
        assert!(WordQuery::new("  ?! ").is_none());
    }
}
