# Fixtures do parser

Cada `.txt` aqui é um export do WhatsApp; o `.expected.json` ao lado é o snapshot do que o parser
extrai dele (formato, autores, contagem por tipo e as primeiras mensagens). O teste
`tests/fixtures.rs` compara os dois.

## Arquivos atuais

Os arquivos abaixo foram **escritos à mão** a partir dos formatos conhecidos. Ainda falta validar
com exports reais:

| Arquivo | Cobre |
|---|---|
| `android-ptbr-sem-midia.txt` | Android pt-BR sem mídia: sistema, multilinha, mídia oculta, apagadas, editada, mudança de número |
| `android-ptbr-com-midia.txt` | Android pt-BR com mídia: figurinha, PTT, imagem com legenda, vídeo, documento, áudio encaminhado |
| `ios-ptbr.txt` | iOS pt-BR: `U+200E`, tipos de mídia ocultos, documento, chamada perdida, anexos |
| `android-en-us-12h.txt` | Android en-US: data M/D/AA, AM/PM com espaço estreito (`U+202F`) |

## Adicionando um export real

1. Exporte a conversa no WhatsApp (sem mídia gera `.txt`, com mídia gera `.zip`).
2. Anonimize (aceita `.txt` ou `.zip`; a saída é sempre `.txt`):
   ```sh
   cargo run -p synth --bin anonymize -- "Conversa do WhatsApp com Fulano.txt" \
     -o crates/core-parser/tests/fixtures/android-real-ptbr.txt
   ```
3. **Revise o arquivo gerado.** O anonimizador troca nomes, palavras, números, URLs e nomes de
   documentos, mas mantém mensagens de sistema legíveis (com nomes e números trocados).
4. Se o export for grande, recorte um trecho representativo (algumas centenas de linhas bastam).
5. Rode `cargo test -p core-parser`. Na primeira vez o teste falha e gera o `.expected.json`.
   Confira se o snapshot está certo (autores, tipos, datas) e rode de novo.

Quando o parser mudar de propósito, regenere todos os snapshots com
`UPDATE_FIXTURES=1 cargo test -p core-parser` e revise o diff.
