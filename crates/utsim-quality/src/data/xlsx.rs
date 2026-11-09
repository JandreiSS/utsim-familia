//! Reading a sheet of an `.xlsx` workbook into a [`Table`] (stage F1 of the studies in
//! `docs/plano_integracao_familia.md`). The zip is opened with the `zip` crate and the XML
//! is read by hand: shared strings, inline strings, numbers, booleans; formulas give their
//! cached value and errors stay empty. The first row is the header when it has any text.

use std::collections::BTreeMap;
use std::io::{Cursor, Read};

use super::{Table, number};

/// Uma marca de XML: nome, atributos, se fecha (`</x>`) ou se é vazia (`<x/>`).
#[derive(Debug)]
struct Marca {
    nome: String,
    atributos: Vec<(String, String)>,
    fecha: bool,
    vazia: bool,
}

impl Marca {
    fn atributo(&self, nome: &str) -> Option<&str> {
        self.atributos
            .iter()
            .find(|(k, _)| k == nome || k.rsplit(':').next() == Some(nome))
            .map(|(_, v)| v.as_str())
    }

    /// O nome sem o prefixo do espaço de nomes (`x:c` vira `c`).
    fn local(&self) -> &str {
        self.nome.rsplit(':').next().unwrap_or(&self.nome)
    }
}

#[derive(Debug)]
enum Parte {
    Marca(Marca),
    Texto(String),
}

fn desescapar(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut resto = s;
    while let Some(i) = resto.find('&') {
        out.push_str(&resto[..i]);
        resto = &resto[i..];
        let Some(fim) = resto.find(';') else {
            out.push_str(resto);
            return out;
        };
        let ent = &resto[1..fim];
        let c = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ if ent.starts_with("#x") || ent.starts_with("#X") => {
                u32::from_str_radix(&ent[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if ent.starts_with('#') => ent[1..].parse().ok().and_then(char::from_u32),
            _ => None,
        };
        match c {
            Some(c) => {
                out.push(c);
                resto = &resto[fim + 1..];
            }
            None => {
                out.push('&');
                resto = &resto[1..];
            }
        }
    }
    out.push_str(resto);
    out
}

/// Divide o XML em marcas e textos (sem validar; comentários, declarações e CDATA à parte).
fn partes(xml: &str) -> Vec<Parte> {
    let mut v = Vec::new();
    let mut resto = xml;
    while !resto.is_empty() {
        if let Some(r) = resto.strip_prefix("<![CDATA[") {
            let fim = r.find("]]>").unwrap_or(r.len());
            v.push(Parte::Texto(r[..fim].to_string()));
            resto = r.get(fim + 3..).unwrap_or("");
        } else if resto.starts_with("<!--") {
            let fim = resto.find("-->").map_or(resto.len(), |i| i + 3);
            resto = &resto[fim..];
        } else if resto.starts_with("<?") || resto.starts_with("<!") {
            let fim = resto.find('>').map_or(resto.len(), |i| i + 1);
            resto = &resto[fim..];
        } else if let Some(r) = resto.strip_prefix('<') {
            let fim = r.find('>').unwrap_or(r.len());
            let dentro = &r[..fim];
            resto = r.get(fim + 1..).unwrap_or("");
            let fecha = dentro.starts_with('/');
            let vazia = dentro.ends_with('/');
            let dentro = dentro.trim_start_matches('/').trim_end_matches('/');
            let nome_fim = dentro
                .find(|c: char| c.is_whitespace())
                .unwrap_or(dentro.len());
            let nome = dentro[..nome_fim].to_string();
            let mut atributos = Vec::new();
            let mut a = &dentro[nome_fim..];
            while let Some(eq) = a.find('=') {
                let k = a[..eq].trim().to_string();
                let r = a[eq + 1..].trim_start();
                let Some(aspa) = r.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                    break;
                };
                let r = &r[1..];
                let fim = r.find(aspa).unwrap_or(r.len());
                atributos.push((k, desescapar(&r[..fim])));
                a = r.get(fim + 1..).unwrap_or("");
            }
            v.push(Parte::Marca(Marca {
                nome,
                atributos,
                fecha,
                vazia,
            }));
        } else {
            let fim = resto.find('<').unwrap_or(resto.len());
            v.push(Parte::Texto(desescapar(&resto[..fim])));
            resto = &resto[fim..];
        }
    }
    v
}

/// Coluna (a partir de 0) e linha (a partir de 0) de uma referência `B12`.
fn referencia(r: &str) -> Option<(usize, usize)> {
    let letras: String = r.chars().take_while(char::is_ascii_alphabetic).collect();
    if letras.is_empty() {
        return None;
    }
    let col = letras.chars().fold(0usize, |a, c| {
        a * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1)
    }) - 1;
    let linha: usize = r[letras.len()..].parse().ok()?;
    Some((col, linha.checked_sub(1)?))
}

fn ler_arquivo(z: &mut zip::ZipArchive<Cursor<&[u8]>>, nome: &str) -> Option<String> {
    let mut f = z.by_name(nome).ok()?;
    let mut s = String::new();
    f.read_to_string(&mut s).ok()?;
    Some(s)
}

