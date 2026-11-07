//! An 8D sent to the portfolio of UTSim Seis Sigma (stage QS13 of
//! `docs/plano_qualidade_e_seguranca.md`, section 7.1): when the cause does not show up,
//! the 8D becomes an idea in the portfolio, with the 8D itself attached by its SHA-256
//! (the same attachment store the Seis Sigma app uses) and the hash checked after writing.

use std::path::Path;

use utsim_lss_core::portfolio;
use utsim_lss_core::{Edit, Id};
use utsim_lss_store::project::ATTACHMENTS;
use utsim_lss_store::{Clock, Identity, ProjectFolder};

use crate::oito_d::OitoD;

/// What the sending wrote in the portfolio.
#[derive(Clone, Debug, PartialEq)]
pub struct Envio {
    pub ideia: Id,
    /// Name of the attachment (`<sha256>.json`).
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
    let mut pasta = ProjectFolder::open(carteira, me, None).map_err(|e| e.to_string())?;
    if !pasta.is_portfolio() {
        return Err(format!(
            "{} não é uma carteira do UTSim Seis Sigma",
            carteira.display()
        ));
    }
    let bytes = serde_json::to_vec_pretty(oito_d).map_err(|e| e.to_string())?;
    let hash = sha256_hex(&bytes);
    // The attachment store takes a file: write it to a temporary place first.
    let tmp = std::env::temp_dir().join(format!("utsim-8d-{hash}.json"));
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    let anexo = pasta.add_attachment(&tmp).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&tmp);
    let anexo = anexo?;
    // Read back and check the hash before pointing the idea at it.
    let gravado =
        std::fs::read(carteira.join(ATTACHMENTS).join(&anexo)).map_err(|e| e.to_string())?;
    if sha256_hex(&gravado) != hash || !anexo.starts_with(&hash) {
        return Err("o anexo gravado na carteira não confere com o 8D".into());
    }
    let c = pasta.project_id();
    let ideia = clock.new_id();
    let r = portfolio::Idea::entity_ref(ideia);
    let mut edits = portfolio::new_idea(c, ideia, &format!("8D: {}", oito_d.titulo));
    edits.push(Edit::new(r, portfolio::fields::PROCESS, posto));
    edits.push(Edit::new(r, portfolio::fields::PROBLEM, oito_d.resumo()));
    edits.push(Edit::new(
        r,
        portfolio::fields::DATA,
        format!("8D anexado: {ATTACHMENTS}/{anexo} (SHA-256 {hash}), vindo do UTSim Qualidade"),
    ));
    pasta.commit(edits, clock).map_err(|e| e.to_string())?;
    Ok(Envio { ideia, anexo, hash })
}
