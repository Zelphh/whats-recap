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
        r"\b(quant[ao]s?|quem (mais|menos|manda|envia|escreve|fala|responde|demora|digita|conversa|puxa)|primeira vez|qual (o |a )?(mes|ano|dia|semana|horario|hora)|que horas?|horario|com que frequencia|media|tempo de resposta|mais rapido|mais devagar|demora\w*|(quais|que|mostr\w*|list\w*|exib\w*) (foram |sao |eram )?(todas )?(as )?(mensagens|msgs?))\b",
    )
    .unwrap()
});

static RETRIEVAL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(o que|sobre o que|quando (a gente|nos|eu|ele|ela|voce|vc)|onde|lembra|combin\w+|falou sobre|disse sobre|conversamos sobre|falamos sobre|por que|porque)\b",
    )
    .unwrap()
});

// "quanto custou o conserto?", "quantos anos a mãe dela fez?": valores ditos na conversa, não
// contagens de mensagens (o "quant" casaria com `QUANT_RE`).
static CONTENT_AMOUNT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(quant[oa]s? (custou|custa|custaram|deu|ficou|ficaram|pag\w*|cobr\w*|gast\w*)|quant[oa] (foi|era|sai|saiu)|quantos anos)\b",
    )
    .unwrap()
});

// Continuação da pergunta anterior: "e depois?", "mas por quê?", "quando foi isso?", "e ela
// respondeu o quê?". Começa com "e"/"mas"/"então"/"aí" ou aponta para o que já foi dito ("isso").
// "depois" no meio da frase não basta: "de quanto ficou o aluguel depois da negociação?".
static FOLLOWUP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(e|mas|entao|ai|tipo)\b|\b(isso|disso|nisso|daquilo|naquilo)\b").unwrap());
static AFTER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(depois|dai|em seguida|e ai|e entao|o que aconteceu|respond\w*)\b").unwrap());
static BEFORE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(antes|por que|porque|como comecou)\b").unwrap());

/// Palavras de uma continuação, no máximo ("e o que ela respondeu depois disso?" tem 8).
const FOLLOWUP_WORDS: usize = 8;

/// A pergunta (já em [`fold`]) parece continuar a anterior: curta e com "e…", "isso", "depois"…, ou
/// de no máximo duas palavras ("por quê?", "quando?").
pub fn is_followup(q: &str) -> bool {
    let words = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .count();
    words > 0 && (words <= 2 || (words <= FOLLOWUP_WORDS && FOLLOWUP_RE.is_match(q.trim_start())))
}

/// Para que lado da resposta anterior a continuação olha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// "e depois?", "o que ela respondeu?"
    After,
    /// "e antes disso?", "por quê?"
    Before,
}

pub fn followup_direction(q: &str) -> Option<Direction> {
    if AFTER_RE.is_match(q) {
        Some(Direction::After)
    } else if BEFORE_RE.is_match(q) {
        Some(Direction::Before)
    } else {
        None
    }
}

/// Minúsculas e sem acentos: a forma usada por todas as heurísticas.
pub fn fold(s: &str) -> String {
    core_stats::text::strip_accents(&s.to_lowercase())
}

pub fn route(question: &str) -> Route {
    let q = fold(question);
    if GLOBAL_RE.is_match(&q) {
        Route::Global
    } else if CONTENT_AMOUNT_RE.is_match(&q) {
        Route::Retrieval
    } else if QUANT_RE.is_match(&q) || crate::intent::is_timed_what(&q) {
        // "o que ela mandou das 7h às 8h?" é um filtro (lista de mensagens), não pergunta de conteúdo.
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
            "quais foram as mensagens dela das 7h às 8h?",
            "me mostra as mensagens do Bruno de madrugada",
            "o que a gente mandou entre 22h e 2h?",
            "me mostra o que ela escreveu de madrugada",
            "mostra o que a Ana mandou no natal do ano passado",
            "o que ele escreveu em 15/03/2024?",
        ] {
            assert_eq!(route(q), Route::Quantitative, "{q}");
        }
        for q in [
            "quando a gente combinou a viagem pra Floripa?",
            "o que ela disse sobre o emprego novo?",
            "onde fomos no aniversário?",
            "quanto custou o conserto do carro?",
            "quantos anos a mãe da Ana fez?",
            "de quanto ficou o aluguel depois da negociação?",
            "e quanto foi a consulta?",
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

    #[test]
    fn followups() {
        for q in [
            "e depois?",
            "por quê?",
            "quando?",
            "e ela respondeu o quê?",
            "mas por que ela disse isso?",
            "quando foi isso?",
            "e antes disso, o que tinha acontecido?",
        ] {
            assert!(is_followup(&fold(q)), "{q}");
        }
        for q in [
            "o que a Ana disse sobre o emprego novo?",
            "quantas vezes ele falou amor?",
            "qual o nome do restaurante japonês que o Rafa indicou?",
            "e se a gente fosse lembrar de tudo o que aconteceu naquele ano todo?",
            "de quanto ficou o aluguel depois da negociação?",
            "o que ela falou antes da viagem?",
        ] {
            assert!(!is_followup(&fold(q)), "{q}");
        }
        assert_eq!(followup_direction(&fold("e depois?")), Some(Direction::After));
        assert_eq!(
            followup_direction(&fold("e ela respondeu o quê?")),
            Some(Direction::After)
        );
        assert_eq!(followup_direction(&fold("por quê?")), Some(Direction::Before));
        assert_eq!(followup_direction(&fold("quando foi isso?")), None);
    }
}
