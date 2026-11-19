//! Author publishes, tester downloads and installs: the whole path without network.

use std::fs;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use utsim_launcher_core::code::AccessCode;
use utsim_launcher_core::crypto::Argon2Params;
use utsim_launcher_core::download::{Job, download};
use utsim_launcher_core::install::{self, Layout};
use utsim_launcher_core::manifest::{Manifest, Product, Release, STABLE, TESTING};
use utsim_launcher_core::transport::{Canned, MemoryTransport};
use utsim_launcher_core::{Error, package, signature};

const PLATFORM: &str = "windows-x64";

fn program(dir: &Path, version: &str) {
    fs::create_dir_all(dir.join("bin")).unwrap();
    fs::write(dir.join("bin/app.exe"), format!("programa {version}")).unwrap();
    fs::create_dir_all(dir.join("examples")).unwrap();
    fs::write(dir.join("examples/um.ron"), "(modelo)").unwrap();
}

/// Builds a release of `version` wrapped for `codes`, serves it on `t`.
fn publish(
    work: &Path,
    t: &MemoryTransport,
    version: &str,
    channel: &str,
    codes: &[&AccessCode],
) -> Release {
    let src = work.join(format!("src-{version}"));
    program(&src, version);
    let zip = work.join(format!("{version}.zip"));
    package::zip_dir(&src, &zip).unwrap();
    let sealed = package::seal_file(&zip, &work.join(format!("utsim-{version}.utpk"))).unwrap();
    let wraps = package::wrap_for_all(
        &sealed.key,
        codes.iter().copied(),
        "utsim",
        version,
        PLATFORM,
        Argon2Params::TEST,
    )
    .unwrap();
    let url = format!("http://drive.local/{version}");
    t.add(&url, Canned::file(fs::read(&sealed.file).unwrap()));
    Release {
        version: version.into(),
        channel: channel.into(),
        notes: format!("Notas da {version}"),
        executable: "bin/app.exe".into(),
        packages: vec![package::package_entry(PLATFORM, &url, &sealed, wraps)],
        ..Release::default()
    }
}

fn fetch_and_install(
    layout: &Layout,
    t: &MemoryTransport,
    release: &Release,
    code: &AccessCode,
) -> utsim_launcher_core::Result<std::path::PathBuf> {
    let pkg = release.package(PLATFORM).unwrap();
    let file = layout.package_file("utsim", release, pkg);
    let stop = AtomicBool::new(false);
    let job = Job {
        url: &pkg.url,
        dest: &file,
        expected_size: Some(pkg.size),
        cancel: &stop,
    };
    download(t, &job, &mut |_| {})?;
    install::install(layout, "utsim", release, pkg, Some(code), &file)
}

