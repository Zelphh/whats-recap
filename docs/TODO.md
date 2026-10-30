# TODO

Pendências do WhatsRecap, em ordem de dependência. Fases 0–4 e o chat tipo 1 sem LLM estão
prontos (ver [README](../README.md)); a máquina alvo (Ryzen 7 5700X + RX 7600 8 GB, Windows 11) já
compila o app e passa em `cargo test --workspace`.

## 1. Benchmark de modelos (`../whatsrecap-evals`)

**Modelo escolhido (2026-10-29): Qwen3.5 9B `IQ4_XS`, sem flash attention**; alternativa Qwen3.5 4B
`Q4_K_M`. Números e achados em `../whatsrecap-evals/COMPARACAO-MODELOS.md`.

- [x] Decidir o modelo: Qwen3.5 (foco em perguntas gerais; o Gemma perdeu em RAG, roteador e redação)
- [ ] Commitar o suporte a Windows no evals (caminho do `core-chat`, `.exe`, encerramento do
      servidor, hardware/VRAM via `typeperf`, `scripts/setup-llama-cpp.ps1`, perfil `rx7600-nofa`)
- [x] llama.cpp `b11938` (Vulkan, Windows) em `vendor/`; `evals hw` → `"is_target": true`
- [x] Baixados: `qwen3.5-4b-q4km`, `qwen3.5-9b-iq4xs`, `gemma4-e4b-q4_0`, `e5-small-q8`
- [x] `evals bench`: os três cabem (pico 4,5 / 6,5 / 4,5 GB, com ~1,5 GB do desktop)
- [x] `evals run -n 3` (v1) e `-v v2` em A–C
- [x] `evals retrieval e5-small-q8`: só-vetorial MRR 0,84 > híbrida 0,70 > FTS 0,63; indexação
      ~20 min para 190 mil mensagens na CPU
- [ ] `evals retrieval bge-m3-q8` e FTS com `AND`/frases
- [ ] Brigas (E): Gemma 0,86 × Qwen 0,46/0,31. Qwen pede `need_context: "after"` quase sempre,
      junta brigas e marca zoeira; iterar o prompt (variante v2 de E) antes de descartar o Qwen
- [ ] VRAM do 9B sem FA: pico de 7,2 GB de 8 GB (com o desktop). Testar `-c 8192` e `-ctk q8_0`
      sem FA para abrir folga
- [ ] Testar Qwen3.5 9B `Q4_K_M` (5,7 GB; pode não caber com 16k sem FA)
- [ ] Opcional: comparar a build ROCm do llama.cpp (`-Backend rocm-10.0`) com a Vulkan
- [ ] Corrigir a extrapolação do modo rápido no `bench.rs` (usa a saída de um chunk inteiro por
      janela; a latência real de D+E dá ~50 min no Qwen 4B, ~65 no 9B, ~105 no Gemma)
- [ ] Ampliar os datasets de B (40) e F (18): diferenças de poucos pontos ainda são ruído
- [ ] Levar a tabela e a recomendação para o README do app

### Achados que mudam o app

- [x] **Ordem das chaves no `tool_schema`.** O schema chegava ao llama.cpp com `"args"` antes de
      `"tool"` (chaves em ordem alfabética), e o modelo preenchia os argumentos antes de escolher a
      ferramenta. Corrigido com `serde_json/preserve_order` no `core-chat` + teste
      `tool_schema_puts_tool_before_args`.
- [x] **Normalizar datas da `ToolCall`.** `ToolCall::without_redundant_range` descarta `from`/`to`
      que não restringem nada. Com as duas correções, B no Qwen 9B (v2) foi de 14% para 76%.
- [ ] **Usar `without_redundant_range` no caminho do LLM** (fase 8), antes de executar a ferramenta
- [ ] **Prompt de B:** "quem…?" ainda vem com `author`, "eu" vira uma das pessoas, `weekday` ×
      `day`/`week`, "ano passado" vira os últimos 12 meses em vez do ano civil
- [ ] **Flash attention por modelo.** No Vulkan/Windows, `-fa on` deixa os Qwen 2–2,5× mais lentos
      no `llama-server` (modo completo 5,7 h → 2,4 h no 4B); o Gemma é o contrário (geração cai de
      32 para 19 tok/s sem FA). O sidecar precisa de flags por modelo.

## 2. Fase 5 — Infraestrutura de LLM no app

