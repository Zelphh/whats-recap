//! Chat sobre a conversa (feature 21: perguntas quantitativas, tipo 1).
//!
//! Fluxo: [`router::route`] classifica a pergunta; [`intent::interpret`] escolhe a ferramenta e
//! os parâmetros nos formatos óbvios; [`tools::Conversation::execute`] faz a contagem no SQLite;
//! [`answer::compose`] redige a resposta com os números do resultado.
//!
//! Onde a heurística não resolve, um [`Llm`] (se houver) classifica a pergunta e escolhe a
//! ferramenta, com os prompts e schemas de [`llm`]. A contagem continua sendo feita aqui.

pub mod answer;
pub mod intent;
pub mod llm;
pub mod router;
mod text;
pub mod tools;

pub use router::Route;
pub use tools::{Conversation, Granularity, ListFilter, ToolCall, ToolError, ToolResult, me, set_me};

use chrono::NaiveDate;
use core_storage::{Db, MessageRow};
use serde::Serialize;
use serde_json::Value;

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
    /// O modelo de linguagem escolheu a ferramenta e os parâmetros.
    Llm,
}

/// Modelo de linguagem para o que a heurística não resolve. O app implementa sobre o `core-llm`
/// (o servidor só sobe na primeira chamada); os testes, com respostas fixas.
pub trait Llm {
    /// Uma chamada com prompt de sistema, pergunta e saída restrita por `schema`. O erro vira texto
    /// para o usuário.
    fn complete(&self, system: &str, user: &str, schema: &Value, max_tokens: u32) -> Result<Value, String>;
}

/// Limites de tokens medidos no `whatsrecap-evals` (as saídas são JSON curto).
const ROUTER_MAX_TOKENS: u32 = 16;
const TOOL_MAX_TOKENS: u32 = 120;

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
        let name = authors.get(i).map(|a| intent::first_name(a)).unwrap_or_default();
        if name.is_empty() { "ela" } else { name }.to_string()
    };
    EXAMPLE_QUESTIONS
        .iter()
        .map(|q| q.replace("{a}", &first(0)).replace("{b}", &first(1)))
        .collect()
}

/// Sem LLM, ou quando o LLM falhou: a pergunta não foi entendida pelas regras fixas.
fn not_understood(route: Route, authors: &[String], llm_error: Option<&str>) -> ChatAnswer {
    let intro = if route == Route::Quantitative {
        "Entendi que é uma pergunta de contagem, mas não consegui identificar o que contar."
    } else {
        "Ainda não sei responder essa pergunta."
    };
    let why = match llm_error {
        Some(e) => format!(" O modelo de linguagem não respondeu ({e}), então só as regras fixas foram usadas."),
        None => " Sem o modelo de linguagem, as perguntas são interpretadas por regras fixas.".to_string(),
    };
    ChatAnswer::text_only(
        route,
        format!("{intro}{why} Experimente, por exemplo: “{}”", examples(authors)[0]),
    )
}

/// O LLM entendeu a pergunta, mas nenhuma ferramenta calcula o que ela pede.
fn no_tool(authors: &[String]) -> ChatAnswer {
    ChatAnswer::text_only(
        Route::Quantitative,
        format!(
            "Essa pergunta não dá para responder com as contagens disponíveis: palavras, horários, períodos, \
tempo de resposta, primeira vez que algo foi dito e listas de mensagens por pessoa, data e horário. Experimente, por exemplo: “{}”",
            examples(authors)[0]
        ),
    )
}

fn global_answer(route: Route) -> ChatAnswer {
    ChatAnswer::text_only(
        route,
        "Perguntas sobre a conversa como um todo (resumos, como a relação evoluiu) ainda não são \
suportadas: elas exigiriam ler a conversa inteira. Tente perguntas de contagem, como “Quem manda mais \
mensagem de madrugada?”."
            .into(),
    )
}

fn retrieval_answer(db: &Db, conv: &Conversation, question: &str) -> Result<ChatAnswer, ToolError> {
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
        ..ChatAnswer::text_only(Route::Retrieval, text)
    })
}

