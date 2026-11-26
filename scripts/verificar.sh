#!/usr/bin/env bash
# Verificação completa deste repositório: formatação, lints, testes, build de release e modelos.
# Para no primeiro passo que falhar.

set -u

cd "$(dirname "$0")/.." || exit 1
REPO=$(basename "$(git rev-parse --show-toplevel 2>/dev/null || pwd)")

TRAVA=/c/Users/Jandrei/AppData/Local/utsim-trava/trava.sh
TRAVA_DONO=/c/Users/Jandrei/AppData/Local/Temp/utsim-verificar.lock/dono
# Quem chamou já tem a trava quando o token dele (exportado por trava.sh) é o dono dela.
if [ -f "$TRAVA" ] && [ "$(cat "$TRAVA_DONO" 2>/dev/null)" != "${UTSIM_TRAVA_TOKEN:-sem-token}" ]; then
    # shellcheck disable=SC1090
    source "$TRAVA" "$REPO:verificar"
fi


# Verificação é de uma vez só: os dados incrementais não servem para nada e ocupam disco.
export CARGO_INCREMENTAL=0
# Cache de compilação compartilhado entre os worktrees (sccache), quando instalado.
if command -v sccache >/dev/null 2>&1; then
    export RUSTC_WRAPPER=sccache
    export SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-25G}"
fi

# Com UTSIM_GODOT=1, a ponte utsim-godot entra no clippy e nos testes do workspace: uma
# chamada separada recompilaria várias dependências com outro conjunto de features.
FORA=""

falha() {
    echo "FALHOU: $1" >&2
    exit 1
}

passo() {
    echo
    echo "==> $1"
}

passo "1/3 Formatação (cargo fmt --check)"
# Pacote por pacote: com o workspace grande, `cargo fmt --all` passa da linha de comando
# máxima do Windows (os error 206).
for toml in crates/*/Cargo.toml; do
    # Crates fora do workspace (o `exclude` do Cargo.toml da raiz) ficam de fora.
    grep -q "\"$(dirname "$toml")\"" <(grep '^exclude' Cargo.toml) && continue
    pacote=$(awk -F'"' '/^name = /{print $2; exit}' "$toml")
    cargo fmt -p "$pacote" -- --check || falha "formatação fora do padrão em $pacote. Rode: cargo fmt -p $pacote"
done

passo "2/3 Lints (cargo clippy -D warnings)"
# shellcheck disable=SC2086
cargo clippy --workspace $FORA --all-targets -- -D warnings || falha "clippy encontrou avisos ou erros"

# shellcheck disable=SC2086
if cargo nextest --version >/dev/null 2>&1; then
    passo "3/3 Testes (cargo nextest run --workspace $FORA, em paralelo, e os doctests)"
    # --retries 1: um teste que só passa na segunda vez aparece como FLAKY no relatório.
    cargo nextest run --workspace $FORA --retries 1 --no-fail-fast || falha "testes falharam"
    cargo test --doc --workspace $FORA || falha "doctests falharam"
else
    passo "3/3 Testes (cargo test --workspace $FORA)"
    cargo test --workspace $FORA || falha "testes falharam"
fi

echo
echo "Tudo certo"
