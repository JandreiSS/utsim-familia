//! The product project a model came from (UTSim Desenvolvimento de Produto, decision P4 of
//! `docs/plano_desenvolvimento_produto.md`): the project folder and the revision whose
//! routings, times and critical characteristics were written into the model. Never read by
//! the simulation.

use serde::{Deserialize, Serialize};

/// Link from a model to the product project it was generated from or updated by.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductLink {
    /// Folder of the product project, relative to the model file when possible.
    #[serde(alias = "projeto")]
    pub project: String,
    /// Revision of the project (0: the working copy, no saved revision yet).
    #[serde(default, alias = "revisao")]
    pub revision: u32,
}

#[cfg(test)]
mod tests {
    use crate::Model;

    #[test]
    fn product_link_round_trip() {
        let text = r#"Model(name: "m", run: (stop_time: 10.0), objects: [],
            product: (projeto: "../Bicicleta", revisao: 3))"#;
        let m = Model::parse(text).unwrap();
        let link = m.product.clone().unwrap();
        assert_eq!(link.project, "../Bicicleta");
        assert_eq!(link.revision, 3);
        let config = ron::ser::PrettyConfig::new()
            .struct_names(true)
            .extensions(ron::extensions::Extensions::IMPLICIT_SOME);
        let out = ron::ser::to_string_pretty(&m, config).unwrap();
        assert!(out.contains("project: \"../Bicicleta\""), "{out}");
        assert_eq!(Model::parse(&out).unwrap(), m);
        // Absent: nothing written.
        let m = Model::parse(r#"Model(name: "m", run: (stop_time: 10.0), objects: [])"#).unwrap();
        assert!(m.product.is_none());
        let out = ron::ser::to_string_pretty(&m, ron::ser::PrettyConfig::new()).unwrap();
        assert!(!out.contains("product"));
    }
}
