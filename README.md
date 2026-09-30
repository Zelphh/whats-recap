# WhatsRecap

Analisador de conversas do WhatsApp entre duas pessoas. App desktop em **Tauri v2** (Rust + React),
**100% local**: nenhuma conversa sai da máquina.

> Princípio central: **código conta, LLM classifica.** Toda contagem e agregação é determinística;
> o LLM (fases futuras) só classifica trechos, e os números finais saem sempre do código.

## Estado atual

| Fase | Entrega | Status |
|---|---|---|
| 0 — Setup | Workspace Rust, app Tauri + React, CI | ✅ |
| 1 — Parser e storage | Android/iOS, detecção dia/mês, SQLite + FTS5, sessões, testes | ✅ |
| 2 — Estatísticas sem mídia | Features 1–16 | ✅ |
| 3 — Dashboard | Gráficos, cards, visualizador virtualizado, gráfico em texto | ✅ |
| 4 — Modo com mídia | Figurinhas (SHA-256 + dHash) e áudios (duração Ogg) | ✅ |
| 5–9 — LLM e chat | llama-server (Vulkan), desculpas, brigas, chat | ⏳ depende da máquina final (RX 7600) |

## Estrutura

```
crates/
  core-parser    .txt / .zip → mensagens normalizadas (streaming, 2 passadas)
  core-storage   SQLite por conversa: mensagens, sessões, FTS5, cache de estatísticas
  core-stats     features 1–18 (paralelizadas com rayon)
  core-media     figurinhas (SHA-256, dHash, miniaturas) e áudios (duração Opus) lidos do .zip
  synth          gerador de conversas sintéticas (demos e testes sem dados reais)
src-tauri/       app Tauri: commands, importação em background com eventos de progresso
src/             React + Vite + TS: importação, dashboard (gráficos em SVG), visualizador (react-virtuoso)
```

Cada conversa importada vira `<app_data>/conversations/<id>.db`
(no Linux: `~/.local/share/dev.whatsrecap.app/conversations/`).

## Rodando

Pré-requisitos: Rust estável, Node 22+, pnpm e as
[dependências de sistema do Tauri](https://v2.tauri.app/start/prerequisites/) (WebKitGTK no Linux).

```sh
pnpm install
pnpm tauri dev          # app em modo desenvolvimento
pnpm tauri build        # instalador de release

cargo test --workspace  # testes do backend
```

### Conversas sintéticas

```sh
cargo run --release -p synth -- --days 1825 --per-day 120 --platform ios -o /tmp/conversa.txt
cargo run --release -p core-stats --example analyze -- /tmp/conversa.txt

# Com mídia: .zip com figurinhas WebP reais (inclusive reencodadas) e áudios Ogg Opus
cargo run --release -p synth -- --days 1825 --per-day 120 --media -o /tmp/conversa.zip
cargo run --release -p core-stats --example analyze -- /tmp/conversa.zip --json /tmp/stats.json
```

`analyze` importa o arquivo e imprime um resumo com os tempos de cada etapa. Referência nesta máquina
de desenvolvimento: 5 anos / ~190 mil mensagens → importação em ~0,6 s, estatísticas em ~0,1 s;
com mídia, ~5,7 mil figurinhas e áudios são indexados em ~0,5 s.

## Decisões técnicas

- **Parse em streaming, em duas passadas.** A primeira percorre o arquivo inteiro para descobrir
  plataforma, ordem dia/mês (um campo > 12 é o dia; se tudo for ambíguo, vence a interpretação que
  mantém as datas em ordem cronológica) e autores. A segunda lê linha a linha e insere em lotes de
  10 mil dentro de transações. O arquivo nunca é carregado inteiro em memória.
- **Banco montado em `.db.partial` e renomeado ao final**: uma importação interrompida nunca deixa um
  banco pela metade. Índices e FTS5 são criados depois da carga em lote.
- **Ids sequenciais a partir de 1**: o índice na lista virtualizada é `id - 1`, então saltar para
  uma mensagem (mais longa, resultado de busca, citação futura do LLM) é O(1).
- **Timestamps sem fuso**: o horário local do export é gravado como está (ambos no mesmo fuso, por premissa).
- **Sessões** (gap padrão de 6h, configurável) servem ao tempo de resposta e, nas próximas fases,
  ao chunking do LLM e às unidades do RAG.
- **Emojis por grapheme cluster** com a propriedade Unicode `Extended_Pictographic`
  (tabela gerada de `emoji-data.txt`): 👨‍👩‍👧, bandeiras e tons de pele contam como um emoji.
- **Figurinhas**: SHA-256 do conteúdo (a mesma figurinha chega com nomes diferentes) e dHash de 64 bits
  do primeiro frame, com a transparência composta sobre branco (senão o RGB "invisível" muda o hash).
  Hashes a até 4 bits de distância (configurável) são unidos por union-find, o que agrupa reencodes.
  Uma miniatura PNG por conteúdo distinto fica no banco, então o `.zip` não precisa continuar acessível.
- **Áudios**: duração lida do container Ogg sem decodificar: granule position da última página,
  menos o *pre-skip* do `OpusHead`, ÷ 48.000.
- **Jobs**: a importação (texto + mídia) roda em background com eventos de progresso e pode ser
  cancelada; o banco só fica no disco se tudo terminar.
- **Migrações de schema** incrementais (`PRAGMA user_version`): bancos de versões anteriores são atualizados ao abrir.
- **"Você usa muito mais"** usa log-odds com prior de Dirichlet informativo (z ≥ 1,96 e razão ≥ 1,5×).
- **O frontend nunca recebe a conversa inteira**: páginas de 200 mensagens sob demanda, busca via FTS5
  e estatísticas pré-calculadas em cache (`stats_cache`), recalculadas ao mudar as configurações.

## Formatos suportados

- Android: `dd/mm/aaaa hh:mm - Autor: msg` (também `,` após a data, AM/PM e separadores `.`/`-`)
- iOS: `[dd/mm/aaaa, hh:mm:ss] Autor: msg`, com remoção de `U+200E` e espaços especiais
- Tipos: texto, figurinha, áudio, imagem, vídeo, documento, apagada, sistema e mídia oculta;
  marcadores de edição são removidos. Conversas com mais de 2 autores são recusadas.

Os marcadores foram escritos a partir da documentação de formatos conhecidos (pt-BR, en, parte do es).
**Ainda precisam ser validados com exports reais**, de Android e iOS, com e sem mídia.
