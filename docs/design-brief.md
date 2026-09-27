# WhatsRecap — brief de design do frontend

Este documento descreve o app para quem vai desenhar a interface. Ele cobre o produto, as telas,
os dados que cada tela recebe, os estados possíveis e as restrições técnicas. **O backend já
existe e está pronto**: a interface precisa se encaixar nos dados abaixo, sem pedir campos novos.

---

## 1. O produto

**WhatsRecap** é um app desktop que analisa o export de uma conversa do WhatsApp **entre duas
pessoas** (um casal, dois amigos) e mostra estatísticas sobre ela: quem manda mais mensagens, quem
responde mais rápido, palavras e emojis favoritos, horários, figurinhas, áudios etc.

- **100% local e privado.** Nenhuma mensagem sai do computador. Isso é o principal argumento do
  produto e deve transparecer na interface (tom de confiança, sem nada que pareça "enviar para a nuvem").
- **Público:** pessoas curiosas sobre as próprias conversas. Não são analistas: os números precisam
  ser legíveis, com frases em linguagem natural ("Ana responde mais rápido").
- **Tom:** leve, afetivo e um pouco divertido (é sobre relações), mas limpo e confiável. Não é um
  painel corporativo. Também é projeto de **portfólio**: screenshots bonitos importam.
- **Idioma:** toda a interface é em **português do Brasil**, com números no formato pt-BR
  (`188.663`, `2,7`, `90,1%`) e datas `dd/mm/aaaa`.

### Premissas que afetam a interface

| Premissa | Consequência no design |
|---|---|
| Dois modos: **sem mídia** (`.txt`) e **com mídia** (`.zip`) | Cards de figurinhas e áudios só funcionam com mídia; no modo sem mídia aparecem desabilitados, com uma dica de como exportar com mídia |
| Conversas de **anos** (até centenas de milhares de mensagens) | Séries temporais longas (1.800+ dias), listas virtualizadas, importação com barra de progresso |
| Tudo é **clicável até a mensagem original** | Mensagem mais longa, resultados de busca e (no futuro) eventos detectados pelo LLM abrem o ponto exato da conversa |

---

## 2. Plataforma e restrições técnicas

- **Tauri v2**: janela desktop com WebView (WebKitGTK no Linux, WebView2 no Windows). Não é um site.
- **React 19 + TypeScript + Vite.** Gráficos em **ECharts**; lista da conversa em **react-virtuoso**.
  Pode propor outra biblioteca de gráficos, desde que aguente séries diárias de vários anos.
- **Janela:** padrão de 1280×860, **mínimo de 900×600**. Não precisa de layout mobile, mas o layout
  deve se adaptar entre ~900 e ~1920 px de largura.
- **Modo claro e escuro** seguindo o sistema (`prefers-color-scheme`). Os dois precisam estar desenhados.
- **Sem fontes ou recursos externos em runtime** (o app funciona offline; a CSP bloqueia CDNs).
  Fontes customizadas precisam ser empacotadas no app; a fonte atual é a do sistema (`system-ui`).
- **Arrastar e soltar arquivos** é tratado pelo Tauri (o evento traz o caminho do arquivo).
- **Diálogos nativos** (escolher arquivo, confirmar exclusão) vêm do sistema operacional, não do HTML.

---

## 3. Navegação

Hoje há uma **sidebar fixa à esquerda** com:

1. **Importar / histórico** (sempre visível)
2. Nome da conversa aberta ("Ana & Bruno") como título de seção, e abaixo dele:
   - **Dashboard**
   - **Conversa** (visualizador)
   - **Configurações**
   - **Análises LLM** (desabilitado, "em breve")
   - **Chat** (desabilitado, "em breve")

Os itens da conversa só aparecem depois que uma conversa é aberta. A estrutura pode ser repensada
(abas, sidebar recolhível etc.), mas as telas e a hierarquia "app → conversa → telas da conversa"
continuam valendo.

---

## 4. Telas

### 4.1 Importar / histórico (tela inicial)

