use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("erro de leitura: {0}")]
    Io(#[from] std::io::Error),
    #[error("arquivo .zip inválido: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("o .zip não contém o arquivo .txt da conversa")]
    NoChatInZip,
}

/// Origem do export: `.txt` (modo sem mídia) ou `.zip` (modo com mídia).
#[derive(Debug, Clone)]
pub enum Source {
    Txt {
        path: PathBuf,
    },
    Zip {
        path: PathBuf,
        chat_entry: usize,
        chat_name: String,
    },
}

impl Source {
    /// Detecta o modo pelo conteúdo (assinatura `PK` do zip), não só pela extensão.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SourceError> {
        let path = path.as_ref().to_path_buf();
        let mut magic = [0u8; 4];
        let n = File::open(&path)?.read(&mut magic)?;
        if n < 4 || &magic != b"PK\x03\x04" {
            return Ok(Source::Txt { path });
        }
        let mut archive = zip::ZipArchive::new(File::open(&path)?)?;
        let mut best: Option<(u8, usize, String)> = None;
        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            let name = entry.name().to_string();
            if entry.is_dir() || !name.to_lowercase().ends_with(".txt") {
                continue;
            }
            let base = name.rsplit('/').next().unwrap_or(&name);
            // `_chat.txt` no iOS; no Android, o .txt fica na raiz do zip.
            let score = if base == "_chat.txt" {
                0
            } else if !name.contains('/') {
                1
            } else {
                2
            };
            if best.as_ref().is_none_or(|(s, _, _)| score < *s) {
                best = Some((score, i, name));
            }
        }
        let (_, chat_entry, chat_name) = best.ok_or(SourceError::NoChatInZip)?;
        Ok(Source::Zip {
            path,
            chat_entry,
            chat_name,
        })
    }

    pub fn path(&self) -> &Path {
        match self {
            Source::Txt { path } | Source::Zip { path, .. } => path,
        }
    }

    pub fn has_media(&self) -> bool {
        matches!(self, Source::Zip { .. })
    }

    /// Abre o texto da conversa e chama `f` com um leitor e o tamanho total em bytes.
    /// Pode ser chamado mais de uma vez (a detecção de formato e o parse são passadas separadas).
    pub fn with_reader<T, E>(&self, f: impl FnOnce(&mut dyn BufRead, u64) -> Result<T, E>) -> Result<T, E>
    where
        E: From<SourceError>,
    {
        match self {
            Source::Txt { path } => {
                let file = File::open(path).map_err(SourceError::from)?;
                let size = file.metadata().map_err(SourceError::from)?.len();
                f(&mut BufReader::with_capacity(1 << 16, file), size)
            }
            Source::Zip { path, chat_entry, .. } => {
                let file = File::open(path).map_err(SourceError::from)?;
                let mut archive = zip::ZipArchive::new(file).map_err(SourceError::from)?;
                let entry = archive.by_index(*chat_entry).map_err(SourceError::from)?;
                let size = entry.size();
                f(&mut BufReader::with_capacity(1 << 16, entry), size)
            }
        }
    }

    /// Lê, em ordem, os arquivos do `.zip` cujo nome (sem pasta) é aceito por `want`, chamando
    /// `f(nome, bytes)` para cada um. `f` retorna `false` para interromper.
    pub fn read_entries(
        &self,
        mut want: impl FnMut(&str) -> bool,
        mut f: impl FnMut(&str, Vec<u8>) -> bool,
    ) -> Result<(), SourceError> {
        let Source::Zip { path, chat_entry, .. } = self else {
            return Ok(());
        };
        let mut archive = zip::ZipArchive::new(File::open(path)?)?;
        for i in 0..archive.len() {
            if i == *chat_entry {
                continue;
            }
            let mut entry = archive.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().rsplit('/').next().unwrap_or_default().to_string();
            if !want(&name) {
                continue;
            }
            let mut bytes = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut bytes)?;
            if !f(&name, bytes) {
                break;
            }
        }
        Ok(())
    }

    /// Nomes dos arquivos de mídia dentro do `.zip` (vazio no modo sem mídia).
    pub fn media_files(&self) -> Result<Vec<String>, SourceError> {
        let Source::Zip { path, chat_entry, .. } = self else {
            return Ok(Vec::new());
        };
        let archive = zip::ZipArchive::new(File::open(path)?)?;
        Ok(archive
            .file_names()
            .enumerate()
            .filter(|(i, n)| *i != *chat_entry && !n.ends_with('/'))
            .map(|(_, n)| n.to_string())
            .collect())
    }
}

/// Envolve um `BufRead` contando os bytes consumidos, para relatar progresso.
pub struct CountingReader<'a> {
    inner: &'a mut dyn BufRead,
    count: Arc<AtomicU64>,
}

impl<'a> CountingReader<'a> {
    pub fn new(inner: &'a mut dyn BufRead) -> (Self, Arc<AtomicU64>) {
        let count = Arc::new(AtomicU64::new(0));
        (
            Self {
                inner,
                count: count.clone(),
            },
            count,
        )
    }
}

impl Read for CountingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.count.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
}

impl BufRead for CountingReader<'_> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.inner.fill_buf()
    }
    fn consume(&mut self, amt: usize) {
        self.count.fetch_add(amt as u64, Ordering::Relaxed);
        self.inner.consume(amt);
    }
}