/// Executa a ferramenta e redige a resposta pelo template.
fn calculate(
    db: &Db,
    conv: &Conversation,
    call: ToolCall,
    intent: &intent::Intent,
    by: Interpreter,
) -> Result<ChatAnswer, ToolError> {
    let result = match conv.execute(&call) {
        Ok(r) => r,
        // Erros de parâmetro (autor desconhecido, datas invertidas) viram resposta, não falha.
        Err(
            e @ (ToolError::UnknownAuthor { .. }
            | ToolError::EmptyWord
            | ToolError::InvertedRange { .. }
            | ToolError::InvalidHour(_)),
        ) => {
            return Ok(ChatAnswer {
                tool: Some(call),
                interpreted_by: Some(by),
                ..ChatAnswer::text_only(Route::Quantitative, format!("Não consegui calcular: {e}."))
            });
        }
        Err(e) => return Err(e),
    };
    let (text, cited) = answer::compose(&call, &result, intent, &conv.authors);
    let mut citations = Vec::with_capacity(cited.len());
    for id in cited {
        citations.extend(db.messages_from(id, 1)?);
    }
    Ok(ChatAnswer {
        route: Route::Quantitative,
        text,
        tool: Some(call),
        result: Some(result),
        interpreted_by: Some(by),
        citations,
    })
}

/// Classifica a pergunta pelo LLM (só quando o roteador por regras não decidiu).
fn llm_route(llm: &dyn Llm, question: &str) -> Result<Route, String> {
    let out = llm.complete(
        &llm::router_prompt(),
        question,
        &llm::router_schema(),
        ROUTER_MAX_TOKENS,
    )?;
    serde_json::from_value(out["type"].clone()).map_err(|e| format!("classificação inválida: {e}"))
}

/// Escolhe a ferramenta pelo LLM. `None`: nenhuma ferramenta responde a pergunta.
fn llm_tool(llm: &dyn Llm, db: &Db, conv: &Conversation, question: &str) -> Result<Option<ToolCall>, String> {
    let authors = &conv.authors;
    let summary = db.summary().map_err(|e| e.to_string())?;
    let date = |ts: i64| chrono::DateTime::from_timestamp(ts, 0).unwrap_or_default().date_naive();
    let (first, last) = (date(summary.first_ts), date(summary.last_ts));
    let today = chrono::Local::now().date_naive();
    let iso = |d: NaiveDate| d.format("%Y-%m-%d").to_string();
    let system = llm::tool_selection_prompt(authors, conv.me.as_deref(), &iso(first), &iso(last), &iso(today));
    let out = llm.complete(&system, question, &llm::tool_schema(authors), TOOL_MAX_TOKENS)?;
    if out["tool"] == llm::NO_TOOL {
        return Ok(None);
    }
    let call: ToolCall = serde_json::from_value(out).map_err(|e| format!("ferramenta inválida: {e}"))?;
    // "Desde o começo até hoje" é o mesmo que não filtrar, e polui o "como calculei".
    Ok(Some(call.without_redundant_range(first, last)))
}

/// Responde a uma pergunta. As regras fixas vêm primeiro (instantâneas e exatas nos formatos que
/// conhecem); o `llm`, se houver, só entra quando elas não entendem a pergunta.
pub fn ask(db: &Db, question: &str, llm: Option<&dyn Llm>) -> Result<ChatAnswer, ToolError> {
    let conv = Conversation::open(db)?;
    let route = router::route(question);
    if matches!(route, Route::Quantitative | Route::Undecided)
        && let Some((call, intent)) = intent::interpret_as(question, &conv.authors, conv.me.as_deref())
    {
        return calculate(db, &conv, call, &intent, Interpreter::Heuristic);
    }
    let route = match (route, llm) {
        (Route::Undecided, Some(llm)) => match llm_route(llm, question) {
            Ok(r) => r,
            Err(e) => return Ok(not_understood(route, &conv.authors, Some(&e))),
        },
        (r, _) => r,
    };
    match (route, llm) {
        (Route::Global, _) => Ok(global_answer(route)),
        (Route::Retrieval, _) => retrieval_answer(db, &conv, question),
        (_, None) => Ok(not_understood(route, &conv.authors, None)),
        (_, Some(llm)) => match llm_tool(llm, db, &conv, question) {
            Ok(Some(call)) => {
                let intent = intent::intent_for(&call, question);
                calculate(db, &conv, call, &intent, Interpreter::Llm)
            }
            Ok(None) => Ok(no_tool(&conv.authors)),
            Err(e) => Ok(not_understood(route, &conv.authors, Some(&e))),
        },
    }
}

#[cfg(test)]
mod tests;
