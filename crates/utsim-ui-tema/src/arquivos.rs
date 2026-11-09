//! Arquivos e pastas: o seletor nativo do sistema (no Windows, o explorador de arquivos) e o
//! campo de caminho com o botão "Procurar…", igual em todos os apps da família.
//!
//! O seletor nativo para o quadro enquanto está aberto (é uma janela modal do sistema); o
//! campo de texto continua lá, para quem prefere colar o caminho.

use std::path::{Path, PathBuf};

use egui::{Response, Ui};

/// O que o seletor escolhe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    /// Um arquivo que já existe, para abrir ou importar.
    Abrir,
    /// Um arquivo para gravar (pode ainda não existir).
    Salvar,
    /// Uma pasta.
    Pasta,
}

/// Um filtro de tipos de arquivo do seletor: o nome mostrado e as extensões (sem ponto).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Filtro {
    pub nome: &'static str,
    pub extensoes: &'static [&'static str],
}

/// Modelos do UTSim.
pub const RON: Filtro = Filtro {
    nome: "Modelo do UTSim (.ron)",
    extensoes: &["ron"],
};
/// Tabelas em texto.
pub const CSV: Filtro = Filtro {
    nome: "Tabela CSV (.csv, .txt)",
    extensoes: &["csv", "txt", "tsv"],
};
/// Planilhas e tabelas em texto.
pub const PLANILHA: Filtro = Filtro {
    nome: "Planilha (.csv, .txt, .xlsx)",
    extensoes: &["csv", "txt", "tsv", "xlsx", "xlsm"],
};
/// Relatórios em PDF.
pub const PDF: Filtro = Filtro {
    nome: "PDF (.pdf)",
    extensoes: &["pdf"],
};
/// Páginas HTML.
pub const HTML: Filtro = Filtro {
    nome: "Página (.html)",
    extensoes: &["html", "htm"],
};

/// Onde o seletor começa: a pasta do caminho já escrito (ou a pasta pai, se for um arquivo
/// ou ainda não existir), senão nenhuma (o sistema escolhe).
pub fn pasta_inicial(atual: &str) -> Option<PathBuf> {
    let t = atual.trim().trim_matches('"');
    if t.is_empty() {
        return None;
    }
    let mut p = PathBuf::from(t);
    loop {
        if p.is_dir() {
            return Some(p);
        }
        if !p.pop() || p.as_os_str().is_empty() {
            return None;
        }
    }
}

/// O nome do arquivo já escrito, para o seletor de gravar sugerir.
fn nome_inicial(atual: &str) -> Option<String> {
    let p = Path::new(atual.trim().trim_matches('"'));
    if p.is_dir() {
        return None;
    }
    p.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// Abre o seletor nativo e devolve o que a pessoa escolheu (`None` se cancelou).
/// `atual` é o caminho já escrito, de onde o seletor parte.
#[cfg(feature = "arquivos")]
pub fn procurar(tipo: Tipo, titulo: &str, filtros: &[Filtro], atual: &str) -> Option<PathBuf> {
    let mut d = rfd::FileDialog::new().set_title(titulo);
    if let Some(p) = pasta_inicial(atual) {
        d = d.set_directory(p);
    }
    if tipo != Tipo::Pasta {
        for f in filtros {
            d = d.add_filter(f.nome, f.extensoes);
        }
        if !filtros.is_empty() {
            d = d.add_filter("Todos os arquivos", &["*"]);
        }
    }
    match tipo {
        Tipo::Abrir => d.pick_file(),
        Tipo::Salvar => {
            if let Some(n) = nome_inicial(atual) {
                d = d.set_file_name(n);
            }
            let p = d.save_file()?;
            // Acrescenta a extensão do primeiro filtro quando a pessoa não escreveu nenhuma.
            match (p.extension(), filtros.first()) {
                (None, Some(f)) if !f.extensoes.is_empty() => {
                    Some(p.with_extension(f.extensoes[0]))
                }
                _ => Some(p),
            }
        }
        Tipo::Pasta => d.pick_folder(),
    }
}

/// O rótulo do botão (com reticências: abre uma janela).
pub const PROCURAR: &str = "Procurar…";

/// O campo de caminho: a caixa de texto e, ao lado, o botão "Procurar…" que abre o seletor
/// nativo. Devolve a resposta da caixa; `changed()` também vale quando o seletor escolheu.
#[cfg(feature = "arquivos")]
pub fn campo(
    ui: &mut Ui,
    texto: &mut String,
    tipo: Tipo,
    titulo: &str,
    filtros: &[Filtro],
    largura: f32,
) -> Response {
    ui.horizontal(|ui| {
        let dica = match tipo {
            Tipo::Pasta => "pasta: escreva ou clique em Procurar…",
            Tipo::Abrir | Tipo::Salvar => "arquivo: escreva ou clique em Procurar…",
        };
        let mut r = ui.add(
            egui::TextEdit::singleline(texto)
                .hint_text(dica)
                .desired_width(largura),
        );
        if ui
            .button(PROCURAR)
            .on_hover_text("Abre o explorador de arquivos para escolher.")
            .clicked()
            && let Some(p) = procurar(tipo, titulo, filtros, texto)
        {
            *texto = p.display().to_string();
            r.mark_changed();
        }
        r
    })
    .inner
}

/// Mostra a pasta (ou o arquivo, selecionado) no explorador de arquivos do sistema.
pub fn mostrar_no_explorador(caminho: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let mut c = std::process::Command::new("explorer");
        if caminho.is_file() {
            c.arg(format!("/select,{}", caminho.display()));
        } else {
            c.arg(caminho);
        }
        c.spawn().map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(caminho)
            .spawn()
            .map(|_| ())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let alvo = if caminho.is_file() {
            caminho.parent().unwrap_or(caminho)
        } else {
            caminho
        };
        std::process::Command::new("xdg-open")
            .arg(alvo)
            .spawn()
            .map(|_| ())
    }
}

/// A pasta "Documentos" da pessoa (ponto de partida sugerido para pastas novas).
pub fn documentos() -> Option<PathBuf> {
    let casa = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    let casa = PathBuf::from(casa);
    for nome in ["Documents", "Documentos"] {
        let d = casa.join(nome);
        if d.is_dir() {
            return Some(d);
        }
    }
    Some(casa)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasta_de_partida() {
        let d = std::env::temp_dir();
        assert_eq!(pasta_inicial(&d.display().to_string()), Some(d.clone()));
        let f = d.join("nao_existe_utsim").join("x.ron");
        assert_eq!(pasta_inicial(&f.display().to_string()), Some(d.clone()));
        assert_eq!(pasta_inicial("   "), None);
        assert_eq!(
            nome_inicial("C:/x/modelo.ron").as_deref(),
            Some("modelo.ron")
        );
    }
}
