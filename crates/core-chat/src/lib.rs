//! Chat sobre a conversa (feature 21: perguntas quantitativas, tipo 1).
//!
//! Fluxo: [`router::route`] classifica a pergunta; [`intent::interpret`] escolhe a ferramenta e
//! os parâmetros nos formatos óbvios; [`tools::Conversation::execute`] faz a contagem no SQLite;
//! [`answer::compose`] redige a resposta com os números do resultado. O LLM (fase 5) entra onde
//! a heurística não resolve, usando os schemas de [`llm`].

pub mod answer;
pub mod intent;
pub mod llm;
pub mod router;
mod text;
pub mod tools;

pub use router::Route;
pub use tools::{Conversation, Granularity, ToolCall, ToolError, ToolResult};

use core_storage::{Db, MessageRow};
use serde::Serialize;

/// Quantos trechos sugerir quando a pergunta é sobre o conteúdo.
const RETRIEVAL_HINTS: i64 = 5;

/// Palavras da estrutura da pergunta ("o que a gente falou sobre…"), que não ajudam a achar o assunto.
const QUESTION_WORDS: &[&str] = &[
    "falou",
    "falei",
    "falamos",
    "falaram",
    "falar",
    "disse",
    "dissemos",
    "disseram",
    "dizer",
    "sobre",
    "gente",
    "quando",
    "onde",
    "combinou",
    "combinamos",
    "combinaram",
    "conversamos",
    "conversou",
    "conversa",
    "lembra",
    "lembro",
    "lembrar",
    "coisa",
    "aquela",
    "aquele",
    "falando",
    "mandou",
    "mandei",
    "contou",
    "contei",
];

pub const EXAMPLE_QUESTIONS: &[&str] = &[
    "Quantas vezes {b} falou \"amor\"?",
    "Quem manda mais mensagem de madrugada?",
    "Qual mês teve mais mensagens?",
    "Quem responde mais rápido?",
    "Quando foi a primeira vez que {a} disse \"te amo\"?",
    "Que horas a gente mais conversa?",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpreter {
    /// Regras fixas, sem LLM.
    Heuristic,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatAnswer {
    pub route: Route,
    pub text: String,
    /// Ferramenta usada e parâmetros, para mostrar "como calculei".
    pub tool: Option<ToolCall>,
    pub result: Option<ToolResult>,
    pub interpreted_by: Option<Interpreter>,
    /// Mensagens citadas (clicáveis no visualizador).
    pub citations: Vec<MessageRow>,
}

impl ChatAnswer {
    fn text_only(route: Route, text: String) -> Self {
        Self {
            route,
            text,
            tool: None,
            result: None,
            interpreted_by: None,
            citations: Vec::new(),
        }
    }
}

/// Exemplos de perguntas com os nomes da conversa.
pub fn examples(authors: &[String]) -> Vec<String> {
    let first = |i: usize| {
        authors
            .get(i)
            .and_then(|a| a.split_whitespace().next())
            .unwrap_or("ela")
            .to_string()
    };
    EXAMPLE_QUESTIONS
        .iter()
        .map(|q| q.replace("{a}", &first(0)).replace("{b}", &first(1)))
        .collect()
}

fn not_understood(route: Route, authors: &[String]) -> ChatAnswer {
    let intro = if route == Route::Quantitative {
        "Entendi que é uma pergunta de contagem, mas não consegui identificar o que contar."
    } else {
        "Ainda não sei responder essa pergunta."
    };
    ChatAnswer::text_only(
        route,
        format!(
            "{intro} Por enquanto, as perguntas são interpretadas por regras fixas; perguntas livres vão \
funcionar quando o modelo de linguagem estiver configurado. Experimente, por exemplo: “{}”",
            examples(authors)[0]
        ),
    )
}

pub fn ask(db: &Db, question: &str) -> Result<ChatAnswer, ToolError> {
    let conv = Conversation::open(db)?;
    let route = router::route(question);
    match route {
        Route::Global => Ok(ChatAnswer::text_only(
            route,
            "Perguntas sobre a conversa como um todo (resumos, como a relação evoluiu) ainda não são \
suportadas: elas exigiriam ler a conversa inteira. Tente perguntas de contagem, como “Quem manda mais \
mensagem de madrugada?”."
                .into(),
        )),
        Route::Retrieval => {
            // Enquanto o RAG (fase 9) não existe, sugere mensagens com as palavras da pergunta.
            let mut stop = conv.settings.stopwords.clone();
            stop.extend(QUESTION_WORDS.iter().map(|w| w.to_string()));
            let terms: Vec<String> = core_stats::text::Normalizer::new(&stop, false)
                .content_words(question)
                .filter(|w| w.chars().count() >= 3)
                .collect();
            let citations = db.search_any(&terms, RETRIEVAL_HINTS)?;
            let text = if citations.is_empty() {
                "Perguntas sobre o conteúdo da conversa ainda não são respondidas automaticamente, e não achei \
mensagens com as palavras da pergunta. Tente a busca na tela Conversa."
                    .to_string()
            } else {
                "Perguntas sobre o conteúdo da conversa ainda não são respondidas automaticamente. Enquanto isso, \
estas mensagens têm palavras da sua pergunta:"
                    .to_string()
            };
            Ok(ChatAnswer {
                citations,
                ..ChatAnswer::text_only(route, text)
            })
        }
        Route::Quantitative | Route::Undecided => {
            let Some((call, intent)) = intent::interpret(question, &conv.authors) else {
                return Ok(not_understood(route, &conv.authors));
            };
            let result = match conv.execute(&call) {
                Ok(r) => r,
                // Erros de parâmetro (autor desconhecido, datas invertidas) viram resposta, não falha.
                Err(e @ (ToolError::UnknownAuthor { .. } | ToolError::EmptyWord | ToolError::InvertedRange { .. })) => {
                    return Ok(ChatAnswer {
                        tool: Some(call),
                        interpreted_by: Some(Interpreter::Heuristic),
                        ..ChatAnswer::text_only(Route::Quantitative, format!("Não consegui calcular: {e}."))
                    });
                }
                Err(e) => return Err(e),
            };
            let (text, cited) = answer::compose(&call, &result, &intent, &conv.authors);
            let mut citations = Vec::with_capacity(cited.len());
            for id in cited {
                citations.extend(db.messages_from(id, 1)?);
            }
            Ok(ChatAnswer {
                route: Route::Quantitative,
                text,
                tool: Some(call),
                result: Some(result),
                interpreted_by: Some(Interpreter::Heuristic),
                citations,
            })
        }
    }
}

#[cfg(test)]
mod tests;
