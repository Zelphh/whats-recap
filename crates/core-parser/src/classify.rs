use crate::MessageKind;
use regex::Regex;
use std::sync::LazyLock;

/// Separa `Autor: mensagem`. Linhas sem autor (mensagens de sistema do Android) retornam `None`.
pub(crate) fn split_author(rest: &str) -> (Option<String>, &str) {
    if let Some(pos) = rest.find(": ") {
        let author = crate::clean_line(&rest[..pos]);
        let author = author.trim();
        // Nomes de contato são curtos; frases longas com ": " são mensagens de sistema.
        if !author.is_empty() && author.chars().count() <= 60 {
            return (Some(author.to_string()), &rest[pos + 2..]);
        }
    }
    (None, rest)
}

pub struct Classified {
    pub kind: MessageKind,
    pub text: Option<String>,
    pub media_file: Option<String>,
}

static EDITED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)[ \t]*<(?:mensagem editada|esta mensagem foi editada|this message was edited|mensaje editado|se editó este mensaje)>",
    )
    .unwrap()
});

// Android: `IMG-20230101-WA0001.jpg (arquivo anexado)`, com legenda opcional nas linhas seguintes.
static ANDROID_ATTACHED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(.+?\.[a-z0-9]{2,5})(?: • [^()]*)? \((?:arquivo anexado|file attached|archivo adjunto)\)$")
        .unwrap()
});

// iOS: `<anexado: 00000012-PHOTO-2023-01-01-10-00-00.jpg>`
static IOS_ATTACHED_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<(?:anexado|attached|adjunto): ([^>]+)>").unwrap());

const DELETED: &[&str] = &[
    "mensagem apagada",
    "esta mensagem foi apagada",
    "você apagou esta mensagem",
    "this message was deleted",
    "you deleted this message",
    "se eliminó este mensaje",
    "eliminaste este mensaje",
];

const HIDDEN_GENERIC: &[&str] = &[
    "<mídia oculta>",
    "<arquivo de mídia oculto>",
    "<media omitted>",
    "<multimedia omitido>",
];

// iOS informa o tipo mesmo quando o export é feito sem mídia.
const HIDDEN_TYPED: &[(&str, MessageKind)] = &[
    ("imagem ocultada", MessageKind::Image),
    ("imagem omitida", MessageKind::Image),
    ("image omitted", MessageKind::Image),
    ("vídeo omitido", MessageKind::Video),
    ("vídeo ocultado", MessageKind::Video),
    ("video omitted", MessageKind::Video),
    ("gif omitido", MessageKind::Video),
    ("gif omitted", MessageKind::Video),
    ("áudio ocultado", MessageKind::Audio),
    ("áudio omitido", MessageKind::Audio),
    ("audio omitted", MessageKind::Audio),
    ("figurinha omitida", MessageKind::Sticker),
    ("figurinha ocultada", MessageKind::Sticker),
    ("sticker omitted", MessageKind::Sticker),
];

const HIDDEN_DOC_SUFFIX: &[&str] = &[
    "documento omitido",
    "document omitted",
    "contato omitido",
    "contact card omitted",
];

// Frases de mensagens de sistema. Para mensagens com autor, só são consideradas quando o
// iOS marca o corpo com `U+200E`, para não confundir com texto digitado pelo usuário.
const SYSTEM_PHRASES: &[&str] = &[
    "criptografia de ponta a ponta",
    "end-to-end encrypted",
    "chamada de voz perdida",
    "chamada de vídeo perdida",
    "ligação de voz perdida",
    "ligação de vídeo perdida",
    "missed voice call",
    "missed video call",
    "mudou o número",
    "mudou para um novo número",
    "changed their phone number",
    "changed to a new number",
    "mensagens temporárias",
    "disappearing messages",
    "código de segurança",
    "security code",
    "bloqueou este contato",
    "desbloqueou este contato",
    "blocked this contact",
    "unblocked this contact",
    "é um contato",
    "is a contact",
    "conta comercial",
    "business account",
];

