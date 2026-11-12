# Prepara src-tauri/llama/ com o llama-server (llama.cpp, Vulkan, Windows x64) e as DLLs de que ele
# precisa. A pasta entra no instalador como recurso (tauri.conf.json > bundle.resources) e o app
# roda <recursos>/llama/llama-server.exe. Os modelos não entram: o app baixa na primeira vez.
#
# Uso: scripts/setup-llama.ps1 [-Tag b11938] [-From <pasta com um llama.cpp já extraído>]
# A tag padrão é a medida no whatsrecap-evals; mudar de versão pede rodar o evals de novo.
param(
    [string]$Tag = "b11938",
    [string]$From = ""
)
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
$dest = "src-tauri/llama"

if (-not $From) {
    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) "whatsrecap-llama-$Tag"
    if (-not (Test-Path "$tmp/llama-server.exe")) {
        New-Item -ItemType Directory -Force $tmp | Out-Null
        $url = "https://github.com/ggml-org/llama.cpp/releases/download/$Tag/llama-$Tag-bin-win-vulkan-x64.zip"
        $zip = "$tmp.zip"
        Write-Host "baixando $url"
        curl.exe -fL -o $zip $url
        if ($LASTEXITCODE -ne 0) { throw "download falhou ($url)" }
        Expand-Archive -Force $zip $tmp
        Remove-Item $zip
        $inner = Get-ChildItem -Recurse -Filter llama-server.exe $tmp | Select-Object -First 1
        if ($inner -and $inner.DirectoryName -ne (Resolve-Path $tmp).Path) {
            Get-ChildItem $inner.DirectoryName | Move-Item -Destination $tmp
        }
    }
    $From = $tmp
}
if (-not (Test-Path "$From/llama-server.exe")) { throw "llama-server.exe não está em $From" }

# Só o servidor e o que ele carrega: llama-server-impl, llama, ggml (base, Vulkan e as variantes de
# CPU, escolhidas em tempo de execução pelo processador) e o OpenMP. As ferramentas ficam de fora.
$files = @(
    "llama-server.exe", "llama-server-impl.dll", "llama-common.dll", "llama.dll", "mtmd.dll",
    "ggml.dll", "ggml-base.dll", "ggml-vulkan.dll", "libomp.dll", "LICENSE-LLVM-OpenMP"
)
Get-ChildItem "$dest/*" -Exclude "LEIA-ME.md" -ErrorAction SilentlyContinue | Remove-Item -Force
New-Item -ItemType Directory -Force $dest | Out-Null
foreach ($f in $files) { Copy-Item "$From/$f" $dest }
Copy-Item "$From/ggml-cpu-*.dll" $dest

# Runtime do Visual C++ (MSVCP140, VCRUNTIME140, VCRUNTIME140_1), redistribuível ao lado do
# executável: sem ele, o llama-server não abre numa máquina sem o pacote do Visual C++.
foreach ($f in "msvcp140.dll", "vcruntime140.dll", "vcruntime140_1.dll") {
    $src = Join-Path $env:SystemRoot "System32/$f"
    if (-not (Test-Path $src)) { throw "$f não encontrado em System32 (instale o Visual C++ Redistributable x64)" }
    Copy-Item $src $dest
}

curl.exe -fsSL -o "$dest/LICENSE-llama.cpp" "https://raw.githubusercontent.com/ggml-org/llama.cpp/$Tag/LICENSE"
Set-Content -Encoding ascii "$dest/TAG" $Tag
# O llama-server escreve a versão no stderr, que o PowerShell 5.1 trata como erro.
$ErrorActionPreference = "Continue"
cmd /c "`"$dest\llama-server.exe`" --version 2>&1" | Select-Object -Last 2
$ErrorActionPreference = "Stop"
$mb = [math]::Round(((Get-ChildItem $dest | Measure-Object Length -Sum).Sum) / 1MB)
Write-Host "ok: $dest ($Tag, $mb MB)"
