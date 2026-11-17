#!/usr/bin/env bash
# Gera as imagens dos enunciados desenhados do Ensino: para cada missão da campanha com
# `cena`, a planta (exemplo `plantas` do utsim-ensino-core) vira imagem e âncoras no Blender
# (tools/blender/enunciado.py), gravadas ao lado da missão em ensino/campanha. Com o Pillow
# instalado (python), a imagem é reduzida a 256 cores, para pesar menos no pacote.
#   bash scripts/enunciados.sh              # todas as cenas
#   bash scripts/enunciados.sh ato4/mix     # só as missões com estes ids
# O Blender vem de $BLENDER, do PATH ou da pasta padrão do Windows.
set -euo pipefail
raiz="$(cd "$(dirname "$0")/.." && pwd)"
b="${BLENDER:-}"
if [ -z "$b" ]; then
  if command -v blender >/dev/null 2>&1; then
    b="$(command -v blender)"
  else
    for c in "/c/Program Files/Blender Foundation"/Blender*/blender.exe "/Applications/Blender.app/Contents/MacOS/Blender"; do
      [ -x "$c" ] && b="$c"
    done
  fi
fi
[ -n "$b" ] || { echo "Blender não encontrado (defina BLENDER)" >&2; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cd "$raiz"
cargo run -q -p utsim-ensino-core --example plantas -- "$tmp" "$@" > "$tmp/lista.txt"
caminho() { command -v cygpath >/dev/null 2>&1 && cygpath -w "$1" || echo "$1"; }
while IFS= read -r planta; do
  planta="${planta//\\//}"
  rel="${planta#"$tmp"/}"
  rel="${rel#"$(caminho "$tmp" | tr '\\' '/')"/}"
  destino="$raiz/ensino/campanha/${rel%.planta.json}"
  saida=$("$b" -b --factory-startup --python-exit-code 1 -P "$raiz/tools/blender/enunciado.py" -- \
    --planta "$(caminho "$planta")" --saida "$(caminho "$destino")" 2>&1) || { echo "$saida" >&2; exit 1; }
  if python -c "import PIL" >/dev/null 2>&1; then
    python - "$(caminho "$destino")" <<'EOF'
import glob, os, sys
from PIL import Image
p = sys.argv[1]
Image.open(p).convert("RGB").quantize(256, method=Image.Quantize.MEDIANCUT).save(p, optimize=True)
# As máquinas de uma grade, com transparência (oficina.cena.serra.png).
for q in glob.glob(glob.escape(os.path.splitext(p)[0]) + ".*.png"):
    Image.open(q).convert("RGBA").quantize(256, method=Image.Quantize.FASTOCTREE).save(q, optimize=True)
EOF
  fi
  echo "enunciado: ensino/campanha/${rel%.planta.json}"
done < "$tmp/lista.txt"
