//! Roteador: classifica a pergunta antes de gastar inferência. Os casos óbvios são resolvidos
//! por palavras-chave; o resto fica `Undecided` e, com o LLM disponível, vai para ele.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Route {
    /// Tipo 1: contagens e agregações (ferramentas fechadas).
    Quantitative,
    /// Tipo 2: perguntas pontuais sobre o conteúdo (RAG, fase 9).
    Retrieval,
    /// Tipo 3: perguntas sobre a conversa inteira (fora do escopo).
    Global,
    /// A heurística não decidiu.
    Undecided,
}

static GLOBAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(resum\w*|em geral|de modo geral|como (evoluiu|mudou|esta|anda|foi|e) (?:(?:a|o) )?(?:(?:nossa|nosso|minha|meu) )?(relacao|relacionamento|conversa|namoro|amizade)|o que (voce )?acha d[aeo] (?:(?:nossa|nosso|minha|meu) )?(relacao|relacionamento|conversa|namoro)|qual (e |foi )?o clima|melhor fase|pior fase|analis[ae] (?:(?:a|o) )?(?:(?:nossa|nosso|minha|meu) )?(relacao|relacionamento|conversa|namoro))",
    )
    .unwrap()
});

static QUANT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(quant[ao]s?|quem (mais|menos|manda|envia|escreve|fala|responde|demora|digita|conversa|puxa)|primeira vez|qual (o |a )?(mes|ano|dia|semana|horario|hora)|que horas?|horario|com que frequencia|media|tempo de resposta|mais rapido|mais devagar|demora\w*)\b",
    )
    .unwrap()
});

static RETRIEVAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(o que|sobre o que|quando (a gente|nos|eu|ele|ela|voce|vc)|onde|lembra|combin\w+|falou sobre|disse sobre|conversamos sobre|falamos sobre|por que|porque)\b",
    )
    .unwrap()
});

/// Minúsculas e sem acentos: a forma usada por todas as heurísticas.
pub fn fold(s: &str) -> String {
    core_stats::text::strip_accents(&s.to_lowercase())
}

pub fn route(question: &str) -> Route {
    let q = fold(question);
    if GLOBAL_RE.is_match(&q) {
        Route::Global
    } else if QUANT_RE.is_match(&q) {
        Route::Quantitative
    } else if RETRIEVAL_RE.is_match(&q) {
        Route::Retrieval
    } else {
        Route::Undecided
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes() {
        for q in [
            "Quantas vezes o Bruno falou 'amor'?",
            "quem manda mais mensagem de madrugada?",
            "Qual mês teve mais mensagens?",
            "quando foi a primeira vez que ele disse te amo?",
            "Quem responde mais rápido?",
            "que horas a gente mais conversa?",
        ] {
            assert_eq!(route(q), Route::Quantitative, "{q}");
        }
        for q in [
            "quando a gente combinou a viagem pra Floripa?",
            "o que ela disse sobre o emprego novo?",
            "onde fomos no aniversário?",
        ] {
            assert_eq!(route(q), Route::Retrieval, "{q}");
        }
        for q in [
            "Como evoluiu a nossa relação?",
            "faz um resumo da conversa",
            "qual foi a melhor fase?",
        ] {
            assert_eq!(route(q), Route::Global, "{q}");
        }
        assert_eq!(route("oi"), Route::Undecided);
    }
}
