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

passo "1/7 Formatação (cargo fmt --check)"
# Pacote por pacote: com o workspace grande, `cargo fmt --all` passa da linha de comando
# máxima do Windows (os error 206).
for toml in crates/*/Cargo.toml; do
    # Crates fora do workspace (o `exclude` do Cargo.toml da raiz) ficam de fora.
    grep -q "\"$(dirname "$toml")\"" <(grep '^exclude' Cargo.toml) && continue
    pacote=$(awk -F'"' '/^name = /{print $2; exit}' "$toml")
    cargo fmt -p "$pacote" -- --check || falha "formatação fora do padrão em $pacote. Rode: cargo fmt -p $pacote"
done

passo "2/7 Lints (cargo clippy -D warnings)"
cargo clippy --workspace --exclude utsim-viewer --exclude utsim-godot --all-targets -- -D warnings || falha "clippy encontrou avisos ou erros"

passo "3/7 Testes (cargo test --workspace --exclude utsim-viewer --exclude utsim-godot)"
cargo test --workspace --exclude utsim-viewer --exclude utsim-godot || falha "testes falharam"

passo "4/7 Build de release (utsim-cli)"
cargo build --release -p utsim-cli || falha "build de release falhou"

passo "5/7 Validação dos modelos (utsim check)"
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

passo "6/7 Visualizador 3D (opcional: UTSIM_VIEWER=1)"
if [ "${UTSIM_VIEWER:-}" = "1" ]; then
    cargo clippy -p utsim-viewer --all-targets -- -D warnings || falha "clippy do utsim-viewer encontrou avisos ou erros"
    cargo test -p utsim-viewer || falha "testes do utsim-viewer falharam"
else
    echo "(pulado; defina UTSIM_VIEWER=1 para incluir o utsim-viewer)"
fi

passo "7/7 Interface Godot (opcional: UTSIM_GODOT=1)"
if [ "${UTSIM_GODOT:-}" = "1" ]; then
    cargo clippy -p utsim-godot --all-targets -- -D warnings || falha "clippy do utsim-godot encontrou avisos ou erros"
    cargo test -p utsim-godot || falha "testes do utsim-godot falharam"
    # O teste sem janela precisa do Godot 4.7 (GODOT, `godot` no PATH ou ~/Documents/Godot_v4.7*).
    bash scripts/godot_teste.sh || falha "teste sem janela do Godot falhou"
else
    echo "(pulado; defina UTSIM_GODOT=1 para incluir a ponte utsim-godot e o teste no Godot)"
fi

echo
echo "Tudo certo"
