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

passo "1/5 Formatação (cargo fmt --check)"
cargo fmt --all -- --check || falha "formatação fora do padrão. Rode: cargo fmt --all"

passo "2/5 Lints (cargo clippy -D warnings)"
cargo clippy --workspace --all-targets -- -D warnings || falha "clippy encontrou avisos ou erros"

passo "3/5 Testes (cargo test --workspace)"
cargo test --workspace || falha "testes falharam"

passo "4/5 Build de release (utsim-cli)"
cargo build --release -p utsim-cli || falha "build de release falhou"

passo "5/5 Validação dos modelos (utsim check)"
BIN="target/release/utsim"
if [ ! -x "$BIN" ]; then
    BIN="target/release/utsim.exe"
fi
[ -x "$BIN" ] || falha "binário utsim não encontrado em target/release"
for modelo in examples/*.ron; do
    case "$(basename "$modelo")" in
        experimento*) continue ;;
    esac
    "$BIN" check "$modelo" || falha "modelo inválido: $modelo"
done

echo
echo "Tudo certo"
