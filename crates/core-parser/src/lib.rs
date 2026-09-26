//! Parser de exports do WhatsApp (Android e iOS, `.txt` ou `.zip`).
//!
//! O parse é feito em duas passadas em streaming, sem carregar o arquivo em memória:
//! 1. [`detect_format`] percorre o arquivo inteiro para descobrir a plataforma, a ordem
//!    dos campos da data (dia/mês) e os autores;
//! 2. [`MessageReader`] lê linha a linha e emite [`ParsedMessage`] já classificadas.

mod classify;
mod format;
mod source;

pub use classify::classify_body;
pub use classify::kind_from_file;
pub use format::{ChatFormat, DateOrder, FormatError, Platform, detect_format};
pub use source::{CountingReader, Source, SourceError};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::io::BufRead;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    Sticker,
    Audio,
    Image,
    Video,
    Doc,
    Deleted,
    System,
    MediaHidden,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageKind::Text => "text",
            MessageKind::Sticker => "sticker",
            MessageKind::Audio => "audio",
            MessageKind::Image => "image",
            MessageKind::Video => "video",
            MessageKind::Doc => "doc",
            MessageKind::Deleted => "deleted",
            MessageKind::System => "system",
            MessageKind::MediaHidden => "media_hidden",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "text" => MessageKind::Text,
            "sticker" => MessageKind::Sticker,
            "audio" => MessageKind::Audio,
            "image" => MessageKind::Image,
            "video" => MessageKind::Video,
            "doc" => MessageKind::Doc,
            "deleted" => MessageKind::Deleted,
            "system" => MessageKind::System,
            "media_hidden" => MessageKind::MediaHidden,
            _ => return None,
        })
    }

    /// Mensagens de mídia (com ou sem o arquivo disponível).
    pub fn is_media(self) -> bool {
        matches!(
            self,
            MessageKind::Sticker
                | MessageKind::Audio
                | MessageKind::Image
                | MessageKind::Video
                | MessageKind::Doc
                | MessageKind::MediaHidden
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMessage {
    /// Horário local, exatamente como aparece no export.
    pub ts: NaiveDateTime,
    /// `None` para mensagens de sistema.
    pub author: Option<String>,
    pub kind: MessageKind,
    /// Texto da mensagem (ou legenda da mídia). Marcadores de edição já removidos.
    pub text: Option<String>,
    /// Nome do arquivo de mídia referenciado, quando o export inclui mídia.
    pub media_file: Option<String>,
}

/// Remove caracteres invisíveis de controle de direção (comuns em exports do iOS)
/// e normaliza espaços não separáveis.
pub fn clean_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for c in line.chars() {
        match c {
            '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}' => {}
            '\u{00A0}' | '\u{202F}' | '\u{2007}' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ReaderStats {
    pub lines: u64,
    pub messages: u64,
    /// Linhas sem timestamp antes da primeira mensagem (não pertencem a nenhuma mensagem).
    pub orphan_lines: u64,
    /// Cabeçalhos com data/hora inválida (tratados como continuação da mensagem anterior).
    pub invalid_dates: u64,
}

struct Pending {
    ts: NaiveDateTime,
    author: Option<String>,
    body: String,
    marked: bool,
}

/// Iterador em streaming sobre as mensagens de um export.
pub struct MessageReader<R: BufRead> {
    reader: R,
    format: ChatFormat,
    buf: Vec<u8>,
    pending: Option<Pending>,
    done: bool,
    stats: ReaderStats,
}

impl<R: BufRead> MessageReader<R> {
    pub fn new(reader: R, format: ChatFormat) -> Self {
        Self {
            reader,
            format,
            buf: Vec::with_capacity(512),
            pending: None,
            done: false,
            stats: ReaderStats::default(),
        }
    }

    pub fn stats(&self) -> ReaderStats {
        self.stats
    }

    fn finish(&mut self, p: Pending) -> ParsedMessage {
        self.stats.messages += 1;
        let c = classify_body(p.author.is_some(), &p.body, p.marked);
        ParsedMessage {
            ts: p.ts,
            author: if c.kind == MessageKind::System { None } else { p.author },
            kind: c.kind,
            text: c.text,
            media_file: c.media_file,
        }
    }
}

impl<R: BufRead> Iterator for MessageReader<R> {
    type Item = std::io::Result<ParsedMessage>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.done {
                return self.pending.take().map(|p| Ok(self.finish(p)));
            }
            self.buf.clear();
            match self.reader.read_until(b'\n', &mut self.buf) {
                Err(e) => return Some(Err(e)),
                Ok(0) => {
                    self.done = true;
                    continue;
                }
                Ok(_) => {}
            }
            self.stats.lines += 1;
            let raw = String::from_utf8_lossy(&self.buf);
            let raw = raw.trim_end_matches(['\n', '\r']);

            match self.format.parse_header(raw) {
                Some(Ok(h)) => {
                    let next = Pending {
                        ts: h.ts,
                        author: h.author,
                        body: h.body,
                        marked: h.marked,
                    };
                    if let Some(prev) = self.pending.replace(next) {
                        return Some(Ok(self.finish(prev)));
                    }
                }
                other => {
                    if matches!(other, Some(Err(()))) {
                        self.stats.invalid_dates += 1;
                    }
                    match &mut self.pending {
                        Some(p) => {
                            p.body.push('\n');
                            p.body.push_str(&clean_line(raw));
                        }
                        None => self.stats.orphan_lines += 1,
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