- [ ] Prompts vencedores do benchmark de volta para `crates/core-chat/src/llm.rs`
      (se a `v2` vencer: data de hoje no prompt, saída "nenhuma ferramenta", `authors` e datas
      legíveis no `ToolResult`)
- [ ] Cliente HTTP do `llama-server` com `response_format: json_schema` (base: `client.rs` do evals)
- [ ] `llama-server` como sidecar do Tauri
  - [ ] Binário em `externalBin` com o sufixo do target (`llama-server-x86_64-pc-windows-msvc.exe`)
  - [ ] DLLs do ggml/Vulkan como recursos do bundle
  - [ ] Subir sem janela de console (`CREATE_NO_WINDOW`)
  - [ ] Iniciar sob demanda, `/health`, porta livre, encerrar ao fechar o app
  - [ ] Flags do Qwen3.5 9B: `-ngl 99 -c 16384 -fa off --reasoning off` (ajustar se o teste de
        VRAM pedir `-c 8192`/`-ctk q8_0`)
  - [ ] Fallback para o 4B (ou CPU) se a GPU não tiver VRAM livre suficiente
- [ ] Download do modelo pelo app na primeira vez (`unsloth/Qwen3.5-9B-GGUF`,
      `Qwen3.5-9B-IQ4_XS.gguf`, 4,8 GiB; progresso, retomada, checagem de tamanho/hash); o GGUF não
      entra no instalador
- [ ] Tela de status do LLM nas configurações (modelo, baixado ou não, GPU detectada)

## 3. Fase 8 — Chat tipo 1 com LLM

- [ ] Roteador: casos `Undecided` da heurística vão para o LLM (`router_schema`)
- [ ] Seleção de ferramenta pelo LLM quando a heurística não entende a pergunta
- [ ] Resposta "fora do escopo" elegante quando nenhuma ferramenta serve
- [ ] Redação pelo LLM (opcional): manter os templates de `answer.rs` como padrão/fallback

## 4. Fase 6 — Desculpas (feature 19)

- [ ] Pré-filtro em Rust (desculpa, desculpe, foi mal, perdão, me perdoa, sorry e variações)
- [ ] Prompt com few-shot e schema (proposta em `contracts.rs` do evals)
- [ ] Pipeline em background com checkpoint em `llm_jobs` e resultado em `llm_events`
- [ ] UI: contagem por pessoa, sinceras × cortesia, eventos clicáveis no visualizador

## 5. Fase 7 — Brigas (feature 20)

- [ ] Pré-filtro (CAPS, `?!`, troca rápida de mensagens curtas, palavrões, respostas secas, apagadas)
- [ ] Expansão determinística de contexto na borda do chunk + `need_context` (máx. 3 expansões)
- [ ] União de conflitos sobrepostos entre chunks
- [ ] UI: lista de brigas com intensidade, resolvida, resumo e link para o trecho
- [ ] Calibrar sobreposição entre chunks (20–30 mensagens) e tamanho de janela

## 6. Fase 9 — Chat tipo 2 / RAG (feature 22)

- [ ] `sqlite-vec` no `core-storage`; preencher a tabela `chunks`
- [ ] Indexação com embeddings na CPU (`e5-small-q8`: ~20 min para ~190 mil mensagens)
- [ ] Busca: hoje a só-vetorial vence a híbrida (MRR 0,84 × 0,70); só adotar RRF se o FTS com
      `AND`/frases ou o `bge-m3` mudarem isso
- [ ] Resposta com citações `#id` que viram links no visualizador; recusa quando não há resposta
- [ ] Calibrar top-k e o tamanho das janelas

## Fora do LLM

- [ ] Validar o parser com exports reais (Android e iOS, com e sem mídia), anonimizados com
      `cargo run -p synth --bin anonymize`; nada sai da máquina nem vai para o git
- [ ] README: pré-requisitos e caminhos do Windows (WebView2, `%APPDATA%\dev.whatsrecap.app`)
- [ ] Atualizar `CONTEXTO-WHATSRECAP.md` do evals (máquina alvo tem 32 GB de RAM e roda Windows)
- [ ] Testar `pnpm tauri build` e o instalador no Windows
- [ ] Descartar a alteração só de fim de linha em `src-tauri/Cargo.toml` (ou configurar
      `.gitattributes` para evitar CRLF/LF)