/// O texto dentro das marcas `<t>` de um trecho (cadeia compartilhada ou em linha).
fn textos_t(ps: &[Parte]) -> String {
    let mut s = String::new();
    let mut dentro = false;
    for p in ps {
        match p {
            Parte::Marca(m) if m.local() == "t" && !m.vazia => dentro = !m.fecha,
            // As anotações fonéticas não fazem parte do texto.
            Parte::Texto(t) if dentro => s.push_str(t),
            _ => {}
        }
    }
    s
}

fn compartilhadas(xml: &str) -> Vec<String> {
    let ps = partes(xml);
    let mut v = Vec::new();
    let mut ini = None;
    for (i, p) in ps.iter().enumerate() {
        if let Parte::Marca(m) = p
            && m.local() == "si"
        {
            if m.vazia {
                v.push(String::new());
            } else if !m.fecha {
                ini = Some(i);
            } else if let Some(a) = ini.take() {
                // Ignora o texto fonético (`rPh`).
                let mut trecho = Vec::new();
                let mut fon = 0;
                for q in &ps[a..i] {
                    if let Parte::Marca(m) = q
                        && m.local() == "rPh"
                        && !m.vazia
                    {
                        fon += if m.fecha { -1 } else { 1 };
                        continue;
                    }
                    if fon == 0 {
                        trecho.push(q);
                    }
                }
                let trecho: Vec<Parte> = trecho
                    .into_iter()
                    .map(|q| match q {
                        Parte::Texto(t) => Parte::Texto(t.clone()),
                        Parte::Marca(m) => Parte::Marca(Marca {
                            nome: m.nome.clone(),
                            atributos: Vec::new(),
                            fecha: m.fecha,
                            vazia: m.vazia,
                        }),
                    })
                    .collect();
                v.push(textos_t(&trecho));
            }
        }
    }
    v
}

/// As abas do arquivo: nome e caminho do XML dentro do zip, na ordem do livro.
fn abas(z: &mut zip::ZipArchive<Cursor<&[u8]>>) -> Vec<(String, String)> {
    let livro = ler_arquivo(z, "xl/workbook.xml").unwrap_or_default();
    let rels = ler_arquivo(z, "xl/_rels/workbook.xml.rels").unwrap_or_default();
    let mut alvo: BTreeMap<String, String> = BTreeMap::new();
    for p in partes(&rels) {
        if let Parte::Marca(m) = p
            && m.local() == "Relationship"
            && let (Some(id), Some(t)) = (m.atributo("Id"), m.atributo("Target"))
        {
            let t = t.trim_start_matches('/');
            let t = if t.starts_with("xl/") {
                t.to_string()
            } else {
                format!("xl/{t}")
            };
            alvo.insert(id.to_string(), t);
        }
    }
    let mut v = Vec::new();
    for p in partes(&livro) {
        if let Parte::Marca(m) = p
            && m.local() == "sheet"
            && !m.fecha
        {
            let nome = m.atributo("name").unwrap_or("Planilha").to_string();
            let caminho = m
                .atributos
                .iter()
                .find(|(k, _)| k == "r:id" || k.ends_with(":id"))
                .and_then(|(_, id)| alvo.get(id).cloned())
                .unwrap_or_else(|| format!("xl/worksheets/sheet{}.xml", v.len() + 1));
            v.push((nome, caminho));
        }
    }
    if v.is_empty() && z.by_name("xl/worksheets/sheet1.xml").is_ok() {
        v.push(("Planilha1".into(), "xl/worksheets/sheet1.xml".into()));
    }
    v
}

/// The names of the sheets, in the workbook order.
pub fn sheets(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut z =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("xlsx inválido: {e}"))?;
    Ok(abas(&mut z).into_iter().map(|(n, _)| n).collect())
}

