# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

WhatsRecap: app desktop (Tauri v2 + React 19) que analisa exports de conversas do WhatsApp entre
**duas** pessoas, 100% local. Código, comentários, docs, UI e mensagens de commit são em **pt-BR**.

Princípio central: **código conta, LLM classifica.** Toda contagem/agregação é determinística em Rust;
o LLM (fases 5–9, ainda não implementadas) só classifica trechos ou interpreta perguntas.

Pendências e ordem das próximas fases: `docs/TODO.md`. Contrato de dados para a interface: `docs/design-brief.md`.

## Comandos

```sh
pnpm install
pnpm tauri dev                    # app em desenvolvimento (Vite em :1420)
pnpm build                        # tsc -b + vite build (é o que o CI do frontend roda)
pnpm typecheck

cargo test --workspace
cargo test -p core-stats <nome>   # um teste específico de um crate
cargo fmt --all --check           # max_width = 120 (rustfmt.toml)
cargo clippy --workspace --all-targets -- -D warnings
```

O CI roda fmt, clippy com `-D warnings` e testes; warnings de clippy quebram o build. Para compilar
`src-tauri` sem o frontend buildado, basta existir `dist/index.html` (o CI cria um placeholder).

Ferramentas de linha de comando:

```sh
cargo run --release -p synth -- --days 1825 --per-day 120 [--media] -o conversa.{txt,zip}   # conversa sintética
cargo run --release -p core-stats --example analyze -- conversa.zip [--json stats.json]       # importa + estatísticas com tempos
cargo run --release -p core-chat --example ask -- conversa.db "Quem responde mais rápido?"    # testa o chat num .db
cargo run -p synth --bin anonymize -- export.txt -o crates/core-parser/tests/fixtures/x.txt  # export real → fixture
```

Fixtures do parser (`crates/core-parser/tests/fixtures/`): cada `.txt` tem um snapshot `.expected.json`.
Um `.txt` sem snapshot faz o teste falhar e gera o arquivo; revise e rode de novo. Para regenerar
todos: `UPDATE_FIXTURES=1 cargo test -p core-parser`. Nunca commitar exports reais sem passar pelo `anonymize`.

## Arquitetura

Pipeline em crates do workspace (`crates/*` + `src-tauri`), cada um dependendo dos anteriores:

1. **core-parser** — `.txt`/`.zip` → mensagens normalizadas. Parse em streaming em duas passadas:
   `detect_format` varre tudo para descobrir plataforma (Android/iOS), ordem dia/mês e autores
   (recusa > 2 autores); `MessageReader` então itera linha a linha. `classify.rs` mapeia marcadores
   de mídia/apagada/sistema por idioma.
2. **core-storage** — um SQLite por conversa em `<app_data>/conversations/<id>.db`. A importação
   escreve em `.db.partial` e renomeia no fim (importação interrompida não deixa banco pela metade);
   inserção em lotes, índices e FTS5 criados depois da carga. Ids de mensagem são sequenciais a partir
   de 1 — o frontend depende disso (índice na lista virtualizada = `id - 1`). Migrações em
   `schema::MIGRATIONS`, aplicadas por `PRAGMA user_version` em `Db::open`: ao mudar o schema,
   incremente `schema::VERSION` e adicione uma migração (há teste de migração em `tests.rs`).
3. **core-stats** — features 1–18 calculadas a partir do banco (rayon). Resultado (`Stats`) fica em
   cache na tabela `stats_cache`; ao mudar o formato de `Stats`, **incremente `STATS_VERSION`** para
   invalidar caches antigos. `StatsSettings` (gap de sessão etc.) fica no banco e recalcula ao mudar.
4. **core-media** — figurinhas (SHA-256 + dHash agrupados por union-find, miniaturas PNG gravadas no
   banco) e duração de áudios lida do container Ogg sem decodificar. Só no modo `.zip`.
5. **core-chat** — chat tipo 1 (perguntas quantitativas): `router::route` → `intent::interpret`
   (heurística) → `tools::Conversation::execute` (consulta SQL) → `answer::compose` (template com os
   números). `llm.rs` guarda os schemas/prompts para quando o LLM entrar nos casos não resolvidos.
6. **core-llm** — acesso ao modelo local: sobe o `llama-server` (llama.cpp) como processo filho e
   chama `/v1/chat/completions` com saída restrita por JSON schema. Genérico (não conhece o chat).
   Testes usam um servidor HTTP falso; o que carrega o modelo de verdade é `#[ignore]` e o example `ferramenta`.
7. **synth** — gerador de conversas sintéticas (com mídia real WebP/Ogg) e o anonimizador de fixtures.

**src-tauri/src/lib.rs** — todos os `#[tauri::command]`. Trabalho pesado roda em `blocking(...)`;
a importação é um job em background com eventos de progresso e cancelamento (`ImportState`).

**src/** — React + Vite + TS, sem roteador: `App.tsx` alterna as views (`home`, `dashboard`,
`viewer`, `settings`, `chat`) por estado. Toda chamada ao backend passa por `src/api.ts`; os tipos
espelhados do Rust ficam em `src/types.ts` (structs Rust usam `serde(rename_all = "camelCase")`).
Ao adicionar/alterar um command, atualize lib.rs (incluindo o `invoke_handler`), `api.ts` e `types.ts`
juntos. O frontend nunca recebe a conversa inteira: mensagens em páginas sob demanda
(`get_messages`, react-virtuoso), busca via FTS5 e estatísticas pré-calculadas. Gráficos são SVG
próprios (`components/charts.tsx`). Números e datas sempre no formato pt-BR (`src/format.ts`).

## Premissas

- Timestamps são gravados como horário local do export, sem fuso.
- Emojis são contados por grapheme cluster (`Extended_Pictographic`, tabela gerada em `emoji_table.rs`).
- Os marcadores de formato ainda não foram validados com todos os exports reais; prefira adicionar
  uma fixture anonimizada ao corrigir o parser.
