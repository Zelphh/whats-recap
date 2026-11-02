# TODO

Pendências do WhatsRecap, em ordem de dependência. Fases 0–4 e o chat tipo 1 sem LLM estão
prontos (ver [README](../README.md)); a máquina alvo (Ryzen 7 5700X + RX 7600 8 GB, Windows 11) já
compila o app e passa em `cargo test --workspace`.

## 1. Benchmark de modelos (`../whatsrecap-evals`)

**Modelo escolhido (2026-10-29): Qwen3.5 9B `IQ4_XS`, sem flash attention**; alternativa Qwen3.5 4B
`Q4_K_M`. Números e achados em `../whatsrecap-evals/COMPARACAO-MODELOS.md`.

- [x] Decidir o modelo: Qwen3.5 (foco em perguntas gerais; o Gemma perdeu em RAG, roteador e redação)
- [x] Commitar o suporte a Windows no evals (caminho do `core-chat`, `.exe`, encerramento do
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
- [x] **Usar `without_redundant_range` no caminho do LLM** (fase 8), antes de executar a ferramenta
- [x] **Prompt de B** (2026-11-01): datas relativas calculadas no código e passadas prontas no prompt,
      regras de autor ("eu" nunca é autor; comparações sem autor), granularidades descritas, mídia e
      categorias → `none`, quatro exemplos. B (50 itens, 10 novos) foi de 72,7% para 93,2% no Qwen 9B;
      p50 de 1,08 s para 0,62 s (os exemplos ensinam a omitir datas redundantes). Restam t04 ("eu
      falei 'bruno'" vira autor Ana), t26 ("ela demora" sem autor) e t28 (`week`; resolvido pelas
      regras fixas)
- [ ] **Flash attention por modelo.** No Vulkan/Windows, `-fa on` deixa os Qwen 2–2,5× mais lentos
      no `llama-server` (modo completo 5,7 h → 2,4 h no 4B); o Gemma é o contrário (geração cai de
      32 para 19 tok/s sem FA). O sidecar precisa de flags por modelo.

## 2. Fase 5 — Infraestrutura de LLM no app

- [x] Prompts vencedores (v2) de roteador e ferramenta em `crates/core-chat/src/llm.rs`: exemplos no
      roteador; data de hoje, apelidos e saída "nenhuma ferramenta" na seleção. O `v1` do evals
      passou a ser esse prompt
- [ ] Redação (C): a v2 venceu (`authors` e datas legíveis no `ToolResult`), mas o app ainda usa
      templates; só importa se a redação pelo LLM entrar
- [x] Crate `core-llm`: cliente com `response_format: json_schema` e `llama-server` como processo
      filho (porta livre, espera o `/health`, sem janela de console, encerra no `Drop`)
- [x] Example `ferramenta` e teste `--ignored` do `core-llm` com o Qwen3.5 9B na RX 7600: carga em ~9 s,
      ~2 s por pergunta, ferramentas e datas certas
- [ ] `llama-server` como sidecar do Tauri
  - [ ] Binário em `externalBin` com o sufixo do target (`llama-server-x86_64-pc-windows-msvc.exe`)
  - [ ] DLLs do ggml/Vulkan como recursos do bundle
  - [x] Iniciar sob demanda (primeira pergunta que precisar do LLM) e encerrar ao fechar o app
        (`LlmState` em `src-tauri`; por enquanto acha os arquivos por `WHATSRECAP_LLAMA_SERVER`/
        `WHATSRECAP_MODEL` ou, em debug, na pasta do `whatsrecap-evals`)
  - [ ] Flags do Qwen3.5 9B: `-ngl 99 -c 16384 -fa off --reasoning off` (ajustar se o teste de
        VRAM pedir `-c 8192`/`-ctk q8_0`)
  - [ ] Fallback para o 4B (ou CPU) se a GPU não tiver VRAM livre suficiente
- [ ] Download do modelo pelo app na primeira vez (`unsloth/Qwen3.5-9B-GGUF`,
      `Qwen3.5-9B-IQ4_XS.gguf`, 4,8 GiB; progresso, retomada, checagem de tamanho/hash); o GGUF não
      entra no instalador
- [ ] Tela de status do LLM nas configurações (modelo, baixado ou não, GPU detectada)

## 3. Fase 8 — Chat tipo 1 com LLM

- [x] Roteador: casos `Undecided` da heurística vão para o LLM (`router_schema`)
- [x] Seleção de ferramenta pelo LLM quando a heurística não entende a pergunta (`Interpreter::Llm`,
      "interpretado pelo modelo" no "como calculei")
- [x] Resposta "fora do escopo" quando nenhuma ferramenta serve (`{"tool":"none"}`)
- [x] Testar no app e com o example `chat` do `core-llm` (fluxo completo numa conversa sintética).
      Achados (2026-10-31), somam-se ao item de prompt de B:
  - [x] "de semana ou no fim de semana?" → `weekday` (regra fixa nova)
  - [x] "quantas figurinhas ela mandou?" → `none`
  - [x] "época do ano" → `month`; "verão passado" e "ano passado" com o intervalo certo
  - [x] Regras fixas não respondem mais períodos relativos ("ano passado", "esse mês", "nas
        férias") sobre a conversa inteira: deixam para o LLM
  - [ ] "de noite ou de dia?": a resposta só traz a noite; o template precisa comparar as duas
        partes do dia quando a pergunta tem "X ou Y"
  - [x] Latência: de 3–5 s para 1,2–1,9 s por pergunta livre. Com um slot, o `llama-server`
        trocava o cache do roteador pelo da ferramenta e salvava o slot ocioso no cache em RAM
        (~1 s por chamada); agora `-np 2 --cache-ram 0`
  - [ ] Primeira pergunta livre ainda leva ~6 s além da carga (processa os dois prompts de
        sistema): aquecer os dois slots logo após subir o servidor
- [x] Aviso na interface enquanto o modelo carrega na primeira pergunta livre (~9 s)
- [ ] Redação pelo LLM (opcional): manter os templates de `answer.rs` como padrão/fallback
- [ ] Ferramenta `list_messages` (filtro exato, sem contagem nem RAG): autor, faixa de horas, datas
      e palavra opcional. Responde o total com algumas citações e um link para a lista completa no
      visualizador. Ex.: "quais foram as mensagens da Ana entre 7h e 8h da manhã?"
  - [ ] Roteador: "quais foram as mensagens de X …" com horário/data/autor vai para o tipo 1, não
        para conteúdo (hoje cai no quebra-galho de `retrieval_answer`)
  - [ ] Schema e prompt da seleção de ferramenta no `llm.rs` + caso no dataset B do evals
- [x] "eu"/"eu disse" na pergunta apontar para o autor "Você" (export do iOS/Android do próprio
      dono), sem confundir com o "eu" da palavra procurada ("eu te amo")

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

- [x] Início e fim do resumo ignoram mensagens de sistema (avisos meses antes da primeira mensagem
      faziam a tela inicial e o chat discordarem do dashboard); migração v3 corrige bancos antigos
- [ ] Validar o parser com exports reais (Android e iOS, com e sem mídia), anonimizados com
      `cargo run -p synth --bin anonymize`; nada sai da máquina nem vai para o git
- [ ] README: pré-requisitos e caminhos do Windows (WebView2, `%APPDATA%\dev.whatsrecap.app`)
- [ ] Atualizar `CONTEXTO-WHATSRECAP.md` do evals (máquina alvo tem 32 GB de RAM e roda Windows)
- [ ] Testar `pnpm tauri build` e o instalador no Windows
- [ ] Descartar a alteração só de fim de linha em `src-tauri/Cargo.toml` (ou configurar
      `.gitattributes` para evitar CRLF/LF)
