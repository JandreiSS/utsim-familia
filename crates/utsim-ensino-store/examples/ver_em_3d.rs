//! "Ver em 3D" without the window: writes the model and the replay of a mission with the
//! reference decisions (the same function as the button) and prints the client and the
//! arguments the button would use. With `--abrir`, opens the installed client.
//!
//! `cargo run -p utsim-ensino-store --example ver_em_3d -- ato1/furadeira <pasta> [--abrir]`

use std::path::Path;

use utsim_ensino_store::tres_d::{abrir_em_3d, argumentos, cliente, gravar_tentativa};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(id), Some(dir)) = (args.first(), args.get(1)) else {
        eprintln!("uso: ver_em_3d <missao> <pasta> [--abrir]");
        std::process::exit(2);
    };
    let p = utsim_ensino_core::campanha::carregar_campanha().expect("campanha");
    let m = p.missao(id).expect("missão");
    let d = m.missao.completar(&m.missao.referencia);
    let dir = Path::new(dir);
    if args.iter().any(|a| a == "--abrir") {
        match abrir_em_3d(dir, m, "ra042", &d) {
            Ok(r) => println!("aberto: {}", r.display()),
            Err(e) => println!("erro: {e}"),
        }
        return;
    }
    let (pm, pr) = gravar_tentativa(dir, m, "ra042", &d).expect("gravar");
    match cliente() {
        Some((exe, padrao, pasta)) => {
            println!("cliente: {}", exe.display());
            println!("pasta: {}", pasta.display());
            for a in argumentos(&exe, &padrao, &pm, &pr) {
                println!("arg: {a}");
            }
        }
        None => println!("o UTSim não está instalado pelo launcher"),
    }
}
