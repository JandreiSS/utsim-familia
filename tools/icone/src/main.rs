//! Gera os ícones a partir dos SVG:
//! - a logo oficial: `assets/icone/utsim-<tamanho>.png` e `assets/icone/utsim.ico`, de
//!   `assets/icone/utsim.svg`;
//! - os ícones da família: `assets/icone/familia/<app>_256.png`, `<app>_64.png` e `<app>.ico`,
//!   de cada `assets/icone/familia/fonte/<app>.svg`.
//!
//! Os SVG vêm de `tools/icone/gerar_svgs.py`.
//!
//! Uso (na raiz do repositório): `cargo run --manifest-path tools/icone/Cargo.toml --release`

use std::path::Path;

use resvg::{tiny_skia, usvg};

const TAMANHOS_PNG: [u32; 7] = [16, 32, 48, 64, 128, 256, 512];
const TAMANHOS_FAMILIA: [u32; 2] = [64, 256];
const TAMANHOS_ICO: [u32; 6] = [16, 32, 48, 64, 128, 256];

fn desenhar(arvore: &usvg::Tree, lado: u32) -> tiny_skia::Pixmap {
    let mut pixmap = tiny_skia::Pixmap::new(lado, lado).expect("tamanho válido");
    let escala = lado as f32 / arvore.size().width();
    resvg::render(
        arvore,
        tiny_skia::Transform::from_scale(escala, escala),
        &mut pixmap.as_mut(),
    );
    pixmap
}

fn ler(svg: &Path) -> Result<usvg::Tree, Box<dyn std::error::Error>> {
    let dados = std::fs::read(svg)?;
    Ok(usvg::Tree::from_data(&dados, &usvg::Options::default())?)
}

fn gravar_ico(arvore: &usvg::Tree, destino: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut icones = ico::IconDir::new(ico::ResourceType::Icon);
    for lado in TAMANHOS_ICO {
        let pixmap = desenhar(arvore, lado);
        // O tiny-skia guarda alfa pré-multiplicado; o .ico espera RGBA direto.
        let mut rgba = Vec::with_capacity(pixmap.data().len());
        for p in pixmap.pixels() {
            let c = p.demultiply();
            rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        let imagem = ico::IconImage::from_rgba_data(lado, lado, rgba);
        icones.add_entry(ico::IconDirEntry::encode(&imagem)?);
    }
    icones.write(std::fs::File::create(destino)?)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pasta = Path::new("assets/icone");
    let logo = ler(&pasta.join("utsim.svg"))?;
    for lado in TAMANHOS_PNG {
        desenhar(&logo, lado).save_png(pasta.join(format!("utsim-{lado}.png")))?;
    }
    gravar_ico(&logo, &pasta.join("utsim.ico"))?;

    let familia = pasta.join("familia");
    let mut fontes: Vec<_> = std::fs::read_dir(familia.join("fonte"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "svg"))
        .collect();
    fontes.sort();
    for svg in fontes {
        let nome = svg
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("nome inválido")?;
        let arvore = ler(&svg)?;
        for lado in TAMANHOS_FAMILIA {
            desenhar(&arvore, lado).save_png(familia.join(format!("{nome}_{lado}.png")))?;
        }
        gravar_ico(&arvore, &familia.join(format!("{nome}.ico")))?;
        println!("  {nome}");
    }
    println!("Ícones gerados em {}", pasta.display());
    Ok(())
}
