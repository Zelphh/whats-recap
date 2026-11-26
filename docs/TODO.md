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
- [x] **Flash attention por modelo.** No Vulkan/Windows, `-fa on` deixa os Qwen 2–2,5× mais lentos
      no `llama-server` (modo completo 5,7 h → 2,4 h no 4B); o Gemma é o contrário (geração cai de
      32 para 19 tok/s sem FA). Flags por modelo em `ModelSpec::args` (`core-llm/src/models.rs`)

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
- [x] `llama-server` no instalador
  - [x] Em vez de `externalBin` (que só leva o `.exe`), a pasta `src-tauri/llama/` inteira vai como
        recurso: o executável precisa das DLLs ao lado (ggml, Vulkan, variantes de CPU, OpenMP e o
        runtime do Visual C++). `scripts/setup-llama.ps1` prepara a pasta (b11938, 84 MB); o
        conteúdo não vai para o git
  - [x] Iniciar ao abrir o chat (ou na primeira pergunta livre) e encerrar ao fechar o app ou pelo
        botão "Descarregar". Caminhos: `WHATSRECAP_LLAMA_SERVER`, `<recursos>/llama/` ou, em debug,
        o `whatsrecap-evals`; modelos em `<app_data>/models` (`WHATSRECAP_MODELS_DIR`) ou, em debug,
        nos `models/` do evals
  - [x] Flags do Qwen3.5 9B: `-ngl 99 -c 16384 -fa off --reasoning off`, com `-dev` na GPU escolhida
        (ajustar se o teste de VRAM pedir `-c 8192`/`-ctk q8_0`)
  - [x] Fallback: `--list-devices` dá a VRAM livre; tenta o maior modelo baixado que cabe na GPU
        (9B: 6 GB livres; 4B: 3,5 GB), o próximo se a carga falhar e, por último, o menor na CPU
        (`-ngl 0 -c 8192 -dev none`)
  - [ ] Testar o instalador numa máquina sem o Visual C++ Redistributable
  - [x] **A VRAM livre do `--list-devices` não enxergava outros processos** (com o 4B carregado
        por outro processo, continuava em 7.362 MiB). No Windows, `list_devices` desconta o uso
        dedicado de todos os processos pelo contador "GPU Adapter Memory" (PDH, nome em inglês, vale
        em qualquer idioma), ligado à placa pelo LUID do DXGI. Na RX 7600 só com o desktop: 6.234
        MiB livres em vez de 7.362 (o 9B ainda cabe, com ~230 MiB de folga sobre os 6.000).
        `cargo run -p core-llm --example dispositivos` mostra o que o app vê
- [x] Download do modelo pelo app (Hugging Face, 9B de 4,8 GiB ou 4B de 2,6 GiB; progresso,
      pausa e retomada por `Range`, checagem de tamanho e SHA-256 antes de renomear o `.partial`)
- [x] Tela de status do LLM nas configurações (o que está carregado e onde, GPU e VRAM livre,
      baixar, pausar, apagar e descarregar)
- [x] Fluxo do app com o modelo real (conversa sintética de 5 anos, 2026-11-12): plano GPU/CPU,
      carga, aquecimento e perguntas. Por pergunta livre, depois do aquecimento: 9B na GPU ~1 s,
      4B na GPU 0,5–0,9 s, 4B na CPU 1,6–3,8 s (aquecimento de 14 s)
- [ ] Testar na interface: download completo pelo app (4,8 GB), pausa/retomada e o painel

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
  - [x] "de noite ou de dia?": compara as partes do dia citadas ("de dia" = 6h–17h), com a média
        por hora quando as faixas têm tamanhos diferentes (regra fixa e intenção do LLM)
  - [x] Latência: de 3–5 s para 1,2–1,9 s por pergunta livre. Com um slot, o `llama-server`
        trocava o cache do roteador pelo da ferramenta e salvava o slot ocioso no cache em RAM
        (~1 s por chamada); agora `-np 2 --cache-ram 0`
  - [x] Primeira pergunta livre pagava a carga e os dois prompts de sistema: o servidor sobe ao
        abrir o chat e aquece os dois slots com os prompts da conversa (`core_chat::llm_prompts`,
        `Client::warm`). Medido (2026-11-12, conversa sintética de 5 anos): carga 4–10 s,
        aquecimento ~2 s em sequência (em paralelo levava 9 s), primeira pergunta livre de 2,7 s
        para 1,1 s
