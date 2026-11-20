//! Chat tipo 2 (fase 9): perguntas sobre o conteúdo ("quando a gente combinou a viagem?").
//!
//! 1. **Indexação** (uma vez por conversa, em segundo plano): a conversa vira trechos de até
//!    [`WINDOW_MESSAGES`] mensagens, quebrados nas pausas longas e com sobreposição; cada trecho vira
//!    um vetor pelo embedder (e5-small, na CPU), gravado na tabela `chunks`. Retomável: trechos sem
//!    vetor são os que faltam.
//! 2. **Busca**: a pergunta vira vetor e os [`TOP_K`] trechos mais parecidos (cosseno, por força
//!    bruta: exato e, com ~20 mil trechos, poucos milissegundos) vão para o contexto. No evals, a
//!    busca só vetorial venceu a híbrida com FTS (MRR 0,84 × 0,70).
//! 3. **Resposta**: o LLM lê só esses trechos e responde citando os ids das mensagens; citações
//!    fora do contexto são descartadas e, sem nenhuma válida, a resposta é recusada.

use crate::llm;
use crate::{ChatAnswer, Interpreter, Llm, Route, ToolError};
use core_storage::{ChunkRow, Db, MessageRow};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};

/// Gera vetores para textos (o app usa o `llama-server` com o e5; os testes, um vetor fixo).
pub trait Embedder {
    /// Identificador do modelo (um índice feito com outro embedder é refeito).
    fn id(&self) -> &str;
    /// Um vetor normalizado por texto, na mesma ordem.
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
}

/// Muda quando a forma dos trechos muda: um índice de outra versão é refeito.
pub const INDEX_VERSION: u32 = 1;
const META_KEY: &str = "rag_index";

/// Pausa que separa dois assuntos: um trecho nunca atravessa uma pausa maior que esta.
const GAP_SECS: i64 = 60 * 60;
/// Tamanho do trecho. As janelas do evals tinham ~7 mensagens; com 12 e sobreposição de 2, uma
/// combinação dita em 3–4 mensagens quase sempre cabe inteira em algum trecho.
pub const WINDOW_MESSAGES: usize = 12;
const OVERLAP: usize = 2;
/// O e5 lê até 512 tokens: ~1.000 caracteres em português, com folga.
const WINDOW_CHARS: usize = 1000;
const MESSAGE_CHARS: usize = 300;
/// Trechos por chamada ao embedder.
const BATCH: usize = 32;
/// Trechos que vão para o LLM (como no evals).
pub const TOP_K: usize = 5;
/// Limite do contexto do LLM (~2.000 tokens): cabe no slot de 2.730 tokens da CPU com a resposta.
const CONTEXT_CHARS: usize = 7000;
const ANSWER_MAX_TOKENS: u32 = 300;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct IndexMeta {
    version: u32,
    embedder: String,
}

/// Situação do índice de uma conversa.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    /// Trechos no índice (0: nunca indexada).
    pub total: i64,
    /// Trechos com vetor.
    pub embedded: i64,
    /// Indexada por inteiro, com a versão e o embedder atuais: a busca pode ser usada.
    pub ready: bool,
    /// Há índice, mas de outra versão ou outro embedder: será refeito do zero.
    pub stale: bool,
}

fn current_meta(embedder_id: &str) -> IndexMeta {
    IndexMeta {
        version: INDEX_VERSION,
        embedder: embedder_id.to_string(),
    }
}

pub fn status(db: &Db, embedder_id: &str) -> Result<IndexStatus, ToolError> {
    let (total, embedded) = db.chunk_counts()?;
    let meta: Option<IndexMeta> = db.meta_get(META_KEY)?;
    let stale = total > 0 && meta.as_ref() != Some(&current_meta(embedder_id));
    Ok(IndexStatus {
        total,
        embedded,
        ready: !stale && total > 0 && embedded == total,
        stale,
    })
}

/// Mensagem que entra nos trechos: de uma pessoa e com texto (ou legenda de mídia).
fn indexable(m: &MessageRow) -> bool {
    m.author.is_some() && m.text.as_deref().is_some_and(|t| !t.trim().is_empty())
}

