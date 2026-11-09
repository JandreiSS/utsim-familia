"""Gera os SVG dos ícones da família UTSim (estilo plano) e da logo oficial (engrenagens).

Estilo da família: quadrado arredondado (raio 48 em 256), degradê vertical claro em cima e
escuro embaixo na cor do app, e um símbolo branco grande. A logo oficial repete as três
engrenagens metálicas da abertura do launcher sobre o fundo quase preto.

Uso (na raiz do repositório):
    python tools/icone/gerar_svgs.py
    cargo run --manifest-path tools/icone/Cargo.toml --release
"""

import math
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[2]
ICONE = RAIZ / "assets" / "icone"
FONTE = ICONE / "familia" / "fonte"

TAU = math.tau


def fundo(topo, base, extra=""):
    return f"""<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">
  <defs>
    <linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{topo}"/>
      <stop offset="1" stop-color="{base}"/>
    </linearGradient>{extra}
  </defs>
  <rect width="256" height="256" rx="48" fill="url(#g)"/>
"""


def pts(lista):
    return " ".join(f"{x:.2f},{y:.2f}" for x, y in lista)


def contorno_engrenagem(cx, cy, raio, dentes, angulo):
    """O mesmo contorno de `gear_outline` em crates/utsim-launcher/src/abertura.rs."""
    interno = raio * 0.84
    passo = TAU / dentes
    saida = []
    for k in range(dentes):
        a = angulo + k * passo
        for frac, r in ((0.0, interno), (0.18, raio), (0.42, raio), (0.6, interno)):
            b = a + frac * passo
            saida.append((cx + math.cos(b) * r, cy + math.sin(b) * r))
    return saida


def tres_engrenagens(unidade, centro):
    """Posições como em `pintar` da abertura: grande à esquerda, duas menores à direita."""
    cx, cy = centro
    grande, media, pequena = unidade, unidade * 0.72, unidade * 0.62
    g = (cx - unidade * 0.42, cy + unidade * 0.12)

    def junto(r, ang):
        d = grande * 0.93 + r * 0.93
        return (g[0] + math.cos(ang) * d, g[1] + math.sin(ang) * d)

    return [
        (junto(pequena, 0.62), pequena, 10, 0.3),
        (junto(media, -0.55), media, 12, 0.12),
        (g, grande, 16, 0.0),
    ]


def engrenagem_metal(c, r, dentes, ang, fundo_cor):
    x, y = c
    teia = r * 0.66
    janelas = "".join(
        f'<circle cx="{x + math.cos(ang + k * TAU / 5 + TAU / 10) * teia * 0.58:.2f}" '
        f'cy="{y + math.sin(ang + k * TAU / 5 + TAU / 10) * teia * 0.58:.2f}" '
        f'r="{teia * 0.3:.2f}" fill="{fundo_cor}"/>'
        for k in range(5)
    )
    return (
        f'  <polygon points="{pts(contorno_engrenagem(x, y, r, dentes, ang))}" '
        f'fill="url(#metal)" stroke="#282828" stroke-width="{max(r * 0.03, 1.0):.2f}" '
        f'stroke-linejoin="round"/>\n'
        f'  <circle cx="{x:.2f}" cy="{y:.2f}" r="{teia:.2f}" fill="#464646"/>\n'
        f"  {janelas}\n"
        f'  <circle cx="{x:.2f}" cy="{y:.2f}" r="{teia:.2f}" fill="none" stroke="#c8c8c8" '
        f'stroke-width="{max(r * 0.035, 1.0):.2f}"/>\n'
        f'  <circle cx="{x:.2f}" cy="{y:.2f}" r="{r * 0.24:.2f}" fill="#e1e1e1" '
        f'stroke="#282828" stroke-width="{max(r * 0.025, 1.0):.2f}"/>\n'
        f'  <circle cx="{x:.2f}" cy="{y:.2f}" r="{r * 0.1:.2f}" fill="#3c3c3c"/>\n'
    )


def engrenagem_plana(c, r, dentes, ang, cor_furo, cor_contorno):
    x, y = c
    teia = r * 0.66
    janelas = "".join(
        f'<circle cx="{x + math.cos(ang + k * TAU / 5 + TAU / 10) * teia * 0.58:.2f}" '
        f'cy="{y + math.sin(ang + k * TAU / 5 + TAU / 10) * teia * 0.58:.2f}" '
        f'r="{teia * 0.27:.2f}" fill="{cor_furo}"/>'
        for k in range(5)
    )
    return (
        f'  <polygon points="{pts(contorno_engrenagem(x, y, r, dentes, ang))}" fill="#ffffff" '
        f'stroke="{cor_contorno}" stroke-width="8" stroke-linejoin="round"/>\n'
        f'  <polygon points="{pts(contorno_engrenagem(x, y, r, dentes, ang))}" fill="#ffffff"/>\n'
        f"  {janelas}\n"
        f'  <circle cx="{x:.2f}" cy="{y:.2f}" r="{r * 0.1:.2f}" fill="{cor_furo}"/>\n'
    )


