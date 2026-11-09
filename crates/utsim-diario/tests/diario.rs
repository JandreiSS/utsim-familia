//! The shared signed journal: chain per author, holes, bifurcation, incremental check and
//! seals with several signatures.

use serde::{Deserialize, Serialize};
use utsim_diario::{
    Chave, Elo, Encadeado, Problema, Selo, assinar, conferir, conferir_desde, conferir_selo, selar,
    sha256_hex,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Reg {
    autor: String,
    texto: String,
    sequencia: u64,
    anterior: String,
}

impl Encadeado for Reg {
    fn autor(&self) -> &str {
        &self.autor
    }
    fn sequencia(&self) -> u64 {
        self.sequencia
    }
    fn anterior(&self) -> &str {
        &self.anterior
    }
    fn encadear(&mut self, sequencia: u64, anterior: String) {
        self.sequencia = sequencia;
        self.anterior = anterior;
    }
}

fn reg(autor: &str, texto: &str) -> Reg {
    Reg {
        autor: autor.into(),
        texto: texto.into(),
        sequencia: 0,
        anterior: String::new(),
    }
}

fn chave(n: u8) -> Chave {
    Chave::de_semente([n; 32])
}

fn cadeia(c: &Chave, autor: &str, n: usize) -> Vec<Elo> {
    let mut v: Vec<Elo> = Vec::new();
    for i in 0..n {
        let e = assinar(c, reg(autor, &format!("op {i}")), v.last()).unwrap();
        v.push(e);
    }
    v
}

#[test]
fn cadeia_inteira_confere() {
    let c = chave(1);
    let d = cadeia(&c, "ana", 5);
    let v = conferir::<Reg>(&c.publica(), "ana", &d);
    assert!(v.limpa(), "{:?}", v.problemas);
    assert_eq!(v.validos.len(), 5);
    assert_eq!(
        v.validos.iter().map(|r| r.sequencia).collect::<Vec<_>>(),
        [1, 2, 3, 4, 5]
    );
    assert_eq!(v.ultimo.as_ref(), d.last());
    // Out of file order, the same.
    let mut r = d.clone();
    r.reverse();
    assert!(conferir::<Reg>(&c.publica(), "ana", &r).limpa());
}

#[test]
fn alteracao_assinatura_e_autor_ficam_de_fora() {
    let c = chave(1);
    let mut d = cadeia(&c, "ana", 3);
    d[1].corpo = d[1].corpo.replace("op 1", "op X");
    let v = conferir::<Reg>(&c.publica(), "ana", &d);
    assert!(
        v.problemas
            .contains(&Problema::HashNaoConfere { indice: 1 })
    );
    assert_eq!(v.validos.len(), 2);

    let mut d = cadeia(&c, "ana", 2);
    d[0].corpo = d[0].corpo.replace("op 0", "op Y");
    d[0].hash = sha256_hex(d[0].corpo.as_bytes());
    let v = conferir::<Reg>(&c.publica(), "ana", &d);
    assert!(
        v.problemas
            .contains(&Problema::AssinaturaInvalida { indice: 0 })
    );

    let d = cadeia(&c, "bia", 1);
    let v = conferir::<Reg>(&c.publica(), "ana", &d);
    assert_eq!(
        v.problemas,
        [Problema::OutroAutor {
            indice: 0,
            autor: "bia".into()
        }]
    );
    // Another key.
    let v = conferir::<Reg>(&chave(2).publica(), "bia", &d);
    assert_eq!(v.problemas, [Problema::AssinaturaInvalida { indice: 0 }]);
}

#[test]
fn buraco_no_meio_e_apontado_e_final_faltando_nao() {
    let c = chave(1);
    let d = cadeia(&c, "ana", 6);
    // A late copy of the shared folder: only the start.
    assert!(conferir::<Reg>(&c.publica(), "ana", &d[..3]).limpa());
    // A hole in the middle.
    let furada: Vec<Elo> = d
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 2)
        .map(|(_, e)| e.clone())
        .collect();
    let v = conferir::<Reg>(&c.publica(), "ana", &furada);
    assert_eq!(v.problemas, [Problema::Falta { depois_de: 2 }]);
    assert_eq!(v.validos.len(), 5);
    // The start missing.
    let v = conferir::<Reg>(&c.publica(), "ana", &d[1..]);
    assert_eq!(v.problemas, [Problema::Falta { depois_de: 0 }]);
}

#[test]
fn copia_repetida_conta_uma_vez() {
    let c = chave(1);
    let mut d = cadeia(&c, "ana", 3);
    d.push(d[1].clone());
    let v = conferir::<Reg>(&c.publica(), "ana", &d);
    assert!(v.limpa(), "{:?}", v.problemas);
    assert_eq!(v.validos.len(), 3);
}