**Objetivo:** trazer uma conversa nova ou reabrir uma já importada.

- **Área de soltar arquivo**: "Arraste aqui o arquivo exportado do WhatsApp", com a explicação
  `.txt` = sem mídia · `.zip` = com figurinhas e áudios, e o botão **Escolher arquivo**.
  Estado de hover ao arrastar um arquivo por cima.
- **Mensagem de privacidade**: "Tudo é processado nesta máquina: nenhuma mensagem sai do seu computador."
- **Durante a importação** a área de soltar vira um painel de progresso:
  - fase atual: `Detectando formato…` → `Lendo mensagens…` → `Criando índices…` →
    `Processando figurinhas e áudios…` (só com mídia) → `Calculando estatísticas…`;
  - barra de progresso única (as fases são fatias da mesma barra, então ela nunca volta);
  - contador: "12.345 mensagens" (ou "1.200 arquivos de mídia" na fase de mídia);
  - botão **Cancelar**.
  Importações reais levam de menos de 1 s a alguns segundos; a fase de mídia pode demorar mais.
- **Erros** (mostrar em destaque, com texto do backend):
  - "Conversas em grupo não são suportadas (autores encontrados: Ana, Bruno, Carla)"
  - "Nenhuma linha no formato de export do WhatsApp foi reconhecida"
  - "Importação cancelada."
- **Lista de conversas importadas**: cada item mostra os nomes ("Ana & Bruno"), a quantidade de
  mensagens, o período, a plataforma (Android/iOS), o modo (com/sem mídia) e, se houver, "N linhas não
  reconhecidas". Ação **Remover** (com confirmação). Estado vazio: instruções de como exportar no
  WhatsApp ("abra a conversa → ⋮ → Mais → Exportar conversa").

### 4.2 Dashboard (tela principal)

É a tela mais importante e a que vai para o portfólio. Recebe um único objeto `Stats` (seção 5).
Blocos atuais, na ordem, que podem ser reorganizados:

1. **Cabeçalho:** "Ana & Bruno", período ("01/01/2023 a 30/12/2027"), plataforma e selo "Com mídia"/"Sem mídia".
2. **Números principais** (4 cards):
   - Mensagens: `188.663`, com a nota "1.788 conversas (sessões)";
   - Palavras: `485.393`;
   - Palavras por mensagem: `2,7`, com a nota "média nas mensagens com texto";
   - Dias conversando: `1.644`, com a nota "de 1.825 dias no período (90,1%)".
3. **Quem manda mais mensagens:** frase ("Ana enviou 51% das mensagens"), barra de proporção entre as
   duas pessoas e tabela por pessoa (mensagens, palavras, palavras/msg).
4. **Mensagem mais longa:** autor, data e hora, palavras e caracteres, início do texto e o link
   **Abrir na conversa**.
5. **Mensagens por mês / por dia** (alternância): duas séries, uma por pessoa. A série diária pode
   ter **1.800+ pontos** e precisa de zoom/rolagem horizontal; dias sem mensagem valem 0 e aparecem
   como buracos.
6. **Dia da semana** (alternância "Média por dia" / "Total"): barras agrupadas por pessoa (Seg…Dom),
   com a frase "Mais ativo: domingo (116,6 msgs em média) · menos ativo: terça (93,9)".
   Inclui o **gráfico em texto para copiar e compartilhar**, com botão **Copiar**:
   ```
   Seg  ███████████████████████
   Ter  ██████████████████████
   Qua  ██████████████████████
   Qui  ███████████████████████
   Sex  ████████████████████████
   Sáb  █████████████████████████
   Dom  █████████████████████████
   ```
   (fonte monoespaçada, até 25 blocos; é um recurso "compartilhável" e merece destaque visual).
7. **Horário mais ativo:** histograma de 24 barras (alternância "Por pessoa" / "Juntos"),
   "Pico às 23h", e uma opção "Ver como tabela".
