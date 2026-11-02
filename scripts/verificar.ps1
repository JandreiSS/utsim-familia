# Verificação completa do UTSim (compatível com Windows PowerShell 5.1): formatação,
# lints, testes, build de release e validação dos modelos. Para no primeiro passo que falhar.

Set-Location (Split-Path -Parent $PSScriptRoot)

function Falha([string]$mensagem) {
    Write-Host "FALHOU: $mensagem" -ForegroundColor Red
    exit 1
}

function Passo([string]$titulo) {
    Write-Host ''
    Write-Host "==> $titulo"
}

Passo '1/7 Formatação (cargo fmt --check)'
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { Falha 'formatação fora do padrão. Rode: cargo fmt --all' }

Passo '2/7 Lints (cargo clippy -D warnings)'
cargo clippy --workspace --exclude utsim-viewer --exclude utsim-godot --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { Falha 'clippy encontrou avisos ou erros' }

Passo '3/7 Testes (cargo test --workspace --exclude utsim-viewer --exclude utsim-godot)'
cargo test --workspace --exclude utsim-viewer --exclude utsim-godot
if ($LASTEXITCODE -ne 0) { Falha 'testes falharam' }

Passo '4/7 Build de release (utsim-cli)'
cargo build --release -p utsim-cli
if ($LASTEXITCODE -ne 0) { Falha 'build de release falhou' }

Passo '5/7 Validação dos modelos (utsim check)'
$bin = Join-Path (Get-Location) 'target\release\utsim.exe'
if (-not (Test-Path $bin)) { Falha "binário não encontrado: $bin" }
foreach ($modelo in Get-ChildItem -Path 'examples' -Filter '*.ron') {
    if ($modelo.Name -like 'experimento*' -or $modelo.Name -like 'otimizacao*') { continue }
    & $bin check $modelo.FullName
    if ($LASTEXITCODE -ne 0) { Falha "modelo inválido: $($modelo.Name)" }
}

Passo '6/7 Visualizador 3D (opcional: UTSIM_VIEWER=1)'
if ($env:UTSIM_VIEWER -eq '1') {
    cargo clippy -p utsim-viewer --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { Falha 'clippy do utsim-viewer encontrou avisos ou erros' }
    cargo test -p utsim-viewer
    if ($LASTEXITCODE -ne 0) { Falha 'testes do utsim-viewer falharam' }
} else {
    Write-Host '(pulado; defina UTSIM_VIEWER=1 para incluir o utsim-viewer)'
}

Passo '7/7 Interface Godot (opcional: UTSIM_GODOT=1)'
if ($env:UTSIM_GODOT -eq '1') {
    cargo clippy -p utsim-godot --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { Falha 'clippy do utsim-godot encontrou avisos ou erros' }
    cargo test -p utsim-godot
    if ($LASTEXITCODE -ne 0) { Falha 'testes do utsim-godot falharam' }
    # O teste sem janela precisa do Godot 4.7 e do bash (Git Bash).
    bash scripts/godot_teste.sh
    if ($LASTEXITCODE -ne 0) { Falha 'teste sem janela do Godot falhou' }
} else {
    Write-Host '(pulado; defina UTSIM_GODOT=1 para incluir a ponte utsim-godot e o teste no Godot)'
}

Write-Host ''
Write-Host 'Tudo certo' -ForegroundColor Green