fn clip(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Trechos `(primeira, última)` mensagem: janelas de até [`WINDOW_MESSAGES`] mensagens e
/// [`WINDOW_CHARS`] caracteres, que não atravessam pausas de [`GAP_SECS`] e se sobrepõem em
/// [`OVERLAP`] mensagens.
pub fn plan_chunks(db: &Db) -> Result<Vec<(i64, i64)>, ToolError> {
    // (id, ts, caracteres) das mensagens indexáveis, em blocos separados pelas pausas.
    let mut blocks: Vec<Vec<(i64, usize)>> = Vec::new();
    let mut last_ts = i64::MIN;
    db.for_each_message(|m| {
        if !indexable(&m) {
            return;
        }
        let len = m.text.as_deref().map_or(0, |t| t.chars().count().min(MESSAGE_CHARS)) + 20;
        if blocks.is_empty() || m.ts - last_ts > GAP_SECS {
            blocks.push(Vec::new());
        }
        last_ts = m.ts;
        blocks.last_mut().unwrap().push((m.id, len));
    })?;
    let mut out = Vec::new();
    for block in blocks {
        let mut start = 0;
        while start < block.len() {
            let mut end = start;
            let mut chars = block[start].1;
            while end + 1 < block.len() && end + 1 - start < WINDOW_MESSAGES && chars + block[end + 1].1 <= WINDOW_CHARS
            {
                end += 1;
                chars += block[end].1;
            }
            out.push((block[start].0, block[end].0));
            if end + 1 >= block.len() {
                break;
            }
            // Recua a sobreposição, mas sempre avança.
            start = (end + 1).saturating_sub(OVERLAP).max(start + 1);
        }
    }
    Ok(out)
}

fn day(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// Mensagens indexáveis de um trecho.
fn chunk_messages(db: &Db, start_id: i64, end_id: i64) -> Result<Vec<MessageRow>, ToolError> {
    let rows = db.messages_from(start_id, end_id - start_id + 1)?;
    Ok(rows.into_iter().filter(indexable).collect())
}

/// Texto do trecho para o embedder: a data uma vez e "Pessoa: mensagem" por linha (sem os ids e a
/// data em cada linha, que só gastariam tokens do limite de 512 do e5).
fn passage(rows: &[MessageRow]) -> String {
    let mut text = format!(
        "{}{}",
        core_llm_prefix::PASSAGE,
        rows.first().map(|m| day(m.ts)).unwrap_or_default()
    );
    for m in rows {
        let author = crate::intent::first_name(m.author.as_deref().unwrap_or_default());
        let line = format!(
            "\n{author}: {}",
            clip(m.text.as_deref().unwrap_or_default().trim(), MESSAGE_CHARS)
        );
        if text.chars().count() + line.chars().count() > WINDOW_CHARS + 40 {
            break;
        }
        text.push_str(&line);
    }
    text
}

/// Prefixos do e5 (repetidos aqui para o `core-chat` não depender do `core-llm`).
mod core_llm_prefix {
    pub const QUERY: &str = "query: ";
    pub const PASSAGE: &str = "passage: ";
}

/// Progresso da indexação: trechos com vetor e total.
pub type Progress<'a> = &'a mut dyn FnMut(i64, i64);

/// Indexa a conversa (ou continua de onde parou). Refaz os trechos se o índice for de outra versão
/// ou de outro embedder. `cancel` interrompe entre dois lotes, sem perder o que já foi gravado.
pub fn index(
    db: &mut Db,
    embedder: &dyn Embedder,
    cancel: &AtomicBool,
    progress: Progress,
) -> Result<IndexStatus, ToolError> {
    let embedder_id = embedder.id();
    let st = status(db, embedder_id)?;
    if st.total == 0 || st.stale {
        let chunks = plan_chunks(db)?;
        db.replace_chunks(&chunks)?;
        db.meta_set(META_KEY, &current_meta(embedder_id))?;
    }
    let (total, mut embedded) = db.chunk_counts()?;
    progress(embedded, total);
    while !cancel.load(Ordering::SeqCst) {
        let pending = db.pending_chunks(BATCH as i64)?;
        if pending.is_empty() {
            break;
        }
        let mut texts = Vec::with_capacity(pending.len());
        for c in &pending {
            texts.push(passage(&chunk_messages(db, c.start_id, c.end_id)?));
        }
        let vectors = embedder.embed(&texts).map_err(ToolError::Model)?;
        if vectors.len() != pending.len() {
            return Err(ToolError::Model(format!(
                "o embedder devolveu {} vetores para {} trechos",
                vectors.len(),
                pending.len()
            )));
        }
        let items: Vec<(i64, Vec<f32>)> = pending.iter().map(|c| c.id).zip(vectors).collect();
        db.set_chunk_embeddings(&items)?;
        embedded += items.len() as i64;
        progress(embedded, total);
    }
    status(db, embedder_id)
}

/// Um trecho achado pela busca.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub chunk: ChunkRow,
    pub score: f32,
}

/// Os `k` trechos mais parecidos com a pergunta, do mais para o menos parecido.
pub fn search(db: &Db, embedder: &dyn Embedder, question: &str, k: usize) -> Result<Vec<Hit>, ToolError> {
    let q = format!("{}{}", core_llm_prefix::QUERY, question.trim());
    let qv = embedder
        .embed(&[q])
        .map_err(ToolError::Model)?
        .pop()
        .ok_or_else(|| ToolError::Model("o embedder não devolveu vetor".into()))?;
    let mut hits: Vec<Hit> = Vec::new();
    db.for_each_chunk_vector(|chunk, v| {
        let score: f32 = qv.iter().zip(v).map(|(a, b)| a * b).sum();
        if hits.len() < k || hits.last().is_some_and(|h| score > h.score) {
            let pos = hits.partition_point(|h| h.score >= score);
            hits.insert(pos, Hit { chunk, score });
            hits.truncate(k);
        }
    })?;
    Ok(hits)
}

/// Contexto para o LLM: os trechos achados, unidos quando se sobrepõem, em ordem de data, no
/// formato medido no evals (`[id] AAAA-MM-DD Pessoa: mensagem`). Devolve o texto e as mensagens.
fn context(db: &Db, hits: &[Hit]) -> Result<(String, Vec<MessageRow>), ToolError> {
    let mut ranges: Vec<(i64, i64)> = hits.iter().map(|h| (h.chunk.start_id, h.chunk.end_id)).collect();
    ranges.sort();
    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (s, e) in ranges {
        match merged.last_mut() {
            Some(last) if s <= last.1 + 1 => last.1 = last.1.max(e),
            _ => merged.push((s, e)),
        }
    }
    let mut blocks = Vec::new();
    let mut rows = Vec::new();
    let mut chars = 0;
    // Os trechos mais relevantes primeiro, até o limite; depois, em ordem de data.
    let rank = |r: &(i64, i64)| {
        hits.iter()
            .position(|h| h.chunk.start_id >= r.0 && h.chunk.end_id <= r.1)
            .unwrap_or(usize::MAX)
    };
    let mut by_rank = merged.clone();
    by_rank.sort_by_key(rank);
    let mut chosen = Vec::new();
    for r in by_rank {
        let msgs = chunk_messages(db, r.0, r.1)?;
        let text: Vec<String> = msgs
            .iter()
            .map(|m| {
                format!(
                    "[{}] {} {}: {}",
                    m.id,
                    day(m.ts),
                    m.author.as_deref().unwrap_or_default(),
                    clip(m.text.as_deref().unwrap_or_default().trim(), MESSAGE_CHARS)
                )
            })
            .collect();
        let block = text.join("\n");
        if !chosen.is_empty() && chars + block.len() > CONTEXT_CHARS {
            continue;
        }
        chars += block.len();
        chosen.push((r.0, block, msgs));
    }
    chosen.sort_by_key(|c| c.0);
    for (_, block, msgs) in chosen {
        blocks.push(block);
        rows.extend(msgs);
    }
    Ok((blocks.join("\n\n"), rows))
}

#[derive(Debug, Deserialize)]
struct RagOut {
    found: bool,
    answer: String,
    #[serde(default)]
    citations: Vec<i64>,
}

/// "#123" no texto da resposta.
fn inline_ids(text: &str) -> Vec<i64> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find('#') {
        rest = &rest[i + 1..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(id) = digits.parse() {
            out.push(id);
        }
    }
    out
}

/// Responde a uma pergunta sobre o conteúdo: busca, contexto e LLM. As citações são conferidas
/// contra o contexto; sem nenhuma válida, a resposta vira recusa (o modelo não pode afirmar algo
/// sem mostrar a mensagem).
pub fn answer(
    db: &Db,
    authors: &[String],
    llm: &dyn Llm,
    embedder: &dyn Embedder,
    question: &str,
) -> Result<ChatAnswer, ToolError> {
    let hits = search(db, embedder, question, TOP_K)?;
    let (ctx, rows) = context(db, &hits)?;
    let not_found = |text: &str| ChatAnswer {
        interpreted_by: Some(Interpreter::Llm),
        ..ChatAnswer::text_only(Route::Retrieval, text.to_string())
    };
    if rows.is_empty() {
        return Ok(not_found("Não encontrei mensagens sobre isso na conversa."));
    }
    let (a, b) = (
        authors.first().map(String::as_str).unwrap_or_default(),
        authors.get(1).map(String::as_str).unwrap_or_default(),
    );
    let out = llm
        .complete(
            &llm::rag_prompt(a, b, &ctx),
            question,
            &llm::rag_schema(),
            ANSWER_MAX_TOKENS,
        )
        .map_err(ToolError::Model)?;
    let out: RagOut = serde_json::from_value::<RagOut>(out.clone())
        .map_err(|e| ToolError::Model(format!("resposta fora do formato ({e}): {out}")))?;
    let in_context: BTreeSet<i64> = rows.iter().map(|m| m.id).collect();
    let cited: BTreeSet<i64> = out
        .citations
        .iter()
        .copied()
        .chain(inline_ids(&out.answer))
        .filter(|id| in_context.contains(id))
        .collect();
    if !out.found || cited.is_empty() {
        let text = if out.found {
            "Não achei uma mensagem que confirme isso nos trechos mais parecidos com a pergunta."
        } else {
            out.answer.trim()
        };
        return Ok(not_found(if text.is_empty() {
            "Não encontrei isso na conversa."
        } else {
            text
        }));
    }
    let citations = rows.into_iter().filter(|m| cited.contains(&m.id)).collect();
    Ok(ChatAnswer {
        route: Route::Retrieval,
        text: out.answer.trim().to_string(),
        tool: None,
        result: None,
        interpreted_by: Some(Interpreter::Llm),
        citations,
    })
}
