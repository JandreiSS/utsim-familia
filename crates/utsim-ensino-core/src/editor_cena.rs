//! The drawn statement in the mission editor (`docs/plano_ensino_e_jogo.md`, 3.9): the
//! editor suggests the scene from the model and the mission (cards on the stations, the
//! levers on the objects they change, the measures on the exits) and, where Blender and the
//! 3D elements are installed, renders the picture with the same script of
//! `scripts/enunciados.sh`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use utsim_model::ObjectKind;

use crate::cena::{Cartao, Cena, TipoCartao};
use crate::editor::Edicao;

/// The Blender script that renders the scenes (`tools/blender/enunciado.py`).
pub const SCRIPT_BLENDER: &str = include_str!("../../../tools/blender/enunciado.py");

/// The files of a scene as the package keeps them, by path relative to the mission folder
/// (the picture, its anchors and, in a layout, one picture per machine).
pub type ArquivosCena = BTreeMap<String, Vec<u8>>;

/// The paths of the files of `cena` for the machines `maquinas` of a layout.
pub fn caminhos_cena(cena: &Cena, maquinas: &[String]) -> Vec<String> {
    let mut v = vec![cena.imagem.clone(), cena.caminho_ancoras()];
    v.extend(maquinas.iter().map(|m| cena.imagem_maquina(m)));
    v
}

/// The last part of a mission id, as a file name (`prof/linha nova` → `linha_nova`).
fn nome_arquivo(id: &str) -> String {
    let base = id.rsplit('/').next().unwrap_or(id);
    let s: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { "cena".into() } else { s }
}

impl Edicao {
    /// The machines of the layout of the mission (empty without one).
    pub fn maquinas_da_cena(&self) -> Vec<String> {
        self.base
            .arranjo
            .as_ref()
            .map(|a| a.maquinas.iter().map(|m| m.id.clone()).collect())
            .unwrap_or_default()
    }

