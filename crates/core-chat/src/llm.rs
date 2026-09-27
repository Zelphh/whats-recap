//! Contrato com o LLM (fase 5): schemas JSON para saída estruturada no `llama-server`
//! (`json_schema`) e os prompts. O cliente em si ainda não existe; este módulo define o que
//! será pedido ao modelo e garante que a resposta desserializa para os tipos do chat.

use serde_json::{Value, json};

/// Schema da classificação da pergunta: `{"type": "quantitative" | "retrieval" | "global"}`.
pub fn router_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "type": { "enum": ["quantitative", "retrieval", "global"] } },
        "required": ["type"],
        "additionalProperties": false
    })
}

/// Schema de [`crate::ToolCall`]. Os nomes das pessoas entram como `enum`, então o modelo
/// não consegue inventar um autor.
pub fn tool_schema(authors: &[String]) -> Value {
    let date = json!({ "type": "string", "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}$" });
    let author = json!({ "enum": authors });
    let word = json!({ "type": "string", "minLength": 1, "maxLength": 60 });
    let tool = |name: &str, props: Value, required: &[&str]| {
        json!({
            "type": "object",
            "properties": {
                "tool": { "const": name },
                "args": { "type": "object", "properties": props, "required": required, "additionalProperties": false }
            },
            "required": ["tool", "args"],
            "additionalProperties": false
        })
    };
    json!({
        "oneOf": [
            tool("count_word", json!({ "word": word, "author": author, "from": date, "to": date }), &["word"]),
            tool("messages_by_hour", json!({ "author": author, "from": date, "to": date }), &[]),
            tool(
                "messages_by_period",
                json!({
                    "granularity": { "enum": ["day", "week", "month", "year", "weekday"] },
                    "author": author, "from": date, "to": date
                }),
                &["granularity"],
            ),
            tool("first_occurrence", json!({ "word": word, "author": author }), &["word"]),
            tool("response_time", json!({ "author": author, "from": date, "to": date }), &[]),
        ]
    })
}

/// Prompt de sistema para escolher a ferramenta.
pub fn tool_selection_prompt(authors: &[String], first_date: &str, last_date: &str) -> String {
    format!(
        "Você escolhe UMA ferramenta para responder a uma pergunta sobre uma conversa de WhatsApp entre {} \
(de {first_date} a {last_date}). Você não lê a conversa: só escolhe a ferramenta e preenche os parâmetros.\n\n\
Ferramentas:\n\
- count_word(word, author?, from?, to?): quantas vezes uma palavra ou expressão curta aparece.\n\
- messages_by_hour(author?, from?, to?): mensagens por hora do dia (0–23), por pessoa. Use para \"de madrugada\", \"que horas\", \"quem manda mais\".\n\
- messages_by_period(granularity, author?, from?, to?): mensagens por dia, semana, mês, ano ou dia da semana.\n\
- first_occurrence(word, author?): a primeira mensagem com a palavra ou expressão.\n\
- response_time(author?, from?, to?): tempo de resposta (média e mediana) por pessoa.\n\n\
Regras: datas no formato AAAA-MM-DD, inclusivas; omita parâmetros não mencionados; \"author\" só quando a \
pergunta for sobre uma pessoa específica (em \"quem...?\", compare as duas e omita).",
        authors.join(" e ")
    )
}

/// Prompt para redigir a resposta final a partir do resultado da ferramenta.
pub fn answer_prompt(question: &str, tool_output: &Value) -> String {
    format!(
        "Pergunta: {question}\n\nResultado calculado (JSON):\n{}\n\n\
Responda em português, em uma ou duas frases, usando SOMENTE os números do resultado. \
Não invente números nem arredonde de forma que mude o sentido.",
        serde_json::to_string_pretty(tool_output).unwrap_or_default()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolCall;

    #[test]
    fn schema_covers_every_tool_and_parses_model_output() {
        let s = tool_schema(&["Ana".into(), "Bruno".into()]);
        let names: Vec<&str> = s["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["properties"]["tool"]["const"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "count_word",
                "messages_by_hour",
                "messages_by_period",
                "first_occurrence",
                "response_time"
            ]
        );
        assert_eq!(
            s["oneOf"][0]["properties"]["args"]["properties"]["author"]["enum"][1],
            "Bruno"
        );

        // Saídas no formato do schema desserializam para ToolCall.
        let out = r#"{"tool":"count_word","args":{"word":"amor","author":"Bruno","from":"2024-01-01"}}"#;
        let call: ToolCall = serde_json::from_str(out).unwrap();
        assert_eq!(call.name(), "count_word");
        assert_eq!(call.author(), Some("Bruno"));
        let out = r#"{"tool":"messages_by_period","args":{"granularity":"weekday"}}"#;
        assert!(serde_json::from_str::<ToolCall>(out).is_ok());
        assert!(serde_json::from_str::<ToolCall>(r#"{"tool":"drop_table","args":{}}"#).is_err());
    }
}