8. **Tempo de resposta:** frase "Ana responde mais rápido (mediana de 1min 16s)"; tabela por pessoa
   com **mediana**, **média** e número de respostas; distribuição em faixas por pessoa
   (`< 1 min`, `1–5 min`, `5–30 min`, `30 min–6h`, em %); nota explicando que média muito acima da
   mediana indica respostas lentas pontuais.
9. **Palavras mais usadas** e **Emojis mais usados:** top 5 com alternância Geral / Ana / Bruno,
   cada item com posição, termo, barra proporcional e contagem.
10. **Palavras de cada pessoa** (um card por pessoa, duas listas cada):
    - "Só Ana usa": palavras que a outra pessoa nunca usou, com a contagem ("aff 4.572×");
    - "Ana usa muito mais": palavras que as duas usam, com a razão ("mano 12× mais").
      Tooltip com as contagens das duas pessoas.
11. **Mídias enviadas** por pessoa. Com mídia (ou export do iOS): tabela por tipo (Figurinhas,
    Áudios, Fotos e vídeos, Documentos, Apagadas). Sem mídia do Android: só total + apagadas, com a
    nota "não dá para separar áudio, foto e figurinha".
12. **Figurinhas mais usadas** *(requer mídia)*: top 5 com **miniatura da figurinha**
    (PNG de até 128×128, geralmente com fundo transparente), contagem, alternância Geral / Ana / Bruno,
    nota "4 versões do arquivo agrupadas" quando aplicável e o rodapé "40 figurinhas distintas".
13. **Áudios** *(requer mídia)*: "Ana é quem mais fala em áudios" e, por pessoa,
    "Ana enviou 1.425 áudios (18h 37min no total, média de 47s)" com uma barra comparando a duração total.

**Sem mídia:** os cards 12 e 13 aparecem **desabilitados**, com o selo "requer mídia" e a dica
"Exporte a conversa com mídia (.zip) para ver…".

**Ideias bem-vindas:** hierarquia mais forte (um "resumo" no topo em frases), destaques do tipo
"curiosidades", cards que funcionem bem como screenshot. Os dados disponíveis estão todos na seção 5.

### 4.3 Conversa (visualizador)

A conversa inteira, como um chat, em **lista virtualizada** (pode ter 200 mil mensagens; as mensagens
chegam em páginas de 200 conforme a rolagem, e uma mensagem ainda não carregada aparece como placeholder).

- **Balões:** pessoa 1 à esquerda, pessoa 2 à direita, nome na cor da pessoa, texto, hora e `#id`.
- **Separador de dia** ("domingo, 1 de janeiro de 2023") quando o dia muda.
- **Mensagens de sistema** centralizadas e discretas (ex.: aviso de criptografia).
- **Tipos especiais:** figurinha (mostra a miniatura, 96 px), "Áudio", "Imagem · IMG-…jpg",
  "Vídeo", "Documento", "Mídia oculta", "Mensagem apagada".
- **Barra superior:** busca de texto (os resultados abrem num painel lateral com `#id · autor · data`
  e trecho; clicar rola até a mensagem), **Ir para data** (seletor de data limitado ao período) e
  **Ir para nº da mensagem**.
- **Mensagem-alvo** (quando se chega por um link): destacada e centralizada na tela.

### 4.4 Configurações da análise

Formulário simples; ao salvar, as estatísticas são recalculadas (leva de menos de 1 s a alguns segundos).

- Intervalo que inicia uma nova conversa, em horas (padrão 6). Afeta o tempo de resposta.
- Frequência mínima para "palavras exclusivas" (padrão 5).
- Ignorar acentos (checkbox) e agrupar tons de pele dos emojis (checkbox).
- Tolerância para agrupar figurinhas, de 0 a 16 (padrão 4). **Só aparece com mídia.**
- Lista de palavras ignoradas (stopwords), editável, uma por linha, com a dica de adicionar "kk" para
  tirar risadas do ranking.
- Botões **Salvar e recalcular** e **Restaurar padrão**, e uma mensagem de status.

### 4.5 Telas futuras (desenhar só a estrutura, marcadas como "em breve")