#[test]
fn publish_install_update_revoke() {
    let work = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(home.path());
    let t = MemoryTransport::new();
    let author = signature::generate().unwrap();
    let trusted = [author.verifying_key()];
    let ana = AccessCode::generate().unwrap();
    let bruno = AccessCode::generate().unwrap();
    let estranho = AccessCode::generate().unwrap();

    // Version 0.1.0, stable, for Ana and Bruno.
    let mut m = Manifest::new();
    m.sequence = 1;
    m.products.push(Product {
        id: "utsim".into(),
        name: "UTSim".into(),
        releases: vec![publish(work.path(), &t, "0.1.0", STABLE, &[&ana, &bruno])],
        ..Product::default()
    });
    let text = m.to_signed(&author).unwrap();

    // Tester side.
    let m = Manifest::from_signed(&text, &trusted).unwrap();
    let p = m.product("utsim").unwrap();
    let r1 = p.latest(STABLE, PLATFORM).unwrap().clone();
    let r = fetch_and_install(&layout, &t, &r1, &estranho);
    assert!(matches!(r, Err(Error::NoAccess)), "{r:?}");
    let dir = fetch_and_install(&layout, &t, &r1, &ana).unwrap();
    assert_eq!(
        fs::read_to_string(dir.join("bin/app.exe")).unwrap(),
        "programa 0.1.0"
    );
    assert!(dir.join("examples/um.ron").is_file());
    install::activate(&layout, "utsim", "0.1.0").unwrap();
    assert_eq!(
        install::active_version(&layout, "utsim").as_deref(),
        Some("0.1.0")
    );

    // Bruno is revoked: 0.2.0 (testing channel) is wrapped only for Ana.
    let mut m2 = m.clone();
    m2.sequence = 2;
    let r2 = publish(work.path(), &t, "0.2.0", TESTING, &[&ana]);
    m2.product_mut("utsim").unwrap().releases.push(r2);
    let m2 = Manifest::from_signed(&m2.to_signed(&author).unwrap(), &trusted).unwrap();
    let p2 = m2.product("utsim").unwrap();
    assert_eq!(p2.latest(STABLE, PLATFORM).unwrap().version, "0.1.0");
    let r2 = p2.latest(TESTING, PLATFORM).unwrap().clone();
    assert_eq!(r2.version, "0.2.0");

    let bruno_home = tempfile::tempdir().unwrap();
    let bruno_layout = Layout::new(bruno_home.path());
    assert!(matches!(
        fetch_and_install(&bruno_layout, &t, &r2, &bruno),
        Err(Error::NoAccess)
    ));
    // ... but keeps what was already published for him.
    fetch_and_install(&bruno_layout, &t, &r1, &bruno).unwrap();

    // Ana updates; the switch is atomic and the old version can then be removed.
    fetch_and_install(&layout, &t, &r2, &ana).unwrap();
    assert_eq!(
        install::active_version(&layout, "utsim").as_deref(),
        Some("0.1.0")
    );
    install::activate(&layout, "utsim", "0.2.0").unwrap();
    let (exe, _, _) = install::launch_command(&layout, "utsim").unwrap();
    assert_eq!(fs::read_to_string(exe).unwrap(), "programa 0.2.0");
    assert_eq!(
        install::installed_versions(&layout, "utsim"),
        vec!["0.2.0", "0.1.0"]
    );
    assert!(install::remove_version(&layout, "utsim", "0.2.0").is_err());
    assert_eq!(
        install::remove_old(&layout, "utsim", 0).unwrap(),
        vec!["0.1.0"]
    );
    assert_eq!(install::installed_versions(&layout, "utsim"), vec!["0.2.0"]);
    let info = install::installed(&layout, "utsim", "0.2.0").unwrap();
    assert_eq!(info.notes, "Notas da 0.2.0");
}

#[test]
fn altered_package_is_rejected_before_extraction() {
    let work = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let layout = Layout::new(home.path());
    let t = MemoryTransport::new();
    let ana = AccessCode::generate().unwrap();
    let r = publish(work.path(), &t, "0.1.0", STABLE, &[&ana]);
    let pkg = r.package(PLATFORM).unwrap();

    // Someone with write access to the folder swaps the file for another of the same size.
    let mut bytes = fs::read(work.path().join("utsim-0.1.0.utpk")).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    t.add(&pkg.url, Canned::file(bytes));
    let err = fetch_and_install(&layout, &t, &r, &ana).unwrap_err();
    assert!(matches!(err, Error::Integrity(_)), "{err:?}");
    assert!(err.to_string().contains("SHA-256"));
    assert!(install::installed_versions(&layout, "utsim").is_empty());
}

#[test]
fn zip_slip_is_refused() {
    let work = tempfile::tempdir().unwrap();
    let zip_path = work.path().join("mau.zip");
    {
        let mut z = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
        z.start_file("../fora.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut z, b"x").unwrap();
        z.finish().unwrap();
    }
    let dest = work.path().join("dest");
    assert!(install::extract_zip(&zip_path, &dest).is_err());
    assert!(!work.path().join("fora.txt").exists());
}

#[test]
fn disk_space_is_checked_with_a_clear_message() {
    assert_eq!(install::human_size(500), "500 bytes");
    assert_eq!(install::human_size(1536), "1,5 kB");
    assert_eq!(install::human_size(320 << 20), "320 MB");
    assert_eq!(install::human_size((3 << 30) / 2), "1,5 GB");

    let dir = tempfile::tempdir().unwrap();
    // A folder not created yet is measured on its nearest existing parent.
    let inside = dir.path().join("ainda/nao/existe");
    install::ensure_space(&inside, 0).unwrap();
    if cfg!(windows) {
        assert!(install::free_space(&inside).is_some_and(|f| f > 0));
        let err = install::ensure_space(&inside, u64::MAX / 2).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("espaço insuficiente"), "{text}");
        assert!(text.contains("livres"), "{text}");
    }
}