/// Reads one sheet (`None`: the first) into a table. The numbers keep the dot as decimal
/// mark, as the file stores them.
pub fn read(bytes: &[u8], sheet: Option<&str>) -> Result<Table, String> {
    let mut z =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("xlsx inválido: {e}"))?;
    let lista = abas(&mut z);
    let (_, caminho) = match sheet {
        Some(a) => lista
            .iter()
            .find(|(n, _)| n == a)
            .cloned()
            .ok_or_else(|| format!("a aba '{a}' não existe no arquivo"))?,
        None => lista.first().cloned().ok_or("o arquivo não tem abas")?,
    };
    let comp = ler_arquivo(&mut z, "xl/sharedStrings.xml")
        .map(|x| compartilhadas(&x))
        .unwrap_or_default();
    let xml = ler_arquivo(&mut z, &caminho).ok_or_else(|| format!("aba sem dados: {caminho}"))?;
    let mut grade: BTreeMap<(usize, usize), String> = BTreeMap::new();
    let ps = partes(&xml);
    let mut i = 0;
    let mut linha_atual = 0usize;
    let mut col_atual = 0usize;
    while i < ps.len() {
        if let Parte::Marca(m) = &ps[i] {
            if m.local() == "row" && !m.fecha {
                if let Some(r) = m.atributo("r").and_then(|r| r.parse::<usize>().ok()) {
                    linha_atual = r.saturating_sub(1);
                }
                col_atual = 0;
                if m.vazia {
                    linha_atual += 1;
                }
            } else if m.local() == "row" && m.fecha {
                linha_atual += 1;
            } else if m.local() == "c" && !m.fecha {
                let (c, l) = m
                    .atributo("r")
                    .and_then(referencia)
                    .unwrap_or((col_atual, linha_atual));
                col_atual = c + 1;
                let tipo = m.atributo("t").unwrap_or("n").to_string();
                if m.vazia {
                    i += 1;
                    continue;
                }
                // Junta até o fim da célula.
                let ini = i + 1;
                let mut fim = ini;
                while fim < ps.len() {
                    if let Parte::Marca(x) = &ps[fim]
                        && x.local() == "c"
                        && x.fecha
                    {
                        break;
                    }
                    fim += 1;
                }
                let trecho = &ps[ini..fim.min(ps.len())];
                let mut valor = String::new();
                let mut em_v = false;
                for p in trecho {
                    match p {
                        Parte::Marca(x) if x.local() == "v" && !x.vazia => em_v = !x.fecha,
                        Parte::Texto(t) if em_v => valor.push_str(t),
                        _ => {}
                    }
                }
                let texto = match tipo.as_str() {
                    "s" => valor
                        .trim()
                        .parse::<usize>()
                        .ok()
                        .and_then(|k| comp.get(k).cloned())
                        .unwrap_or_default(),
                    "inlineStr" => textos_t(trecho),
                    "b" => if valor.trim() == "1" {
                        "VERDADEIRO"
                    } else {
                        "FALSO"
                    }
                    .to_string(),
                    "e" => String::new(),
                    _ => valor.trim().to_string(),
                };
                if !texto.is_empty() {
                    grade.insert((l, c), texto);
                }
                i = fim;
            }
        }
        i += 1;
    }
    let nl = grade.keys().map(|(l, _)| l + 1).max().unwrap_or(0);
    let nc = grade.keys().map(|(_, c)| c + 1).max().unwrap_or(0);
    let mut rows: Vec<Vec<String>> = (0..nl)
        .map(|l| {
            (0..nc)
                .map(|c| grade.get(&(l, c)).cloned().unwrap_or_default())
                .collect()
        })
        .collect();
    rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    let header = rows
        .first()
        .filter(|l| {
            l.iter()
                .any(|c| !c.trim().is_empty() && number(c, false).is_none())
        })
        .cloned();
    if header.is_some() {
        rows.remove(0);
    }
    Ok(Table {
        header,
        rows,
        decimal_comma: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_xlsxwriter::Workbook;

    fn workbook() -> Vec<u8> {
        let mut wb = Workbook::new();
        let s = wb.add_worksheet();
        s.set_name("Defeitos").unwrap();
        s.write_string(0, 0, "Categoria").unwrap();
        s.write_string(0, 1, "Quantidade").unwrap();
        for (i, (n, q)) in [
            ("Rebarba", 48.0),
            ("Risco & mancha", 22.5),
            ("Ação <alta>", 3.0),
        ]
        .iter()
        .enumerate()
        {
            s.write_string(i as u32 + 1, 0, *n).unwrap();
            s.write_number(i as u32 + 1, 1, *q).unwrap();
        }
        // A gap row and a cell far to the right.
        s.write_string(5, 0, "Empeno").unwrap();
        s.write_number(5, 3, 7.0).unwrap();
        s.write_boolean(5, 2, true).unwrap();
        let m = wb.add_worksheet();
        m.set_name("Medidas").unwrap();
        for i in 0..4u32 {
            m.write_number(i, 0, 10.0 + f64::from(i) / 4.0).unwrap();
            m.write_number(i, 1, f64::from(i)).unwrap();
        }
        wb.save_to_buffer().unwrap()
    }

    #[test]
    fn sheets_header_text_numbers_and_gaps() {
        let b = workbook();
        assert_eq!(sheets(&b).unwrap(), ["Defeitos", "Medidas"]);
        let t = read(&b, None).unwrap();
        assert_eq!(
            t.header.as_deref().unwrap(),
            ["Categoria", "Quantidade", "", ""]
        );
        assert_eq!(t.rows.len(), 4);
        assert_eq!(t.rows[1][0], "Risco & mancha");
        assert_eq!(t.rows[2][0], "Ação <alta>");
        assert_eq!(number(&t.rows[1][1], t.decimal_comma), Some(22.5));
        assert_eq!(t.rows[3], ["Empeno", "", "VERDADEIRO", "7"]);
        let m = read(&b, Some("Medidas")).unwrap();
        assert!(m.header.is_none());
        assert_eq!(m.rows[1][0], "10.25");
        assert!(read(&b, Some("Outra")).unwrap_err().contains("não existe"));
        assert!(read(b"not a zip", None).is_err());
    }
}
