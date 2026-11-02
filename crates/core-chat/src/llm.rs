//! Contrato com o LLM: schemas JSON para saída estruturada no `llama-server` (`json_schema`) e
//! os prompts. As versões daqui são as que venceram no `whatsrecap-evals` (Qwen3.5 9B, RX 7600):
//! roteador com exemplos (98% de acerto) e seleção de ferramenta com datas relativas já calculadas,
//! regras de autor em comparações, "nenhuma ferramenta" e exemplos (93% de acerto exato em B com
//! 50 perguntas; era 73% antes dessas regras).

use chrono::{Datelike, Months, NaiveDate};
use serde_json::{Value, json};

/// Schema da classificação da pergunta: `{"type": "quantitative" | "retrieval" | "global"}`.
/// Desserializa em [`crate::Route`] pelo campo `type`.
pub fn router_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "type": { "enum": ["quantitative", "retrieval", "global"] } },
        "required": ["type"],
        "additionalProperties": false
    })
}

/// Prompt de sistema do roteador; a pergunta vai como mensagem do usuário.
pub fn router_prompt() -> String {
    "Você classifica perguntas sobre uma conversa de WhatsApp entre duas pessoas. Responda só com o tipo:\n\
- quantitative: contagem, frequência, comparação ou estatística que dá para calcular (quantas vezes, quem manda \
mais, que horas, qual mês, tempo de resposta, primeira vez que alguém disse algo).\n\
- retrieval: pergunta pontual sobre o conteúdo de algum momento da conversa (o que alguém disse, quando combinaram \
algo, onde foram).\n\
- global: pede uma análise ou resumo da conversa inteira ou da relação (como evoluiu, resumo, melhor fase).\n\n\
Exemplos:\n\
\"quantas vezes ela me chamou de chato?\" → quantitative\n\
\"quem puxa mais assunto de manhã?\" → quantitative\n\
\"o que a gente decidiu sobre o aluguel?\" → retrieval\n\
\"qual era o nome do restaurante que ele indicou?\" → retrieval\n\
\"a gente brigava mais no começo?\" → global\n\
\"me conta como foi nosso 2023\" → global"
        .to_string()
}

/// `{"tool": "none", "args": {}}`: a pergunta não cabe em nenhuma ferramenta. Não é uma
/// [`crate::ToolCall`]; quem chama trata antes de desserializar.
pub const NO_TOOL: &str = "none";

/// Schema de [`crate::ToolCall`] (mais a saída [`NO_TOOL`]). Os nomes das pessoas entram como
/// `enum`, então o modelo não consegue inventar um autor.
///
/// A ordem das chaves importa: o llama.cpp gera os campos na ordem das `properties`, então
/// `"tool"` vem antes de `"args"` para o modelo escolher a ferramenta antes de preencher os
/// parâmetros. Em ordem alfabética (`"args"` primeiro), o acerto da ferramenta caía de 39/40 para
/// 14/40 no Qwen3.5 9B. Depende de `serde_json/preserve_order` (ver o teste
/// `tool_schema_puts_tool_before_args`).
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
            tool(NO_TOOL, json!({}), &[]),
        ]
    })
}

