//! Gera `assets/icone/utsim-<tamanho>.png` e `assets/icone/utsim.ico` a partir do SVG.
//!
//! Uso (na raiz do repositório): `cargo run --manifest-path tools/icone/Cargo.toml --release`

use std::path::Path;

use resvg::{tiny_skia, usvg};

const TAMANHOS_PNG: [u32; 7] = [16, 32, 48, 64, 128, 256, 512];
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pasta = Path::new("assets/icone");
    let svg = std::fs::read(pasta.join("utsim.svg"))?;
    let arvore = usvg::Tree::from_data(&svg, &usvg::Options::default())?;

    for lado in TAMANHOS_PNG {
        desenhar(&arvore, lado).save_png(pasta.join(format!("utsim-{lado}.png")))?;
    }

    let mut icones = ico::IconDir::new(ico::ResourceType::Icon);
    for lado in TAMANHOS_ICO {
        let pixmap = desenhar(&arvore, lado);
        // O tiny-skia guarda alfa pré-multiplicado; o .ico espera RGBA direto.
        let mut rgba = Vec::with_capacity(pixmap.data().len());
        for p in pixmap.pixels() {
            let c = p.demultiply();
            rgba.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        let imagem = ico::IconImage::from_rgba_data(lado, lado, rgba);
        icones.add_entry(ico::IconDirEntry::encode(&imagem)?);
    }
    icones.write(std::fs::File::create(pasta.join("utsim.ico"))?)?;
    println!("Ícones gerados em {}", pasta.display());
    Ok(())
}