    /// A scene for the mission: the picture named after its id, a scale that brings the
    /// model to about 30 m, and cards for the stations of a balancing, the machines of a
    /// layout, the levers in use (on the object their keys change), the first prediction
    /// (under the first queue) and the measures in use (on the object they read, else on
    /// the exit).
    pub fn sugerir_cena(&self) -> Cena {
        let m = &self.modelo_da_cena();
        let visiveis: Vec<&utsim_model::ObjectDef> = m
            .objects
            .iter()
            .filter(|o| crate::cena::elemento_padrao(o).is_some())
            .collect();
        let existe = |n: &str| visiveis.iter().any(|o| o.name == n);
        let escala = match &self.base.arranjo {
            Some(a) => 30.0 / (a.colunas.max(a.linhas) as f64 * a.celula),
            None => {
                let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
                for o in &visiveis {
                    x0 = x0.min(o.position[0]);
                    x1 = x1.max(o.position[0]);
                    y0 = y0.min(o.position[1]);
                    y1 = y1.max(o.position[1]);
                }
                let lado = (x1 - x0).max(y1 - y0);
                if lado.is_finite() && lado > 0.0 {
                    30.0 / lado
                } else {
                    1.0
                }
            }
        };
        let escala = ((escala.clamp(0.2, 1.0)) * 10.0).round() / 10.0;
        let mut cartoes: Vec<Cartao> = Vec::new();
        let mut usados: BTreeSet<String> = BTreeSet::new();
        // A free anchor on object `n`: over it, else at its foot.
        let mut ancora = |n: &str| -> String {
            for a in [n.to_string(), format!("{n}@pe")] {
                if usados.insert(a.clone()) {
                    return a;
                }
            }
            format!("{n}@pe")
        };
        if let Some(b) = self.base.balanceamento.as_ref().filter(|b| b.editavel) {
            for n in 1..=b.estacoes {
                let est = format!("{}{n}", b.prefixo);
                if existe(&est) {
                    cartoes.push(Cartao {
                        no: ancora(&est),
                        tipo: TipoCartao::Estacao(n),
                    });
                }
            }
        }
        if let Some(a) = &self.base.arranjo {
            for mq in &a.maquinas {
                cartoes.push(Cartao {
                    no: format!("{}@pe", mq.id),
                    tipo: TipoCartao::Maquina(mq.id.clone()),
                });
            }
        }
        let primeiro = visiveis.first().map(|o| o.name.clone());
        let saida = visiveis
            .iter()
            .find(|o| matches!(o.kind, ObjectKind::Sink))
            .map(|o| o.name.clone())
            .or_else(|| visiveis.last().map(|o| o.name.clone()));
        let em_grade = self.base.arranjo.is_some();
        // In a layout the objects of the model are the compiled routes: the other cards go
        // on the fixed machine (the dock), else on the first machine.
        let fixa = self.base.arranjo.as_ref().and_then(|a| {
            a.maquinas
                .iter()
                .find(|x| x.fixa)
                .or(a.maquinas.first())
                .map(|x| x.id.clone())
        });
        let geral = |n: Option<String>| if em_grade { fixa.clone() } else { n };
        for it in self.alavancas.iter().filter(|a| a.usar) {
            let a = &it.alavanca;
            let alvo = a
                .ajustes(a.inicial())
                .keys()
                .filter_map(|k| k.split_once('.').map(|(o, _)| o.to_string()))
                .find(|o| existe(o) && !em_grade)
                .or_else(|| geral(primeiro.clone()));
            if let Some(o) = alvo {
                cartoes.push(Cartao {
                    no: if em_grade {
                        format!("{o}@pe")
                    } else {
                        ancora(&o)
                    },
                    tipo: TipoCartao::Alavanca(a.id().to_string()),
                });
            }
        }
        if let Some(p) = self.base.previsao.first() {
            let fila = visiveis
                .iter()
                .find(|o| matches!(o.kind, ObjectKind::Queue { .. }))
                .map(|o| o.name.clone())
                .or(primeiro.clone());
            if let Some(o) = geral(fila) {
                cartoes.push(Cartao {
                    no: format!("{o}@pe"),
                    tipo: TipoCartao::Previsao(p.id().to_string()),
                });
            }
        }
        for it in self.metricas.iter().filter(|x| x.usar) {
            let mt = &it.metrica;
            let lido = mt
                .valor
                .iter()
                .filter_map(|t| t.chave.split_once('.').map(|(o, _)| o.to_string()))
                .find(|o| existe(o) && !em_grade);
            if let Some(o) = lido.or_else(|| geral(saida.clone())) {
                // A measure of the pieces (efficiency, moment) is live: a value card.
                let tipo = if mt.valor.iter().all(|t| crate::pecas::e_termo(&t.chave)) {
                    TipoCartao::Valor {
                        rotulo: mt.rotulo.clone(),
                        valor: mt.valor.clone(),
                        unidade: mt.unidade.clone(),
                        casas: mt.casas,
                    }
                } else {
                    TipoCartao::Metrica(mt.nome.clone())
                };
                cartoes.push(Cartao {
                    no: if em_grade {
                        format!("{o}@pe")
                    } else {
                        ancora(&o)
                    },
                    tipo,
                });
            }
        }
        Cena {
            imagem: format!("{}.cena.png", nome_arquivo(&self.base.id)),
            escala,
            unidade: "s".into(),
            elementos: BTreeMap::new(),
            ocultar: Vec::new(),
            sem_pessoa: Vec::new(),
            sem_copias: Vec::new(),
            mover: BTreeMap::new(),
            pessoas: BTreeMap::new(),
            pecas: Vec::new(),
            cartoes,
        }
    }