fn is_system_phrase(lower: &str) -> bool {
    SYSTEM_PHRASES.iter().any(|p| lower.contains(p))
}

/// Infere o tipo da mídia pelo nome do arquivo gerado pelo WhatsApp.
pub fn kind_from_file(name: &str) -> MessageKind {
    let up = name.to_uppercase();
    let ext = up.rsplit_once('.').map_or("", |(_, e)| e);
    if up.starts_with("STK-") || up.contains("-STICKER-") {
        MessageKind::Sticker
    } else if up.starts_with("PTT-") || up.starts_with("AUD-") || up.contains("-AUDIO-") {
        MessageKind::Audio
    } else if up.starts_with("VID-") || up.contains("-VIDEO-") || up.contains("-GIF-") {
        MessageKind::Video
    } else if up.starts_with("IMG-") || up.contains("-PHOTO-") {
        MessageKind::Image
    } else {
        match ext {
            "WEBP" => MessageKind::Sticker,
            "OPUS" | "OGG" | "M4A" | "MP3" | "AAC" | "AMR" | "WAV" => MessageKind::Audio,
            "MP4" | "3GP" | "MOV" | "MKV" | "WEBM" | "GIF" => MessageKind::Video,
            "JPG" | "JPEG" | "PNG" | "HEIC" | "BMP" => MessageKind::Image,
            _ => MessageKind::Doc,
        }
    }
}

fn non_empty(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

/// Classifica o corpo de uma mensagem (já sem caracteres invisíveis).
/// `marked` indica que o corpo original começava com `U+200E`.
pub fn classify_body(has_author: bool, body: &str, marked: bool) -> Classified {
    if !has_author {
        return Classified {
            kind: MessageKind::System,
            text: non_empty(body),
            media_file: None,
        };
    }
    let body = EDITED_RE.replace_all(body, "");
    let trimmed = body.trim();
    let lower = trimmed.to_lowercase();
    let lower_no_dot = lower.trim_end_matches('.');

    if DELETED.contains(&lower_no_dot) {
        return Classified {
            kind: MessageKind::Deleted,
            text: None,
            media_file: None,
        };
    }
    if HIDDEN_GENERIC.contains(&lower.as_str()) {
        return Classified {
            kind: MessageKind::MediaHidden,
            text: None,
            media_file: None,
        };
    }
    if let Some((_, kind)) = HIDDEN_TYPED.iter().find(|(p, _)| *p == lower) {
        return Classified {
            kind: *kind,
            text: None,
            media_file: None,
        };
    }
    if marked && HIDDEN_DOC_SUFFIX.iter().any(|s| lower.ends_with(s)) {
        return Classified {
            kind: MessageKind::Doc,
            text: None,
            media_file: None,
        };
    }

    let (first, caption) = trimmed.split_once('\n').unwrap_or((trimmed, ""));
    if let Some(c) = ANDROID_ATTACHED_RE.captures(first.trim()) {
        let file = c[1].trim().to_string();
        return Classified {
            kind: kind_from_file(&file),
            text: non_empty(caption),
            media_file: Some(file),
        };
    }
    if let Some(c) = IOS_ATTACHED_RE.captures(trimmed) {
        let file = c[1].trim().to_string();
        let whole = c.get(0).unwrap();
        let rest = format!("{}{}", &trimmed[..whole.start()], &trimmed[whole.end()..]);
        return Classified {
            kind: kind_from_file(&file),
            text: non_empty(&rest),
            media_file: Some(file),
        };
    }

    if marked && is_system_phrase(&lower) {
        return Classified {
            kind: MessageKind::System,
            text: non_empty(trimmed),
            media_file: None,
        };
    }
    Classified {
        kind: MessageKind::Text,
        text: non_empty(&body),
        media_file: None,
    }
}
