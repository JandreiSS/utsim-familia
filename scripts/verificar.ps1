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

Passo '1/6 Formatação (cargo fmt --check)'
cargo fmt --all -- --check
if ($LASTEXITCODE -ne 0) { Falha 'formatação fora do padrão. Rode: cargo fmt --all' }

Passo '2/6 Lints (cargo clippy -D warnings)'
cargo clippy --workspace --exclude utsim-viewer --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { Falha 'clippy encontrou avisos ou erros' }

Passo '3/6 Testes (cargo test --workspace --exclude utsim-viewer)'
cargo test --workspace --exclude utsim-viewer
if ($LASTEXITCODE -ne 0) { Falha 'testes falharam' }

Passo '4/6 Build de release (utsim-cli)'
cargo build --release -p utsim-cli
if ($LASTEXITCODE -ne 0) { Falha 'build de release falhou' }

Passo '5/6 Validação dos modelos (utsim check)'
$bin = Join-Path (Get-Location) 'target\release\utsim.exe'
if (-not (Test-Path $bin)) { Falha "binário não encontrado: $bin" }
foreach ($modelo in Get-ChildItem -Path 'examples' -Filter '*.ron') {
    if ($modelo.Name -like 'experimento*' -or $modelo.Name -like 'otimizacao*') { continue }
    & $bin check $modelo.FullName
    if ($LASTEXITCODE -ne 0) { Falha "modelo inválido: $($modelo.Name)" }
}

Passo '6/6 Visualizador 3D (opcional: UTSIM_VIEWER=1)'
if ($env:UTSIM_VIEWER -eq '1') {
    cargo clippy -p utsim-viewer --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { Falha 'clippy do utsim-viewer encontrou avisos ou erros' }
    cargo test -p utsim-viewer
    if ($LASTEXITCODE -ne 0) { Falha 'testes do utsim-viewer falharam' }
} else {
    Write-Host '(pulado; defina UTSIM_VIEWER=1 para incluir o utsim-viewer)'
}

Write-Host ''
Write-Host 'Tudo certo' -ForegroundColor Green
