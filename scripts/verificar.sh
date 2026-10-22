#!/usr/bin/env bash
# Verificação completa do UTSim: formatação, lints, testes, build de release e modelos.
# Para no primeiro passo que falhar.

set -u

cd "$(dirname "$0")/.." || exit 1

falha() {
    echo "FALHOU: $1" >&2
    exit 1
}

passo() {
    echo
    echo "==> $1"
}

passo "1/6 Formatação (cargo fmt --check)"
cargo fmt --all -- --check || falha "formatação fora do padrão. Rode: cargo fmt --all"

passo "2/6 Lints (cargo clippy -D warnings)"
cargo clippy --workspace --exclude utsim-viewer --all-targets -- -D warnings || falha "clippy encontrou avisos ou erros"

passo "3/6 Testes (cargo test --workspace --exclude utsim-viewer)"
cargo test --workspace --exclude utsim-viewer || falha "testes falharam"

passo "4/6 Build de release (utsim-cli)"
cargo build --release -p utsim-cli || falha "build de release falhou"

passo "5/6 Validação dos modelos (utsim check)"
BIN="target/release/utsim"
if [ ! -x "$BIN" ]; then
    BIN="target/release/utsim.exe"
fi
[ -x "$BIN" ] || falha "binário utsim não encontrado em target/release"
for modelo in examples/*.ron; do
    case "$(basename "$modelo")" in
        experimento*|otimizacao*) continue ;;
    esac
    "$BIN" check "$modelo" || falha "modelo inválido: $modelo"
done

passo "6/6 Visualizador 3D (opcional: UTSIM_VIEWER=1)"
if [ "${UTSIM_VIEWER:-}" = "1" ]; then
    cargo clippy -p utsim-viewer --all-targets -- -D warnings || falha "clippy do utsim-viewer encontrou avisos ou erros"
    cargo test -p utsim-viewer || falha "testes do utsim-viewer falharam"
else
    echo "(pulado; defina UTSIM_VIEWER=1 para incluir o utsim-viewer)"
fi

echo
echo "Tudo certo"
