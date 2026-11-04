//! A project folder opened by one person on one computer.
//!
//! Reading is tolerant of what sync clients leave behind: temporary and hidden files are
//! skipped, files that do not parse yet (still arriving) stay *pending* and are retried on
//! every [`ProjectFolder::refresh`], and conflict copies of an operation file
//! (`x (1).json`) hold an operation already known and are ignored as duplicates.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use utsim_lss_core::domain::{Approach, Person, Project};
use utsim_lss_core::format::{self, Manifest};
use utsim_lss_core::presence::{self, Presence};
use utsim_lss_core::{Date, Edit, EntityRef, Id, Insert, Journal, Op, Snapshot, State};

use crate::user::Identity;
use crate::{Clock, Error, Result, fsio};

pub const EXTENSION: &str = "lss";
pub const MANIFEST: &str = "projeto.json";
pub const SNAPSHOTS: &str = "instantaneos";
pub const OPS: &str = "ops";
pub const DATA: &str = "dados";
pub const ATTACHMENTS: &str = "anexos";
pub const MODELS: &str = "modelos";
/// Reports printed by the program (made again at any time; not in the journal).
pub const REPORTS: &str = "relatorios";
/// The company's template of the reports, in a `modelos/` folder.
pub const REPORT_TEMPLATE: &str = "relatorio.json";
pub const PRESENCE: &str = "presenca";

/// A snapshot is written when this many operations are held outside one.
pub const SNAPSHOT_EVERY: usize = 500;

/// Name and version written into `projeto.json`.
pub const PROGRAM: &str = concat!("utsim-seis-sigma ", env!("CARGO_PKG_VERSION"));

/// What a refresh found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Refresh {
    /// New operations from other people (or other computers).
    pub new_ops: usize,
    /// Files that could not be read yet.
    pub pending: usize,
    /// The state had to be rebuilt from every operation.
    pub full_replay: bool,
    /// A snapshot was written.
    pub snapshot: Option<PathBuf>,
}

/// How the state was loaded (shown in the about box and useful in bug reports).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadInfo {
    /// Operations covered by the snapshot used (0: none).
    pub from_snapshot: usize,
    /// Whether that snapshot came from the local cache.
    pub from_cache: bool,
    /// Operation files read.
    pub files_read: usize,
}

#[derive(Debug)]
pub struct ProjectFolder {
    root: PathBuf,
    manifest: Manifest,
    me: Identity,
    journal: Journal,
    seen: HashSet<PathBuf>,
    pending: BTreeMap<PathBuf, String>,
    cache_dir: Option<PathBuf>,
    load: LoadInfo,
    /// Every operation kept in memory, no snapshot nor cache (program folders and coaching
    /// logs: their reconstruction checks each operation's author, `utsim_lss_core::perfis`).
    full: bool,
}

/// The path with the `.lss` extension (added when missing).
pub fn with_extension(path: &Path) -> PathBuf {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
    {
        path.to_path_buf()
    } else {
        let mut name = path.file_name().unwrap_or_default().to_os_string();
        name.push(".");
        name.push(EXTENSION);
        path.with_file_name(name)
    }
}

/// Whether `path` looks like a project folder.
pub fn is_project(path: &Path) -> bool {
    path.join(MANIFEST).is_file()
}

/// The id written at the start of an operation file name (`<uuid>-<pessoa>.json`).
fn id_from_name(path: &Path) -> Option<Id> {
    let name = path.file_name()?.to_str()?;
    name.get(..36)?.parse().ok()
}

/// Number of operations in a snapshot file name (`000184-<uuid>.json`).
fn snapshot_count(path: &Path) -> Option<usize> {
    let name = path.file_name()?.to_str()?;
    name.split('-').next()?.parse().ok()
}

impl ProjectFolder {
    /// Creates a project folder (the `.lss` extension is added if missing) with a project of
    /// the given approach, its default phases and the creator registered as a person.
    pub fn create(
        path: &Path,
        title: &str,
        approach: Approach,
        me: &Identity,
        clock: &dyn Clock,
        cache_dir: Option<&Path>,
    ) -> Result<Self> {
        Self::create_with(path, title, me, clock, cache_dir, |id| {
            Project::create(id, title.trim(), approach, || clock.new_id())
        })
    }