/// Prompt de sistema para escolher a ferramenta. Datas em `AAAA-MM-DD`; `today` resolve datas
/// relativas ("ano passado"), que já vão calculadas no prompt: o modelo errava a conta (trocava
/// "ano passado" pelos últimos 12 meses).
pub fn tool_selection_prompt(authors: &[String], first_date: &str, last_date: &str, today: &str) -> String {
    format!(
        "Você escolhe UMA ferramenta para responder a uma pergunta sobre uma conversa de WhatsApp entre {} (de {first_date} a {last_date}). Você não lê a conversa: só escolhe a ferramenta e preenche os parâmetros.

Ferramentas:
- count_word(word, author?, from?, to?): quantas vezes uma palavra ou expressão curta aparece, por pessoa.
- messages_by_hour(author?, from?, to?): mensagens por hora do dia (0–23), por pessoa. Use para partes do dia (madrugada, manhã, tarde, noite, \"de dia\"), \"que horas\" e \"quem manda mais mensagem\".
- messages_by_period(granularity, author?, from?, to?): mensagens por período. granularity: \"weekday\" para dia da semana, segunda a domingo, \"semana ou fim de semana\"; \"day\" para um dia do calendário; \"week\" para semanas do calendário (\"qual semana\", \"por semana\"); \"month\" para meses, épocas e estações do ano; \"year\" para anos.
- first_occurrence(word, author?): a primeira mensagem com a palavra ou expressão.
- response_time(author?, from?, to?): tempo de resposta (média e mediana) por pessoa.

Pessoas: apelidos e pronomes (\"ela\", \"ele\", \"a Aninha\") se referem a uma das duas pelo nome. Quem pergunta é uma das duas, mas você não sabe qual: \"eu\", \"me\" e \"mim\" nunca viram \"author\". Preencha \"author\" só quando a pergunta for sobre UMA pessoa. Em comparações (\"quem...?\", \"X ou Y?\", \"eu ou ela?\", \"quem fala mais...\"), omita \"author\": as ferramentas já devolvem os números das duas.

Datas: AAAA-MM-DD, inclusivas; omita \"from\" e \"to\" quando a pergunta não fala de período. Hoje é {today}. {}Outros períodos (meses, estações) também viram \"from\" e \"to\"; o verão vai de 21 de dezembro a 20 de março.

Em count_word e first_occurrence, \"word\" é só a palavra ou expressão procurada, sem aspas, como a pessoa escreveria. Se nenhuma ferramenta responde, use {{\"tool\":\"{NO_TOOL}\",\"args\":{{}}}}: brigas, sentimentos, assuntos, opiniões, contagem de mídia (figurinhas, áudios, fotos, vídeos) e categorias de palavras (palavrões, emojis em geral). Nunca troque uma categoria por uma palavra ou emoji de exemplo.

Exemplos (sem as datas):
\"quem fala mais 'bom dia', eu ou ele?\" → {{\"tool\":\"count_word\",\"args\":{{\"word\":\"bom dia\"}}}}
\"quantas vezes eu falei 'oi'?\" → {{\"tool\":\"count_word\",\"args\":{{\"word\":\"oi\"}}}}
\"quem demora mais pra responder?\" → {{\"tool\":\"response_time\",\"args\":{{}}}}
\"falamos mais no sábado ou na segunda?\" → {{\"tool\":\"messages_by_period\",\"args\":{{\"granularity\":\"weekday\"}}}}",
        authors.join(" e "),
        relative_dates(today)
    )
}

/// "ano passado = 2025-01-01 a 2025-12-31; …" a partir de `today` (vazio se a data for inválida).
fn relative_dates(today: &str) -> String {
    let Ok(t) = NaiveDate::parse_from_str(today, "%Y-%m-%d") else {
        return String::new();
    };
    let d = |y: i32, m: u32, day: u32| NaiveDate::from_ymd_opt(y, m, day).unwrap_or(t);
    let month_start = d(t.year(), t.month(), 1);
    let last_month_end = month_start.pred_opt().unwrap_or(t);
    let year_ago = t.checked_sub_months(Months::new(12)).unwrap_or(t);
    let iso = |x: NaiveDate| x.format("%Y-%m-%d").to_string();
    format!(
        "Use estes intervalos: \"este ano\" = {} a {}; \"ano passado\" = {} a {}; \"este mês\" = {} a {}; \"mês passado\" = {} a {}; \"último ano\" ou \"últimos 12 meses\" = {} a {}. ",
        iso(d(t.year(), 1, 1)),
        iso(t),
        iso(d(t.year() - 1, 1, 1)),
        iso(d(t.year() - 1, 12, 31)),
        iso(month_start),
        iso(t),
        iso(d(last_month_end.year(), last_month_end.month(), 1)),
        iso(last_month_end),
        iso(year_ago),
        iso(t),
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
                "response_time",
                "none"
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

    #[test]
    fn tool_schema_puts_tool_before_args() {
        // O texto enviado ao llama-server, não o Value: é a ordem serializada que vira gramática.
        let s = serde_json::to_string(&tool_schema(&["Ana".into(), "Bruno".into()])).unwrap();
        let branches = s.matches(r#""properties":{"tool":"#).count();
        assert_eq!(branches, 6, "\"tool\" tem de vir antes de \"args\" em cada ramo: {s}");
    }

    #[test]
    fn relative_dates_are_computed() {
        let p = tool_selection_prompt(
            &["Ana".into(), "Bruno".into()],
            "2021-03-14",
            "2026-03-31",
            "2026-03-31",
        );
        assert!(p.contains("\"ano passado\" = 2025-01-01 a 2025-12-31"), "{p}");
        assert!(p.contains("\"mês passado\" = 2026-02-01 a 2026-02-28"), "{p}");
        assert!(p.contains("\"últimos 12 meses\" = 2025-03-31 a 2026-03-31"), "{p}");
        assert!(!tool_selection_prompt(&[], "", "", "ontem").contains("ano passado\" ="));
    }

    #[test]
    fn redundant_range_is_dropped() {
        let d = |s: &str| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap();
        let (first, last) = (d("2021-03-14"), d("2026-02-27"));
        let call = |from: &str, to: &str| ToolCall::MessagesByHour {
            author: None,
            from: Some(d(from)),
            to: Some(d(to)),
        };
        let range = |c: ToolCall| c.without_redundant_range(first, last).range();

        // Período inteiro (ou além dele) equivale a não filtrar.
        assert_eq!(range(call("2021-03-14", "2026-02-27")), (None, None));
        assert_eq!(range(call("2020-01-01", "2030-12-31")), (None, None));
        // Só a ponta que não restringe some.
        assert_eq!(range(call("2021-03-14", "2024-12-31")), (None, Some(d("2024-12-31"))));
        assert_eq!(range(call("2024-01-01", "2026-02-27")), (Some(d("2024-01-01")), None));
        assert_eq!(
            range(call("2024-01-01", "2024-12-31")),
            (Some(d("2024-01-01")), Some(d("2024-12-31")))
        );
    }
}
