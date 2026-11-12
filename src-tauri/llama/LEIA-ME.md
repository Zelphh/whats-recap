# llama-server do instalador

Esta pasta entra no instalador como recurso (`bundle.resources` em `tauri.conf.json`) e o app roda
`<recursos>/llama/llama-server.exe` para o chat. O conteúdo não vai para o git; para preparar:

```powershell
scripts/setup-llama.ps1            # baixa a build b11938 (Vulkan, Windows x64)
scripts/setup-llama.ps1 -From ..\whatsrecap-evals\vendor\llama.cpp\current   # copia de uma pasta local
```

Sem a pasta preparada, o app em desenvolvimento usa o `llama-server` do `whatsrecap-evals`; o
instalador sai sem o chat com LLM (só as regras fixas).