- [x] Aviso na interface enquanto o modelo carrega na primeira pergunta livre (~9 s)
- [ ] Redação pelo LLM (opcional): manter os templates de `answer.rs` como padrão/fallback
- [x] Ferramenta `list_messages` (filtro exato, sem contagem nem RAG): autor, datas, faixa de horas
      (passa da meia-noite) e palavra opcional. Responde o total por pessoa com as 5 primeiras
      citações; "Ver as N na conversa" abre a lista completa (até 1.000) no painel do visualizador
  - [x] Regras fixas: "quais foram/mostra as mensagens…", "o que ela mandou" + horário, "quantas
        mensagens" + faixa de horas; "das 7h às 8h", "entre 22h e 2h", "de 7 a 9 da noite", "às 7h",
        "antes das 7h"; datas "15/03/2024" e "15 de março de 2024"
  - [x] "Quem é você nesta conversa?" nas configurações (meta `chat_me`; padrão: o autor "Você"):
        "eu" vira você e "ela"/"ele"/"dela"/"dele", a outra pessoa, nas regras fixas e no prompt
        da seleção de ferramenta (sem a escolha, o prompt é o mesmo medido no evals)
  - [x] Schema e prompt da seleção de ferramenta no `llm.rs`; exemplo no roteador; casos t51–t55 no B
  - [x] B no Qwen 9B com a ferramenta nova (2026-11-12): 91,8% (55 itens; era 93,2% com 50). Os
        cinco novos (t51–t55) passam; erra t04, t26, t28 (já conhecidos) e t07 ("quem manda mais
        mensagem de madrugada?" ganhou autor; no app, as regras fixas respondem antes). p50 de
        0,99 s (era 0,62 s; o prompt cresceu)
  - [x] Achados no teste do fluxo:
    - [x] "quem é mais tagarela de nós dois?" → regra fixa de quem manda mais (tagarela, falador,
          faladeira, papudo) e citado no prompt em `messages_by_hour`
    - [x] "mostra o que a Ana mandou no natal do ano passado" → lista: "o que" + verbo de envio +
          data, período ou "mostra", sem "sobre", vai como contagem; com período relativo, o LLM
          preenche as datas (o prompt cita o Natal e o exemplo "o que ele mandou no Natal")
    - [x] O 4B ignorava "no ano passado" em `response_time`: se o modelo devolve sem datas e a
          pergunta tem um único período relativo que o código calcula (este ano, ano passado,
          este mês, mês passado, últimos 12 meses), as datas entram (`llm::relative_range`)
    - [ ] Medir B de novo no evals com o prompt alterado (tagarela, Natal) e checar as três
          perguntas com o 9B e o 4B
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

- [x] Embedder `e5-small-q8` no catálogo (`core_llm::E5_SMALL`, 126 MiB, baixado pelo app com
      SHA-256) num segundo `llama-server` na CPU (`--embedding --pooling mean`), que sobe na primeira
      indexação ou pergunta; `Client::embed` devolve vetores normalizados
- [x] Tabela `chunks` preenchida (`core_chat::rag`): janelas de até 12 mensagens / 1.000
      caracteres, quebradas em pausas de 1 h, sobreposição de 2; mensagens de pessoas com texto ou
      legenda. Vetores `f32` no BLOB `embedding`; `NULL` = falta (indexação retomável, em lotes de
      32). Índice com versão e embedder em `meta.rag_index`: outro embedder ou versão refaz
- [x] Busca só vetorial (cosseno por força bruta sobre os BLOBs, exata; é o que o `sqlite-vec`
      faria, sem a dependência em C). RRF só se o FTS com `AND`/frases ou o `bge-m3` mudarem o
      resultado do evals
- [x] Resposta: top-5 trechos (unidos quando se sobrepõem, até ~7 mil caracteres) no formato do
      evals, prompt e schema do evals (`llm::rag_prompt`); citações conferidas contra o contexto,
      e sem nenhuma válida a resposta vira recusa. Links para o visualizador nas citações
- [x] Interface: cartão no chat para indexar, pausar e continuar (progresso e tempo restante);
      linha do modelo de busca em Configurações; o servidor de chat ganhou um 3º slot (o prompt do
      RAG muda a cada pergunta e não tira do cache o roteador e a seleção de ferramenta)
- [x] Roteador: "quanto custou/pagou/ficou" e "quantos anos" são conteúdo, não contagem
- [x] Teste com os modelos de verdade (2026-11-20, `--example conteudo`): as 20 janelas e 18
      perguntas do `rag.json` do evals injetadas numa conversa sintética de 188.772 mensagens.
      - Indexação na CPU (Ryzen 7 5700X): 18.858 trechos em **264 s** (~1,4 min por 100 mil
        mensagens; o evals estimava ~20 min). Embedder carrega em 1,8 s; busca em ~45 ms
      - 4/4 recusas certas (nada inventado); 7/14 respostas certas com citação certa (Hanami,
        Pipoca, conserto, senha do wifi, endereço, aluguel, Chile). Nenhuma resposta errada
        afirmada: as 7 falhas viraram "não encontrei" ou resposta parcial
      - As falhas são de busca: o sintético repete os mesmos assuntos milhares de vezes
        ("comprei as passagens pra Floripa" 3.370×, "filme" 3.276×, "aniversário" 3.247×) e os
        trechos certos não ficam entre os 5. Com conversas reais o ruído é outro; medir lá
      - "o que o veterinário falou da Pipoca?": achou a otite, mas respondeu `found: false`
      - Resposta em 3,5–10 s por pergunta (9B na GPU; contexto de até ~2 mil tokens sem cache),
        acima dos ~2 s do evals (janelas menores)
- [x] Calibração da busca (2026-11-25): `cargo run --release -p core-llm --example busca --
      conversa-rag.db ../whatsrecap-evals/datasets/rag.json [12,2,1000 8,2,700 ...]` mede
      recall@k e MRR das 14 perguntas respondíveis do `rag.json` na conversa do teste acima, com
      os trechos e o texto do app (e5 na CPU, sem o modelo de chat). MRR / recall@5:

      | trechos (msgs/caracteres/sobreposição) | vetorial | + datas | híbrida (FTS, RRF) |
      |---|---|---|---|
      | 12 / 1.000 / 2 (18.858 trechos) | 0,60 / 9 | 0,62 / 10 | 0,59 / 8 |
      | 8 / 700 / 2 (29.924) | 0,62 / 10 | 0,63 / 10 | 0,59 / 9 |
      | 6 / 500 / 1 (36.275) | 0,58 / 9 | 0,64 / 10 | 0,59 / 9 |

      - Fica: trechos de 12, top-5, só vetorial e **filtro de datas** quando a pergunta cita um
        período ("em julho de 2023", "ano passado"; ±7 dias; sem trecho no período, ignora).
        O filme de julho de 2023 foi do 12º para o 4º lugar
      - A híbrida piora em todas as formas (as palavras comuns da pergunta puxam trechos
        errados), como no evals; fica no código atrás de `rag::HYBRID` para medir com conversa real
      - Janelas menores não acham mais e indexam 1,6–1,9× mais trechos
      - Os acertos depois do 5º só aparecem no 11º–14º (emprego novo, idade da mãe): top-8/10
        não ajuda. Fora dos 50 nas três formas: Floripa e "casar" (o sintético repete esses
        assuntos milhares de vezes)
      - Isso substitui medir o formato novo dos trechos no evals: a medida agora é com o formato
        do app, numa conversa do tamanho real. Não houve comparação direta com o formato antigo
        no mesmo corpus
- [ ] Reordenar pelo LLM os 15–20 primeiros trechos antes de mandar 5 (pegaria o 11º–14º);
      custa uma chamada a mais na GPU. Medir com conversa real antes
- [ ] Medir a busca numa conversa real (anonimizada, fora do git), onde o ruído é outro
- [x] Perguntas de acompanhamento: o frontend manda a última resposta (pergunta, rota, texto e
      ids citados) e, se ela foi sobre o conteúdo e a nova é curta e com cara de continuação
      ("e depois?", "por quê?", "quando foi isso?", "e ela respondeu o quê?"), a nova vai para o
      RAG: busca com as duas perguntas, contexto com as mensagens em volta das citações
      anteriores (mais para depois em "e depois?", mais para antes em "por quê?") e o LLM
      recebe a pergunta e a resposta anteriores. A interface diz "entendida como continuação"
- [ ] Continuações de contagens ("quantas vezes ele falou amor?" → "e ela?") ainda não são
      entendidas
- [x] Continuações e filtro de datas com os modelos de verdade (2026-11-25, `--example conteudo`,
      que agora manda a resposta anterior como o app): "e onde fica?" (rua Augusta), "e o que
      tinha quebrado?" (embreagem), "e depois?" (parcelou em 3x), "por quê?" (a Pipoca, ele, a
      mãe) e "e quanto foi a consulta?" (180), todas com a citação certa, em 2–7 s. O filme de
      julho de 2023 passou a ser achado: 8/14 respostas certas (eram 7), 4/4 recusas certas.
      Corrigido no teste: "depois" no meio da frase marcava continuação ("o aluguel depois da
      negociação") e "quanto foi" ia para contagem
- [ ] O modelo responde `found: false` quando a resposta é indireta ou um "não": "o que o
      veterinário falou da Pipoca?" (diz a otite, mas sem citação) e "a Ana chegou a se mudar pra
      SP?" (diz que ela decidiu não ir, sem citação). Ajustar o prompt do RAG e medir F no evals
- [ ] "e quantas vezes ela falou isso?" conta a palavra "isso" (regra fixa)

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