- **Análises LLM:** botão para iniciar a análise ("modo rápido" ou "modo completo"), progresso
  (chunks processados / total, tempo estimado, pausar/cancelar/retomar) e a lista de eventos detectados:
  - **pedidos de desculpa** (quem pediu para quem, sincero ou cortesia);
  - **brigas** (intervalo de mensagens, intensidade de 1 a 5, resolvida ou não, resumo de uma frase).
  Cada evento abre o trecho no visualizador. Totais em cards ("12 brigas em 2024").
- **Chat:** perguntas sobre a conversa ("quando a gente combinou a viagem pra Floripa?"), com respostas
  que **citam mensagens** (`#1234`) como links clicáveis para o visualizador.
  Perguntas globais ("como evoluiu a relação?") recebem uma resposta explicando que ainda não são suportadas.

---

## 5. Dados disponíveis

Todos os nomes de campos abaixo são exatamente os que o backend envia (JSON, camelCase).
Os valores de exemplo vêm de uma conversa sintética de 5 anos.

### Conversa importada (lista do histórico)

```ts
{
  id: "c1758850000000",
  summary: {
    sourceName: "Conversa do WhatsApp com Bruno.zip",
    hasMedia: true,
    platform: "android",         // ou "ios"
    authors: ["Ana", "Bruno"],   // índice 0 = pessoa 1, índice 1 = pessoa 2
    messageCount: 194949,
    firstTs: 1672563600, lastTs: 1830124800,  // segundos; exibir em UTC (é o horário local do export)
    sessionCount: 1802,
    sessionGapSecs: 21600,
    importedAt: 1758850000,
    orphanLines: 0               // linhas não reconhecidas
  }
}
```

### Estatísticas (dashboard)

```ts
{
  authors: ["Ana", "Bruno"],
  hasMedia: true,
  totals: { messages: 188663, words: 485393, textMessages: 170000, avgWordsPerMessage: 2.7,
            stickers: 2837, audios: 2900, images: 5918, videos: 0, docs: 0, mediaHidden: 0, deleted: 975 },
  perAuthor: [ /* mesmo formato de totals, um por pessoa */ ],
  longestMessage: { id: 1234, author: 0, ts: 1789146480, words: 18, chars: 80, preview: "o que acha de ir no show?…" },
  days: { firstDate: "2023-01-01", lastDate: "2027-12-30", activeDays: 1644, spanDays: 1825, activePct: 90.1 },
  daily:   { start: "2023-01-01", values: [[/* 1825 números */], [/* … */]] },  // values[pessoa][dia]
  monthly: { start: "2023-01",    values: [[/* 60 números */],   [/* … */]] },
  weekday: { totals: [[/* 7 */], [/* 7 */]], occurrences: [261,…], avgPerOccurrence: [96.5,…],
             mostActive: 6, leastActive: 1 },   // 0 = segunda … 6 = domingo
  weekdayChartText: "Seg  ███…\nTer  ███…",
  hourly: [[/* 24 */], [/* 24 */]],
  topWords: [{ item: "gente", count: 7874 }, …],        // 10 itens (mostrar 5)
  topWordsByAuthor: [[…], […]],
  topEmojis: [{ item: "🥰", count: 5837 }, …],
  topEmojisByAuthor: [[…], […]],
  exclusiveWords: [
    { onlyYou:  [{ word: "aff", count: 4572, otherCount: 0, ratio: null, zScore: 60.1 }, …],   // até 15
      muchMore: [{ word: "gente", count: 7000, otherCount: 1900, ratio: 3.6, zScore: 45.2 }, …] },
    { /* pessoa 2 */ }
  ],
  responseTimes: [
    { count: 51110, meanSecs: 278, medianSecs: 76, buckets: [23, 45, 14, 3] },  // buckets em contagem
    { count: 51125, meanSecs: 410, medianSecs: 121, buckets: [ … ] }
  ],
  fastestResponder: 0,            // ou null
  media: {                        // null no modo sem mídia
    topStickers: [{ file: "STK-20230102-WA0004.webp", count: 341, variants: 4 }, …],
    topStickersByAuthor: [[…], […]],
    distinctStickers: 40,
    missingStickers: 0,
    audio: [ { count: 1425, withDuration: 1425, totalMs: 67020000, avgMs: 47000 }, { … } ]
  }
}
```