#[test]
fn backup_restaurado_e_bifurcacao() {
    let c = chave(1);
    let d = cadeia(&c, "ana", 4);
    // The backup had up to 2; the person wrote a different 3 on it.
    let outra = assinar(&c, reg("ana", "outra 3"), Some(&d[1])).unwrap();
    let mut todos = d.clone();
    todos.push(outra);
    let v = conferir::<Reg>(&c.publica(), "ana", &todos);
    assert_eq!(v.problemas, [Problema::Bifurcacao { sequencia: 3 }]);
    assert_eq!(v.validos.len(), 5);
}

#[test]
fn conferencia_incremental_igual_a_inteira() {
    let c = chave(1);
    let d = cadeia(&c, "ana", 8);
    let a = conferir::<Reg>(&c.publica(), "ana", &d[..5]);
    assert!(a.limpa());
    let b = conferir_desde::<Reg>(&c.publica(), "ana", a.ultimo.as_ref(), &d[5..]);
    assert!(b.limpa(), "{:?}", b.problemas);
    assert_eq!(b.validos.len(), 3);
    assert_eq!(b.ultimo.as_ref(), d.last());
    let inteira = conferir::<Reg>(&c.publica(), "ana", &d);
    let mut juntas = a.validos.clone();
    juntas.extend(b.validos);
    assert_eq!(juntas, inteira.validos);
    // Nothing new: nothing to say, and the starting point stays.
    let n = conferir_desde::<Reg>(&c.publica(), "ana", b.ultimo.as_ref(), &[]);
    assert!(n.limpa() && n.validos.is_empty());
    assert_eq!(n.ultimo.as_ref(), d.last());
    // A hole right after the last checked link.
    let f = conferir_desde::<Reg>(&c.publica(), "ana", Some(&d[4]), &d[6..]);
    assert_eq!(f.problemas, [Problema::Falta { depois_de: 5 }]);
    // A new link that does not point to the last checked one.
    let solto = assinar(&c, reg("ana", "solto"), Some(&d[3])).unwrap();
    let mut solto: Reg = solto.ler().unwrap();
    solto.sequencia = 6;
    let e = {
        let corpo = serde_json::to_string(&solto).unwrap();
        Elo {
            hash: sha256_hex(corpo.as_bytes()),
            assinatura: c.assinar(corpo.as_bytes()),
            corpo,
        }
    };
    let o = conferir_desde::<Reg>(&c.publica(), "ana", Some(&d[4]), &[e]);
    assert_eq!(o.problemas, [Problema::ForaDeOrdem { sequencia: 6 }]);
    // After the last checked link, an old sequence with another hash is a bifurcation.
    let velho = assinar(&c, reg("ana", "velho 4"), Some(&d[2])).unwrap();
    let r = conferir_desde::<Reg>(&c.publica(), "ana", Some(&d[4]), &[velho]);
    assert_eq!(r.problemas, [Problema::Bifurcacao { sequencia: 4 }]);
    assert_eq!(r.ultimo.as_ref(), Some(&d[4]));
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Certificado {
    pessoa: String,
    faixa: String,
}

#[test]
fn selo_com_varias_assinaturas() {
    let mbb = chave(7);
    let patrocinador = chave(8);
    let cert = Certificado {
        pessoa: "Ana".into(),
        faixa: "verde".into(),
    };
    let a = selar(&mbb, &cert, 1_800_000_000_000).unwrap();
    let b = selar(&patrocinador, &cert, 1_800_000_100_000).unwrap();
    assert_eq!(a.hash_corpo(), b.hash_corpo());
    assert_eq!(a.publica, mbb.publica());
    assert_eq!(b.publica, patrocinador.publica());
    assert_eq!(conferir_selo::<Certificado>(&a).unwrap(), cert);
    assert_eq!(conferir_selo::<Certificado>(&b).unwrap(), cert);
    // Neither the instant nor the body change unnoticed.
    let mut x: Selo = a.clone();
    x.quando += 1;
    assert!(conferir_selo::<Certificado>(&x).is_err());
    let mut y = a.clone();
    y.corpo = y.corpo.replace("verde", "preta");
    assert!(conferir_selo::<Certificado>(&y).is_err());
    let mut z = a;
    z.publica = patrocinador.publica();
    assert!(conferir_selo::<Certificado>(&z).is_err());
}

#[test]
fn chave_no_arquivo_com_cabecalho_do_app() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("perfil").join("chave.txt");
    let a = Chave::carregar_ou_criar(&p, "app-x v1").unwrap();
    let b = Chave::carregar_ou_criar(&p, "app-x v1").unwrap();
    assert_eq!(a.publica(), b.publica());
    assert!(Chave::carregar_ou_criar(&p, "app-y v1").is_err());
    assert_eq!(a.publica().len(), 64);
    assert_eq!(a.assinar(b"x").len(), 128);
}
