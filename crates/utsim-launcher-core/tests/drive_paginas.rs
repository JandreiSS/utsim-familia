//! The Google Drive pages saved in `tests/dados` and the download through them.

use std::sync::atomic::AtomicBool;

use utsim_launcher_core::Error;
use utsim_launcher_core::download::{Job, download};
use utsim_launcher_core::drive::{self, Page};
use utsim_launcher_core::transport::{Canned, MemoryTransport};

const ID: &str = "1AbCdEfGhIjKlMnOpQrStUvWxYz012345";

fn page(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("dados")
        .join(name);
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn current_virus_warning_gives_the_form_address() {
    let url = drive::download_url(ID);
    let Page::Confirm(next) = drive::interpret(&page("drive_aviso_virus.html"), &url) else {
        panic!("não reconheceu o aviso");
    };
    assert_eq!(
        next,
        format!(
            "https://drive.usercontent.google.com/download?id={ID}&export=download&confirm=t&uuid=5c1e8a52-7d0b-4c8f-9a1e-2b3c4d5e6f70"
        )
    );
}

#[test]
fn portuguese_warning_with_single_quotes_and_extra_fields() {
    let url = drive::download_url(ID);
    let Page::Confirm(next) = drive::interpret(&page("drive_aviso_virus_pt.html"), &url) else {
        panic!("não reconheceu o aviso em português");
    };
    assert!(next.starts_with("https://drive.usercontent.google.com/download?id="));
    assert!(next.contains("&confirm=t&"));
    assert!(next.contains("uuid=0f9e8d7c-6b5a-4938-2716-05f4e3d2c1b0"));
    assert!(next.contains("at=APvzH3o%3A1730550000000"), "{next}");
}

#[test]
fn legacy_warning_link() {
    let url = format!("https://drive.google.com/uc?export=download&id={ID}");
    assert_eq!(
        drive::interpret(&page("drive_aviso_legado.html"), &url),
        Page::Confirm(format!(
            "https://drive.google.com/uc?export=download&confirm=Xy9_&id={ID}"
        ))
    );
}

#[test]
fn other_pages_become_messages() {
    let url = drive::download_url(ID);
    assert_eq!(
        drive::interpret(&page("drive_cota_excedida.html"), &url),
        Page::QuotaExceeded
    );
    assert_eq!(
        drive::interpret(&page("drive_sem_permissao.html"), &url),
        Page::NeedsPermission
    );
    assert_eq!(drive::interpret("<html>nada</html>", &url), Page::Unknown);
}

#[test]
fn download_goes_through_the_warning_and_resumes_through_it() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pacote.utpk");
    let t = MemoryTransport::new();
    let share = format!("https://drive.google.com/file/d/{ID}/view?usp=sharing");
    let first = drive::download_url(ID);
    let html = page("drive_aviso_virus.html");
    let Page::Confirm(confirmed) = drive::interpret(&html, &first) else {
        panic!()
    };
    let body: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
    t.add(&first, Canned::html(&html));
    t.add(&confirmed, Canned::file(body.clone()));

    let stop = AtomicBool::new(false);
    let job = Job {
        url: &share,
        dest: &dest,
        expected_size: Some(body.len() as u64),
        cancel: &stop,
    };
    // First try: the warning, then the file, cut after 70 000 bytes.
    t.cut_next_after(70_000);
    let r = download(&t, &job, &mut |_| {});
    assert!(matches!(r, Err(Error::Network(_))), "{r:?}");
    // Second try: warning again (the range request also gets it), then the rest.
    download(&t, &job, &mut |_| {}).unwrap();
    assert_eq!(std::fs::read(&dest).unwrap(), body);
    let reqs = t.requests();
    assert_eq!(reqs[0], (first.clone(), None));
    assert_eq!(reqs[1], (confirmed.clone(), None));
    assert_eq!(reqs[2], (first, Some(70_000)));
    assert_eq!(reqs[3], (confirmed, Some(70_000)));
}

#[test]
fn quota_page_during_download_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("pacote.utpk");
    let t = MemoryTransport::new();
    let url = drive::download_url(ID);
    t.add(&url, Canned::html(&page("drive_cota_excedida.html")));
    let stop = AtomicBool::new(false);
    let job = Job {
        url: &url,
        dest: &dest,
        expected_size: Some(10),
        cancel: &stop,
    };
    let err = download(&t, &job, &mut |_| {}).unwrap_err();
    assert!(matches!(err, Error::Drive(_)));
    assert!(
        err.to_string().contains("tente de novo em algumas horas"),
        "{err}"
    );
}
