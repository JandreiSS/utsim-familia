//! An 8D sent to the portfolio of UTSim LSS (stage QS13 of
//! `docs/plano_qualidade_e_seguranca.md`, section 7.1): when the cause does not show up,
//! the 8D becomes an idea in the portfolio through the intake of the Seis Sigma store
//! (`utsim_lss_store::intake::submit_idea`), with the source `qualidade_8d`, the 8D's
//! number as reference and the 8D itself attached by content. The attachment is read back
//! and its SHA-256 checked.

use std::path::Path;

use utsim_lss_core::Id;
use utsim_lss_core::portfolio::{self, SOURCE_8D, Submission};
use utsim_lss_store::intake::{attachment_path, submit_idea};
use utsim_lss_store::{Clock, Identity, ProjectFolder};

use crate::oito_d::OitoD;

/// What the sending wrote in the portfolio.
#[derive(Clone, Debug, PartialEq)]
pub struct Envio {
    pub ideia: Id,
    /// Name of the attachment in the portfolio's `anexos/`.
    pub anexo: String,
    pub hash: String,
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Sends `oito_d` to the portfolio folder `carteira` as a new idea, by `me`.
pub fn enviar(
    carteira: &Path,
    me: &Identity,
    clock: &dyn Clock,
    oito_d: &OitoD,
    posto: &str,
) -> Result<Envio, String> {
    let bytes = serde_json::to_vec_pretty(oito_d).map_err(|e| e.to_string())?;
    let hash = sha256_hex(&bytes);
    let referencia = if oito_d.id.is_empty() {
        oito_d.titulo.clone()
    } else {
        format!("8D-{}", oito_d.id)
    };
    // The intake copies files: write the 8D to a temporary place first.
    let dir = std::env::temp_dir().join(format!("utsim-8d-{}", &hash[..16]));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let arquivo = dir.join(format!(
        "{}.json",
        referencia.replace(['/', '\\', ':'], "_")
    ));
    std::fs::write(&arquivo, &bytes).map_err(|e| e.to_string())?;
    let ideia = submit_idea(
        carteira,
        me,
        clock,
        &Submission {
            title: format!("8D: {}", oito_d.titulo),
            process: posto.to_string(),
            problem: oito_d.resumo(),
            data: format!("8D anexado (SHA-256 {hash}), vindo do UTSim Qualidade"),
            rough_gain: 0.0,
            source: SOURCE_8D.into(),
            reference: referencia,
        },
        std::slice::from_ref(&arquivo),
    );
    let _ = std::fs::remove_dir_all(&dir);
    let ideia = ideia.map_err(|e| e.to_string())?;
    // Read back the attachment linked to the idea and check its hash.
    let pasta = ProjectFolder::open(carteira, me, None).map_err(|e| e.to_string())?;
    let anexo = portfolio::attachments(pasta.state(), ideia)
        .into_iter()
        .find(|a| {
            std::fs::read(attachment_path(carteira, &a.file)).is_ok_and(|b| sha256_hex(&b) == hash)
        })
        .ok_or("o anexo gravado na carteira não confere com o 8D")?;
    Ok(Envio {
        ideia,
        anexo: anexo.file,
        hash,
    })
}