    /// Creates a portfolio folder (`docs/plano_lean_seis_sigma.md`, 1.1 and 8.1): the same
    /// journal as a project, whose main entity (the manifest id) is a `carteira` with its
    /// business unit and program, and the creator registered. Its project folders sit next
    /// to it.
    pub fn create_portfolio(
        path: &Path,
        name: &str,
        unit: &str,
        program: &str,
        me: &Identity,
        clock: &dyn Clock,
        cache_dir: Option<&Path>,
    ) -> Result<Self> {
        Self::create_with(path, name, me, clock, cache_dir, |id| {
            utsim_lss_core::portfolio::create(
                id,
                name,
                (clock.new_id(), unit),
                (clock.new_id(), program),
            )
        })
    }

    fn create_with(
        path: &Path,
        title: &str,
        me: &Identity,
        clock: &dyn Clock,
        cache_dir: Option<&Path>,
        initial: impl FnOnce(Id) -> Vec<Edit>,
    ) -> Result<Self> {
        let title = title.trim();
        if title.is_empty() {
            return Err(Error::Invalid("dê um título ao projeto".into()));
        }
        let root = with_extension(path);
        if root.exists()
            && fs::read_dir(&root)
                .map_err(|e| Error::io(&root, e))?
                .next()
                .is_some()
        {
            return Err(Error::Exists(root));
        }
        for sub in [OPS, SNAPSHOTS, DATA, ATTACHMENTS, MODELS, PRESENCE] {
            let d = root.join(sub);
            fs::create_dir_all(&d).map_err(|e| Error::io(&d, e))?;
        }
        let manifest = Manifest {
            id: clock.new_id(),
            title: title.to_string(),
            created_ms: clock.now_ms(),
            created_by: me.person,
            program: PROGRAM.to_string(),
        };
        let mpath = root.join(MANIFEST);
        fsio::write_new(&mpath, format::write(&manifest).as_bytes())?;
        let mut pf = Self::empty(root, manifest, me.clone(), cache_dir);
        let mut edits = initial(pf.manifest.id);
        edits.extend(Person::register(pf.state(), me.person, &me.name, &me.email));
        pf.commit(edits, clock)?;
        Ok(pf)
    }

    fn empty(root: PathBuf, manifest: Manifest, me: Identity, cache_dir: Option<&Path>) -> Self {
        Self {
            root,
            manifest,
            me,
            journal: Journal::new(),
            seen: HashSet::new(),
            pending: BTreeMap::new(),
            cache_dir: cache_dir.map(Path::to_path_buf),
            load: LoadInfo::default(),
            full: false,
        }
    }

    /// Creates a program folder (`docs/plano_lean_seis_sigma.md`, 8.9): the same journal,
    /// whose main entity is the `programa`, with its `mentoria/` subfolder for the coaching
    /// logs. It keeps every operation in memory ([`ProjectFolder::all_ops`]).
    pub fn create_program(
        path: &Path,
        name: &str,
        me: &Identity,
        clock: &dyn Clock,
    ) -> Result<Self> {
        let mut pf = Self::create_with(path, name, me, clock, None, |id| {
            utsim_lss_core::programa::create(id, name)
        })?;
        pf.full = true;
        let d = pf.root.join(crate::program::COACHING);
        fs::create_dir_all(&d).map_err(|e| Error::io(&d, e))?;
        Ok(pf)
    }

    /// Creates the coaching log folder of a belt (main entity `diario_mentoria`).
    pub fn create_coaching(
        path: &Path,
        belt: Id,
        program: Id,
        me: &Identity,
        clock: &dyn Clock,
    ) -> Result<Self> {
        let title = format!("Mentoria {}", belt.short());
        let mut pf = Self::create_with(path, &title, me, clock, None, |id| {
            let r = EntityRef::new(utsim_lss_core::Kind::CoachingLog, id);
            vec![
                Edit::new(r, utsim_lss_core::mentoria::fields::BELT, belt.to_string()),
                Edit::new(
                    r,
                    utsim_lss_core::mentoria::fields::PROGRAM,
                    program.to_string(),
                ),
            ]
        })?;
        pf.full = true;
        Ok(pf)
    }