- A miniatura de uma figurinha é pedida pelo nome do arquivo (`file`) e chega como PNG.
- Qualquer lista pode vir **vazia** (ex.: ninguém usou emoji; nenhuma palavra exclusiva).
- `fastestResponder` e `longestMessage` podem ser `null`.
- Os valores variam muito: de conversas de uma semana (centenas de mensagens) a anos (centenas de milhares).
  Números grandes precisam caber nos cards.

### Mensagem (visualizador)

```ts
{ id: 173, ts: 1672628700, author: "Ana" /* null = sistema */,
  kind: "text" | "sticker" | "audio" | "image" | "video" | "doc" | "deleted" | "system" | "media_hidden",
  text: "olha isso" /* ou null */, mediaFile: "STK-….webp" /* ou null */, sessionId: 12 }
```

Os ids são sequenciais a partir de 1 (a mensagem `#173` é a 173ª da conversa).

---

## 6. Linguagem visual atual (ponto de partida, não obrigação)

O design atual é funcional e neutro. Pode ser substituído, **mantendo as regras de dados** abaixo.

**Regras que devem continuar valendo:**
- **Cada pessoa tem uma cor fixa** em todo o app (gráficos, legendas, nomes nos balões, barras).
  A cor segue a pessoa, nunca a posição no ranking. As duas cores precisam ser distinguíveis também
  por daltônicos e funcionar nos dois modos.
- Dados "gerais" (sem pessoa) usam uma cor neutra, não a cor de uma das pessoas.
- Todo gráfico com as duas pessoas tem legenda. Textos e números ficam em cores de texto, não na cor da série.
- Gráficos com tooltip no hover. Grade e eixos discretos.
- Nunca dois eixos Y no mesmo gráfico.
- Contraste adequado nos dois modos; foco visível para navegação por teclado.

**Tokens atuais** (claro / escuro):

| Papel | Claro | Escuro |
|---|---|---|
| Fundo da página | `#f9f9f7` | `#0d0d0d` |
| Superfície (cards) | `#fcfcfb` | `#1a1a19` |
| Superfície secundária | `#f0efec` | `#262624` |
| Texto | `#0b0b0b` | `#ffffff` |
| Texto secundário | `#52514e` | `#c3c2b7` |
| Texto apagado / eixos | `#898781` | `#898781` |
| Grade | `#e1e0d9` | `#2c2c2a` |
| Borda | `rgba(11,11,11,.10)` | `rgba(255,255,255,.10)` |
| Destaque (marca) | `#0f766e` (verde-azulado) | `#2dd4bf` |
| Pessoa 1 | `#2a78d6` (azul) | `#3987e5` |
| Pessoa 2 | `#eb6834` (laranja) | `#d95926` |
| Balão pessoa 1 / 2 | `#e3eefb` / `#fde8df` | `#1c2c42` / `#3a2419` |
| Erro | `#d03b3b` | `#e66767` |

**Ícone do app:** um balão de conversa claro com três barras de gráfico, sobre um quadrado arredondado
verde-azulado (`#0f766e`).

---

## 7. Entregável esperado

- Telas desenhadas nos **modos claro e escuro**: Importar/histórico (vazio, com itens, importando, erro),
  Dashboard (com mídia e sem mídia), Conversa (com busca aberta e mensagem-alvo destacada),
  Configurações e as telas futuras "em breve".
- Componentes reutilizáveis: card de número, card de ranking com alternância Geral/pessoa, legenda das
  duas pessoas, alternância segmentada, barra de proporção, balão de mensagem, estado vazio, estado
  desabilitado "requer mídia", barra de progresso.
- A implementação é em **React + TypeScript**. Estilos em CSS com variáveis (tokens) são preferíveis
  a frameworks pesados; o código atual usa um único `styles.css` com tokens em `:root`.
