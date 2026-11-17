//! Writes the plan of the scene of every mission of the built-in campaign that has one
//! (`cena`), for `tools/blender/enunciado.py` (run by `scripts/enunciados.sh`):
//! `<saida>/<folder of the mission>/<picture>.planta.json`.
//!
//!     cargo run -p utsim-ensino-core --example plantas -- <saida> [id da missão...]

use std::path::PathBuf;

use utsim_ensino_core::campanha::arquivos;
use utsim_ensino_core::missao::Missao;
use utsim_ensino_core::pacote::manifesto;
use utsim_model::Model;

fn main() {
    let mut args = std::env::args().skip(1);
    let saida = PathBuf::from(args.next().expect("uso: plantas <saida> [id...]"));
    let so: Vec<String> = args.collect();
    let arq = arquivos();
    let texto = |p: &str| String::from_utf8(arq[p].clone()).expect("utf-8");
    let pacote = manifesto(&arq).expect("pacote.ron");
    let mut n = 0;
    // Missions on the same line share the picture (the same plan): it is written once.
    let mut feitas = std::collections::BTreeSet::new();
    for ato in &pacote.atos {
        for path in &ato.missoes {
            let m = Missao::parse(&texto(path)).expect(path);
            let Some(cena) = &m.cena else { continue };
            if !so.is_empty() && !so.contains(&m.id) {
                continue;
            }
            let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
            let modelo_path = utsim_ensino_core::pacote::junta(dir, &m.modelo);
            let modelo = Model::parse(&texto(&modelo_path)).expect(&modelo_path);
            let planta = cena.planta(&modelo);
            let destino = saida.join(dir).join(format!("{}.planta.json", cena.imagem));
            if !feitas.insert(destino.clone()) {
                continue;
            }
            std::fs::create_dir_all(destino.parent().expect("pasta")).expect("pasta");
            std::fs::write(
                &destino,
                serde_json::to_string_pretty(&planta).expect("json"),
            )
            .expect("gravar");
            println!("{}", destino.display());
            n += 1;
        }
    }
    eprintln!("plantas: {n} cena(s)");
}