    /// Creates a restricted training folder (main entity `pasta_formacao`): the question
    /// bank, the grades or a class folder of the program.
    pub fn create_training(
        path: &Path,
        folder: crate::program::TrainingFolder,
        program: Id,
        me: &Identity,
        clock: &dyn Clock,
    ) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let mut pf = Self::create_with(path, &folder.title(), me, clock, None, |id| {
            let r = EntityRef::new(utsim_lss_core::Kind::TrainingFolder, id);
            let mut e = vec![
                Edit::new(
                    r,
                    utsim_lss_core::formacao::fields::PROGRAM,
                    program.to_string(),
                ),
                Edit::new(r, utsim_lss_core::formacao::fields::KIND, folder.key()),
            ];
            if let crate::program::TrainingFolder::Class(c) = folder {
                e.push(Edit::new(
                    r,
                    utsim_lss_core::formacao::fields::CLASS,
                    c.to_string(),
                ));
            }
            e
        })?;
        pf.full = true;
        Ok(pf)
    }

    /// Opens a folder keeping every operation in memory, without snapshots nor cache.
    pub fn open_full(path: &Path, me: &Identity) -> Result<Self> {
        let root = path.to_path_buf();
        let mpath = root.join(MANIFEST);
        if !mpath.is_file() {
            return Err(Error::NotAProject(root));
        }
        let text = fsio::read_text(&mpath)?;
        let manifest = format::read_manifest(&text).map_err(|e| Error::format(&mpath, e))?;
        let mut pf = Self::empty(root, manifest, me.clone(), None);
        pf.full = true;
        pf.load_all();
        Ok(pf)
    }

    /// Every operation, in order. Complete only for a folder opened with
    /// [`ProjectFolder::open_full`] or created as a program or coaching log; otherwise the
    /// operations after the snapshot in use.
    pub fn all_ops(&self) -> impl Iterator<Item = &Op> {
        self.journal.tail()
    }

    /// The kind of the main entity (the manifest id).
    pub fn main_kind(&self) -> Option<utsim_lss_core::Kind> {
        self.state().entity(self.manifest.id).map(|e| e.kind)
    }

    /// Whether this folder is a program (its main entity is a `programa`).
    pub fn is_program(&self) -> bool {
        self.main_kind() == Some(utsim_lss_core::Kind::Program)
    }

    /// Opens an existing project folder. `cache_dir` is the local cache (outside the synced
    /// folder); without it, or when it is missing or stale, the folder alone is used.
    pub fn open(path: &Path, me: &Identity, cache_dir: Option<&Path>) -> Result<Self> {
        let root = path.to_path_buf();
        let mpath = root.join(MANIFEST);
        if !mpath.is_file() {
            return Err(Error::NotAProject(root));
        }
        let text = fsio::read_text(&mpath)?;
        let manifest = format::read_manifest(&text).map_err(|e| Error::format(&mpath, e))?;
        let mut pf = Self::empty(root, manifest, me.clone(), cache_dir);
        pf.load_initial();
        Ok(pf)
    }

    fn cache_file(&self) -> Option<PathBuf> {
        self.cache_dir
            .as_ref()
            .map(|d| d.join(self.manifest.id.to_string()).join("estado.json"))
    }

    /// The largest readable snapshot of the folder and of the cache.
    fn best_snapshot(&self) -> Option<(Snapshot, bool)> {
        let mut files = fsio::json_files(&self.root.join(SNAPSHOTS));
        files.sort_by_key(|p| std::cmp::Reverse(snapshot_count(p).unwrap_or(0)));
        let folder = files.iter().find_map(|p| {
            fsio::read_text(p)
                .ok()
                .and_then(|t| format::read_snapshot(&t).ok())
        });
        let cache = self.cache_file().and_then(|p| {
            fsio::read_text(&p)
                .ok()
                .and_then(|t| format::read_snapshot(&t).ok())
        });
        match (folder, cache) {
            (Some(f), Some(c)) if c.len() > f.len() => Some((c, true)),
            (Some(f), _) => Some((f, false)),
            (None, Some(c)) => Some((c, true)),
            (None, None) => None,
        }
    }

    fn op_files(&self) -> Vec<PathBuf> {
        fsio::json_files_deep(&self.root.join(OPS), 2)
    }

    fn read_op_file(&mut self, path: &Path) -> Option<Op> {
        let parsed = fsio::read_text(path)
            .and_then(|t| format::read_op(&t).map_err(|e| Error::format(path, e)));
        match parsed {
            Ok(op) => {
                self.seen.insert(path.to_path_buf());
                self.pending.remove(path);
                Some(op)
            }
            Err(e) => {
                self.pending.insert(path.to_path_buf(), e.to_string());
                None
            }
        }
    }

    fn load_initial(&mut self) {
        let snapshot = self.best_snapshot();
        let files = self.op_files();
        let mut tail = Vec::new();
        let mut read = 0;
        for f in &files {
            let covered = snapshot
                .as_ref()
                .is_some_and(|(s, _)| id_from_name(f).is_some_and(|id| s.includes(id)));
            if covered {
                self.seen.insert(f.clone());
                continue;
            }
            read += 1;
            if let Some(op) = self.read_op_file(f) {
                tail.push(op);
            }
        }
        match snapshot {
            Some((snap, from_cache)) => {
                let covered = snap.len();
                match Journal::from_snapshot(snap, tail) {
                    Ok(j) => {
                        self.journal = j;
                        self.load = LoadInfo {
                            from_snapshot: covered,
                            from_cache,
                            files_read: read,
                        };
                    }
                    Err(_) => self.load_all(),
                }
            }
            None => {
                self.journal = Journal::from_ops(tail);
                self.load = LoadInfo {
                    files_read: read,
                    ..LoadInfo::default()
                };
            }
        }
    }

    /// Rebuilds from every operation file, without snapshots.
    fn load_all(&mut self) {
        self.seen.clear();
        self.pending.clear();
        let files = self.op_files();
        let mut ops = Vec::new();
        for f in &files {
            if let Some(op) = self.read_op_file(f) {
                ops.push(op);
            }
        }
        self.journal = Journal::from_ops(ops);
        self.load = LoadInfo {
            files_read: files.len(),
            ..LoadInfo::default()
        };
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Id of the project entity.
    pub fn project_id(&self) -> Id {
        self.manifest.id
    }

    pub fn me(&self) -> &Identity {
        &self.me
    }

    pub fn state(&self) -> &State {
        self.journal.state()
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    pub fn load_info(&self) -> &LoadInfo {
        &self.load
    }

    /// Files not readable yet, with the reason.
    pub fn pending(&self) -> &BTreeMap<PathBuf, String> {
        &self.pending
    }

    /// The current project title (from the journal; the manifest's as fallback). For a
    /// portfolio folder, its name.
    pub fn title(&self) -> String {
        use utsim_lss_core::domain::fields;
        self.state()
            .entity(self.manifest.id)
            .map(|e| {
                let t = e.text(fields::project::TITLE);
                if t.is_empty() {
                    e.text(fields::portfolio::NAME)
                } else {
                    t
                }
            })
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| self.manifest.title.clone())
    }

    /// Whether this folder is a portfolio (its main entity is a `carteira`).
    pub fn is_portfolio(&self) -> bool {
        self.state()
            .entity(self.manifest.id)
            .is_some_and(|e| e.kind == utsim_lss_core::Kind::Portfolio)
    }

    /// Where an operation file goes: `ops/AAAA/MM/<uuid>-<pessoa>.json` (month of the id).
    pub fn op_path(&self, op: &Op) -> PathBuf {
        let d = Date::from_unix_ms(op.id.unix_ms() as i64, 0);
        self.root
            .join(OPS)
            .join(format!("{:04}", d.year))
            .join(format!("{:02}", d.month))
            .join(format!("{}-{}.json", op.id, op.author))
    }

    /// Turns edits into operations of this person, writes one file per operation (never
    /// replacing any) and applies them. Returns the operations written.
    pub fn commit(&mut self, edits: Vec<Edit>, clock: &dyn Clock) -> Result<Vec<Op>> {
        if edits.is_empty() {
            return Ok(Vec::new());
        }
        let ops = self
            .state()
            .prepare(self.me.person, clock.now_ms(), edits, || clock.new_id());
        for op in &ops {
            let path = self.op_path(op);
            fsio::write_new(&path, format::write(op).as_bytes())?;
            self.seen.insert(path);
            // Our own operations always come after everything known.
            let _ = self.journal.insert(op.clone());
        }
        Ok(ops)
    }

    /// Registers this person in the project (name and e-mail), if not there yet.
    pub fn register_me(&mut self, clock: &dyn Clock) -> Result<()> {
        let edits = Person::register(self.state(), self.me.person, &self.me.name, &self.me.email);
        self.commit(edits, clock).map(|_| ())
    }

    /// Picks up the operation files that appeared since the last look (other people's,
    /// synced in), retries the pending ones and writes a snapshot when the tail is long.
    pub fn refresh(&mut self, clock: &dyn Clock) -> Result<Refresh> {
        let mut report = Refresh::default();
        let mut full = false;
        for f in self.op_files() {
            if self.seen.contains(&f) {
                continue;
            }
            if let Some(op) = self.read_op_file(&f) {
                match self.journal.insert(op) {
                    Insert::Applied | Insert::Refolded => report.new_ops += 1,
                    Insert::Duplicate => {}
                    Insert::NeedsFullReplay => {
                        report.new_ops += 1;
                        full = true;
                    }
                }
            }
        }
        if full {
            self.load_all();
            report.full_replay = true;
        }
        report.pending = self.pending.len();
        if !self.full && self.journal.tail_len() >= SNAPSHOT_EVERY {
            report.snapshot = Some(self.write_snapshot(clock)?);
            // Following refreshes keep the in-memory journal; the snapshot serves the next
            // opening (here or elsewhere).
        }
        Ok(report)
    }

    /// Writes a snapshot of everything known into `instantaneos/` (a new file).
    pub fn write_snapshot(&mut self, clock: &dyn Clock) -> Result<PathBuf> {
        let snap = self.journal.snapshot();
        let path =
            self.root
                .join(SNAPSHOTS)
                .join(format!("{:06}-{}.json", snap.len(), clock.new_id()));
        fsio::write_new(&path, format::write(&snap).as_bytes())?;
        // Continue from it, so the next threshold counts from here.
        if let Ok(j) = Journal::from_snapshot(snap, Vec::new()) {
            self.journal = j;
        }
        Ok(path)
    }

    /// Saves the state to the local cache (outside the synced folder). It can be deleted at
    /// any time; the next opening just reads more files.
    pub fn save_cache(&self) -> Result<()> {
        let Some(path) = self.cache_file() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        fsio::write_replace(&path, format::write(&self.journal.snapshot()).as_bytes())
    }

    fn presence_file(&self) -> PathBuf {
        self.root
            .join(PRESENCE)
            .join(format!("{}.json", self.me.person))
    }

    /// Writes this person's heartbeat: on which screen and editing what.
    pub fn heartbeat(
        &self,
        clock: &dyn Clock,
        screen: &str,
        editing: Option<EntityRef>,
    ) -> Result<()> {
        let p = Presence {
            person: self.me.person,
            name: self.me.name.clone(),
            screen: screen.to_string(),
            editing,
            at_ms: clock.now_ms(),
        };
        fsio::write_replace(&self.presence_file(), format::write(&p).as_bytes())
    }

    /// Removes this person's presence (closing the project).
    pub fn leave(&self) {
        let _ = fs::remove_file(self.presence_file());
    }

    /// The other people with the project open now.
    pub fn others(&self, now_ms: i64) -> Vec<Presence> {
        let all: Vec<Presence> = fsio::json_files(&self.root.join(PRESENCE))
            .iter()
            .filter_map(|p| fsio::read_text(p).ok())
            .filter_map(|t| format::read::<Presence>(&t, "presença").ok())
            .collect();
        presence::others(&all, self.me.person, now_ms)
    }

    /// Writes a data file into `dados/<sha256>.<ext>` (once per content); returns the file
    /// name and the hash of the content.
    pub fn add_data(&self, bytes: &[u8], ext: &str) -> Result<(String, String)> {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let hash: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let name = format!("{hash}.{ext}");
        let target = self.root.join(DATA).join(&name);
        if !target.is_file() {
            fs::create_dir_all(self.root.join(DATA)).map_err(|e| Error::io(&self.root, e))?;
            fsio::write_new(&target, bytes)?;
        }
        Ok((name, hash))
    }

    /// The text of a data file of `dados/`.
    pub fn read_data(&self, file: &str) -> Result<String> {
        if file.contains(['/', '\\']) || file.contains("..") {
            return Err(Error::Invalid(format!("nome de arquivo inválido: {file}")));
        }
        fsio::read_text(&self.root.join(DATA).join(file))
    }

    /// Where a data file of `dados/` lives (to open it outside the app).
    pub fn data_path(&self, file: &str) -> PathBuf {
        self.root.join(DATA).join(file)
    }

    /// Copies a model into `modelos/<name>` (a new name when another content is there) and
    /// returns its path relative to the project folder.
    pub fn copy_model(&self, source: &Path, text: &str) -> Result<String> {
        let stem = source.file_stem().map_or_else(
            || "modelo".to_string(),
            |s| s.to_string_lossy().into_owned(),
        );
        let dir = self.root.join(MODELS);
        fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        for i in 0.. {
            let name = if i == 0 {
                format!("{stem}.ron")
            } else {
                format!("{stem}-{i}.ron")
            };
            let target = dir.join(&name);
            if target.is_file() {
                if fsio::read_text(&target).ok().as_deref() == Some(text) {
                    return Ok(format!("{MODELS}/{name}"));
                }
                continue;
            }
            fsio::write_new(&target, text.as_bytes())?;
            return Ok(format!("{MODELS}/{name}"));
        }
        unreachable!()
    }

    /// The absolute path of a model link's path (relative to the project, or absolute).
    pub fn model_path(&self, rel: &str) -> PathBuf {
        let p = Path::new(rel);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.root.join(p)
        }
    }

    /// Writes a report into `relatorios/` (replacing an older print of the same name);
    /// returns its path. Reports are made again at any time and are not in the journal.
    pub fn write_report(&self, name: &str, bytes: &[u8]) -> Result<PathBuf> {
        if name.contains(['/', '\\']) || name.contains("..") || name.is_empty() {
            return Err(Error::Invalid(format!("nome de arquivo inválido: {name}")));
        }
        let dir = self.root.join(REPORTS);
        fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let target = dir.join(name);
        fsio::write_replace(&target, bytes)?;
        Ok(target)
    }

    /// The text of the company's template of the reports: the project's own
    /// `modelos/relatorio.json`, else the one in the `modelos/` folder next to the projects.
    pub fn report_template(&self) -> Option<String> {
        let own = self.root.join(MODELS).join(REPORT_TEMPLATE);
        let shared = self
            .root
            .parent()
            .map(|p| p.join(MODELS).join(REPORT_TEMPLATE));
        [Some(own), shared]
            .into_iter()
            .flatten()
            .find(|p| p.is_file())
            .and_then(|p| fsio::read_text(&p).ok())
    }

    /// Saves the company's template next to the projects (`../modelos/relatorio.json`), so
    /// every project of the folder prints with it; returns the path.
    pub fn save_report_template(&self, json: &str) -> Result<PathBuf> {
        let dir = self
            .root
            .parent()
            .map_or_else(|| self.root.join(MODELS), |p| p.join(MODELS));
        fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        let target = dir.join(REPORT_TEMPLATE);
        fsio::write_replace(&target, json.as_bytes())?;
        Ok(target)
    }

    /// Copies a file into `anexos/<sha256>.<ext>` (once per content) and returns its name.
    pub fn add_attachment(&self, source: &Path) -> Result<String> {
        let mut f = fs::File::open(source).map_err(|e| Error::io(source, e))?;
        let mut hasher = Sha256::new();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes)
            .map_err(|e| Error::io(source, e))?;
        hasher.update(&bytes);
        let hash: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let ext = source
            .extension()
            .map(|e| e.to_string_lossy().to_ascii_lowercase())
            .filter(|e| {
                !e.is_empty() && e.len() <= 10 && e.chars().all(|c| c.is_ascii_alphanumeric())
            })
            .unwrap_or_else(|| "bin".into());
        let name = format!("{hash}.{ext}");
        let target = self.root.join(ATTACHMENTS).join(&name);
        match fsio::write_new(&target, &bytes) {
            Ok(()) | Err(Error::Exists(_)) => Ok(name),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(
            with_extension(Path::new("a/Linha 2")),
            Path::new("a/Linha 2.lss")
        );
        assert_eq!(with_extension(Path::new("a/x.LSS")), Path::new("a/x.LSS"));
        let id = Id::v7(5, [5; 10]);
        let p = PathBuf::from(format!("ops/2026/11/{id}-{} (1).json", Id::v7(1, [1; 10])));
        assert_eq!(id_from_name(&p), Some(id));
        assert_eq!(id_from_name(Path::new("x.json")), None);
        assert_eq!(snapshot_count(Path::new("000184-abc.json")), Some(184));
    }
}