def logo_oficial():
    """Fundo quase preto da abertura e as engrenagens metálicas."""
    extra = """
    <linearGradient id="metal" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#f0f0f0"/>
      <stop offset="0.5" stop-color="#c3c3c3"/>
      <stop offset="1" stop-color="#8c8c8c"/>
    </linearGradient>"""
    svg = fundo("#24262b", "#0b0b0c", extra).replace("<svg ", "<svg ", 1)
    svg = svg.replace("<defs>", "<title>UTSim</title>\n  <defs>", 1)
    for c, r, d, a in tres_engrenagens(58, (126, 124)):
        svg += engrenagem_metal(c, r, d, a, "#141518")
    return svg + "</svg>\n"


def launcher():
    svg = fundo("#3a3e46", "#121316")
    for c, r, d, a in tres_engrenagens(56, (124, 126)):
        svg += engrenagem_plana(c, r, d, a, "#2a2d33", "#22252a")
    return svg + "</svg>\n"


def utsim():
    """Editor e simulador: uma rede de nós ligados (o modelo) com peças fluindo."""
    svg = fundo("#2cc0ff", "#0a4f99")
    nos = [(64, 176), (128, 84), (192, 176)]
    svg += (
        '  <g stroke="#ffffff" stroke-width="16" stroke-linecap="round" fill="none">\n'
        f"    <path d=\"M{nos[0][0]} {nos[0][1]} L{nos[1][0]} {nos[1][1]} L{nos[2][0]} {nos[2][1]}\"/>\n"
        f"    <path d=\"M{nos[0][0]} {nos[0][1]} L{nos[2][0]} {nos[2][1]}\"/>\n"
        "  </g>\n"
    )
    svg += f'  <rect x="{nos[1][0] - 30}" y="{nos[1][1] - 30}" width="60" height="60" rx="12" fill="#ffffff"/>\n'
    for x, y in (nos[0], nos[2]):
        svg += f'  <circle cx="{x}" cy="{y}" r="30" fill="#ffffff"/>\n'
        svg += f'  <circle cx="{x}" cy="{y}" r="12" fill="#1478c8"/>\n'
    svg += f'  <rect x="{nos[1][0] - 12}" y="{nos[1][1] - 12}" width="24" height="24" rx="4" fill="#1478c8"/>\n'
    return svg + "</svg>\n"


def seis_sigma():
    """Lean Seis Sigma: um sigma minúsculo."""
    svg = fundo("#62dc7c", "#137a3a")
    svg += '  <circle cx="112" cy="150" r="52" fill="none" stroke="#ffffff" stroke-width="28"/>\n'
    svg += '  <rect x="108" y="70" width="104" height="30" rx="15" fill="#ffffff"/>\n'
    return svg + "</svg>\n"


def ensino():
    """Ensino: capelo de formatura."""
    svg = fundo("#a47cf0", "#4a2a9e")
    svg += '  <polygon points="128,60 232,106 128,152 24,106" fill="#ffffff" stroke="#ffffff" stroke-width="8" stroke-linejoin="round"/>\n'
    svg += '  <path d="M72 128 L72 172 Q128 206 184 172 L184 128 L128 154 Z" fill="#ffffff" stroke="#ffffff" stroke-width="6" stroke-linejoin="round"/>\n'
    svg += '  <path d="M208 116 L208 176" stroke="#ffffff" stroke-width="10" stroke-linecap="round"/>\n'
    svg += '  <circle cx="208" cy="186" r="13" fill="#ffffff"/>\n'
    return svg + "</svg>\n"


def estatistica():
    """Estatística: curva normal preenchida com a linha da média."""
    svg = fundo("#2fd6bf", "#0b6e64")
    base, topo, meio, largura = 194, 66, 128, 104
    curva = []
    for k in range(81):
        x = meio - largura + 2 * largura * k / 80
        z = (x - meio) / (largura / 2.9)
        curva.append((x, base - (base - topo) * math.exp(-z * z / 2)))
    caminho = f"M{curva[0][0]:.2f} {base} " + " ".join(f"L{x:.2f} {y:.2f}" for x, y in curva)
    caminho += f" L{curva[-1][0]:.2f} {base} Z"
    svg += f'  <path d="{caminho}" fill="#ffffff"/>\n'
    svg += f'  <rect x="{meio - 5}" y="{topo + 18}" width="10" height="{base - topo - 18}" rx="3" fill="#0f8a7c"/>\n'
    svg += f'  <rect x="20" y="{base}" width="216" height="14" rx="7" fill="#ffffff"/>\n'
    return svg + "</svg>\n"


def main():
    FONTE.mkdir(parents=True, exist_ok=True)
    (ICONE / "utsim.svg").write_text(logo_oficial(), encoding="utf-8", newline="\n")
    for nome, func in (
        ("launcher", launcher),
        ("utsim", utsim),
        ("lean-seis-sigma", seis_sigma),
        ("ensino", ensino),
        ("estatistica", estatistica),
    ):
        (FONTE / f"{nome}.svg").write_text(func(), encoding="utf-8", newline="\n")
    print(f"SVG gerados em {ICONE}")


if __name__ == "__main__":
    main()