    /// Renders the picture of the scene with Blender (`blender`, the program) and the 3D
    /// elements in `elementos` (the `assets/modelos3d` folder of an installation), and keeps
    /// the files to write with the mission. Takes about a minute.
    pub fn gerar_cena(&self, blender: &Path, elementos: &Path) -> Result<ArquivosCena, String> {
        let cena = self
            .base
            .cena
            .as_ref()
            .ok_or_else(|| "a missão não tem cena".to_string())?;
        let raiz = elementos
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| format!("pasta dos elementos inválida: {}", elementos.display()))?;
        let pasta = std::env::temp_dir().join(format!(
            "utsim-ensino-cena-{}-{}",
            std::process::id(),
            nome_arquivo(&self.base.id)
        ));
        let _ = std::fs::remove_dir_all(&pasta);
        std::fs::create_dir_all(&pasta).map_err(|e| e.to_string())?;
        let script = pasta.join("enunciado.py");
        std::fs::write(&script, SCRIPT_BLENDER).map_err(|e| e.to_string())?;
        let planta = pasta.join("planta.json");
        let p = cena.planta(&self.modelo_da_cena(), self.base.arranjo.as_ref());
        std::fs::write(
            &planta,
            serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let saida = pasta.join("cena.png");
        let r = std::process::Command::new(blender)
            .args(["-b", "--factory-startup", "--python-exit-code", "1", "-P"])
            .arg(&script)
            .arg("--")
            .arg("--planta")
            .arg(&planta)
            .arg("--saida")
            .arg(&saida)
            .arg("--pasta")
            .arg(elementos)
            .env("UTSIM_RAIZ", raiz)
            .output()
            .map_err(|e| format!("não foi possível rodar o Blender: {e}"))?;
        if !r.status.success() {
            let err = String::from_utf8_lossy(&r.stdout);
            let fim: Vec<&str> = err.lines().rev().take(8).collect();
            return Err(format!(
                "o Blender falhou: {}",
                fim.into_iter().rev().collect::<Vec<_>>().join(" | ")
            ));
        }
        let ler = |p: &Path| std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()));
        let mut out = ArquivosCena::new();
        out.insert(cena.imagem.clone(), ler(&saida)?);
        out.insert(
            cena.caminho_ancoras(),
            ler(&pasta.join("cena.ancoras.json"))?,
        );
        for m in self.maquinas_da_cena() {
            out.insert(
                cena.imagem_maquina(&m),
                ler(&pasta.join(format!("cena.{m}.png")))?,
            );
        }
        let _ = std::fs::remove_dir_all(&pasta);
        Ok(out)
    }

    /// The model the scene draws: in a modelling mission the reference model (the process
    /// the statement describes; the student starts from almost nothing), else the model of
    /// the mission.
    pub fn modelo_da_cena(&self) -> utsim_model::Model {
        self.gabarito().unwrap_or_else(|| self.modelo.clone())
    }

    /// Problems of the scene before writing: what the scene itself checks and the picture
    /// still to be generated.
    pub fn erros_da_cena(&self) -> Vec<String> {
        let Some(c) = &self.base.cena else {
            return Vec::new();
        };
        let mut e = c.validar(&self.missao(), &self.modelo_da_cena());
        if caminhos_cena(c, &self.maquinas_da_cena())
            .iter()
            .any(|p| !self.cena_arquivos.contains_key(p))
        {
            e.push("cena: gere a imagem (ou tire a cena) antes de gravar".into());
        }
        e
    }
}

/// Blender, from `$BLENDER`, the `PATH` or the default folder of Windows.
pub fn achar_blender() -> Option<PathBuf> {
    if let Some(b) = std::env::var_os("BLENDER")
        .map(PathBuf::from)
        .filter(|p| p.exists())
    {
        return Some(b);
    }
    let exe = if cfg!(windows) {
        "blender.exe"
    } else {
        "blender"
    };
    if let Some(p) = std::env::var_os("PATH") {
        for d in std::env::split_paths(&p) {
            let c = d.join(exe);
            if c.exists() {
                return Some(c);
            }
        }
    }
    let base = Path::new("C:/Program Files/Blender Foundation");
    let mut achados: Vec<PathBuf> = std::fs::read_dir(base)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path().join("blender.exe"))
        .filter(|p| p.exists())
        .collect();
    achados.sort();
    achados.pop()
}

/// The official download page of Blender, shown when it is not found (nothing is installed
/// by the app).
pub const SITE_BLENDER: &str = "https://www.blender.org/download/";

/// The 3D elements (`assets/modelos3d`, with the people): first the ones installed with the
/// program (`assets/modelos3d` next to it or in a folder above it, which is also the
/// repository when running from the sources), else `$UTSIM_ELEMENTOS_3D`.
pub fn achar_elementos() -> Option<PathBuf> {
    let instalados = std::env::current_exe().ok().and_then(|exe| {
        exe.ancestors()
            .map(|d| d.join("assets").join("modelos3d"))
            .find(|p| elementos_completos(p))
    });
    instalados.or_else(|| {
        std::env::var_os("UTSIM_ELEMENTOS_3D")
            .map(PathBuf::from)
            .filter(|p| elementos_completos(p))
    })
}

/// Whether `p` holds the 3D elements Blender needs: the models and, two folders up, the
/// animation clips of the people (`assets/terceiros/quaternius_ual`).
pub fn elementos_completos(p: &Path) -> bool {
    let clipes = p
        .parent()
        .map(|a| a.join("terceiros/quaternius_ual/UAL1_Standard.glb"));
    p.join("pessoa_homem.glb").exists()
        && p.join("posto.glb").exists()
        && clipes.is_some_and(|c| c.exists())
}
