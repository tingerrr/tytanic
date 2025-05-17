use std::collections::HashMap;
use std::fmt::Debug;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::MutexGuard;

use ecow::eco_format;
use typst::diag::FileError;
use typst::diag::FileResult;
use typst::diag::PackageError;
use typst::foundations::Bytes;
use typst::syntax::FileId;
use typst::syntax::Source;
use typst_kit::download::Progress;
use typst_kit::package::PackageStorage;

use super::ProvideFile;

/// Provides access to files from memory.
#[derive(Debug)]
pub struct VirtualFileProvider {
    slots: Mutex<HashMap<FileId, VirtualFileSlot>>,
}

impl VirtualFileProvider {
    /// Creates a new file provider with no files.
    pub fn new() -> Self {
        Self::from_slots(HashMap::new())
    }

    /// Creates a new file provider with the given file slots.
    pub fn from_slots(slots: HashMap<FileId, VirtualFileSlot>) -> Self {
        Self {
            slots: Mutex::new(slots),
        }
    }
}

impl VirtualFileProvider {
    /// The slots used to store file contents.
    pub fn slots(&self) -> MutexGuard<'_, HashMap<FileId, VirtualFileSlot>> {
        self.slots.lock().unwrap()
    }

    /// The slots used to store file contents.
    pub fn slots_mut(&mut self) -> &mut HashMap<FileId, VirtualFileSlot> {
        self.slots.get_mut().unwrap()
    }
}

impl VirtualFileProvider {
    /// Access the canonical slot for the given file id.
    pub fn slot<T, F>(&self, id: FileId, f: F) -> FileResult<T>
    where
        F: FnOnce(&VirtualFileSlot) -> T,
    {
        let map = self.slots.lock().unwrap();
        map.get(&id)
            .map(f)
            .ok_or_else(|| FileError::NotFound(id.vpath().as_rooted_path().to_owned()))
    }
}

impl Default for VirtualFileProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProvideFile for VirtualFileProvider {
    fn provide_source(&self, id: FileId, _progress: &mut dyn Progress) -> FileResult<Source> {
        self.slot(id, |slot| slot.source())?
            .ok_or_else(|| FileError::NotSource)
    }

    fn provide_bytes(&self, id: FileId, _progress: &mut dyn Progress) -> FileResult<Bytes> {
        self.slot(id, |slot| slot.bytes())
    }

    fn reset_all(&self) {}
}

/// Holds the processed data for a file ID.
///
/// Is eagerly populated with data (unlike [`FileSlot`]).
#[derive(Debug)]
pub struct VirtualFileSlot {
    id: FileId,
    source: Option<Source>,
    bytes: Bytes,
}

impl VirtualFileSlot {
    /// Create a new source file with the given source code.
    pub fn from_source(source: Source) -> Self {
        Self {
            id: source.id(),
            bytes: Bytes::new(source.text().as_bytes().to_vec()),
            source: Some(source),
        }
    }

    /// Create a new generic file with the given bytes.
    pub fn from_bytes<T>(id: FileId, bytes: T) -> Self
    where
        T: AsRef<[u8]> + Send + Sync + 'static,
    {
        Self {
            id,
            bytes: Bytes::new(bytes),
            source: None,
        }
    }
}

impl VirtualFileSlot {
    /// The file id of this file.
    pub fn id(&self) -> FileId {
        self.id
    }

    /// The optional source of this file.
    pub fn source(&self) -> Option<Source> {
        self.source.clone()
    }

    /// The bytes of this file.
    pub fn bytes(&self) -> Bytes {
        self.bytes.clone()
    }
}

/// Provides access to files from the filesystem.
#[derive(Debug)]
pub struct FilesystemFileProvider {
    root: PathBuf,
    slots: Mutex<HashMap<FileId, FileSlot>>,
    package_storage: Option<PackageStorage>,
}

impl FilesystemFileProvider {
    /// Creates a new file provider for the given project root.
    ///
    /// The package storage will be used to download and prepare packages.
    pub fn new<P>(root: P, package_storage: Option<PackageStorage>) -> Self
    where
        P: Into<PathBuf>,
    {
        Self {
            root: root.into(),
            slots: Mutex::new(HashMap::new()),
            package_storage,
        }
    }
}

impl FilesystemFileProvider {
    /// The project root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The slots used to store file contents.
    pub fn slots(&self) -> MutexGuard<'_, HashMap<FileId, FileSlot>> {
        self.slots.lock().unwrap()
    }

    /// The slots used to store file contents.
    pub fn slots_mut(&mut self) -> &mut HashMap<FileId, FileSlot> {
        self.slots.get_mut().unwrap()
    }

    /// The package storage if one is given.
    pub fn package_storage(&self) -> Option<&PackageStorage> {
        self.package_storage.as_ref()
    }
}

impl FilesystemFileProvider {
    /// Reset the slots for the next compilation.
    pub fn reset_slots(&self) {
        for slot in self.slots.lock().unwrap().values_mut() {
            slot.reset();
        }
    }

    /// Access the canonical slot for the given file id.
    pub fn slot<F, T>(&self, id: FileId, f: F) -> T
    where
        F: FnOnce(&mut FileSlot) -> T,
    {
        let mut map = self.slots.lock().unwrap();
        f(map.entry(id).or_insert_with(|| FileSlot::new(id)))
    }
}

impl ProvideFile for FilesystemFileProvider {
    fn provide_source(&self, id: FileId, progress: &mut dyn Progress) -> FileResult<Source> {
        self.slot(id, |slot| {
            slot.source(self.root(), self.package_storage(), progress)
        })
    }

    fn provide_bytes(&self, id: FileId, progress: &mut dyn Progress) -> FileResult<Bytes> {
        self.slot(id, |slot| {
            slot.bytes(self.root(), self.package_storage(), progress)
        })
    }

    fn reset_all(&self) {
        self.reset_slots();
    }
}

/// Holds the processed data for a file ID.
///
/// Both fields can be populated if the file is both imported and read().
#[derive(Debug)]
pub struct FileSlot {
    /// The slot's file id.
    id: FileId,
    /// The lazily loaded and incrementally updated source file.
    source: SlotCell<Source>,
    /// The lazily loaded raw byte buffer.
    file: SlotCell<Bytes>,
}

impl FileSlot {
    /// Create a new file slot.
    pub fn new(id: FileId) -> Self {
        Self {
            id,
            file: SlotCell::new(),
            source: SlotCell::new(),
        }
    }

    /// Marks the file as not yet accessed in preparation of the next
    /// compilation.
    pub fn reset(&mut self) {
        self.source.reset();
        self.file.reset();
    }

    /// Retrieve the source for this file.
    pub fn source(
        &mut self,
        root: &Path,
        package_storage: Option<&PackageStorage>,
        progress: &mut dyn Progress,
    ) -> FileResult<Source> {
        self.source.get_or_init(
            || read(self.id, root, package_storage, progress),
            |data, prev| {
                let text = decode_utf8(&data)?;
                if let Some(mut prev) = prev {
                    prev.replace(text);
                    Ok(prev)
                } else {
                    Ok(Source::new(self.id, text.into()))
                }
            },
        )
    }

    /// Retrieve the file's bytes.
    pub fn bytes(
        &mut self,
        root: &Path,
        package_storage: Option<&PackageStorage>,
        progress: &mut dyn Progress,
    ) -> FileResult<Bytes> {
        self.file.get_or_init(
            || read(self.id, root, package_storage, progress),
            |data, _| Ok(Bytes::new(data)),
        )
    }
}

/// Lazily processes data for a file.
#[derive(Debug)]
struct SlotCell<T> {
    /// The processed data.
    data: Option<FileResult<T>>,
    /// A hash of the raw file contents / access error.
    fingerprint: u128,
    /// Whether the slot has been accessed in the current compilation.
    accessed: bool,
}

impl<T: Clone> SlotCell<T> {
    /// Creates a new, empty cell.
    fn new() -> Self {
        Self {
            data: None,
            fingerprint: 0,
            accessed: false,
        }
    }

    /// Marks the cell as not yet accessed in preparation of the next
    /// compilation.
    fn reset(&mut self) {
        self.accessed = false;
    }

    /// Gets the contents of the cell or initialize them.
    fn get_or_init(
        &mut self,
        load: impl FnOnce() -> FileResult<Vec<u8>>,
        f: impl FnOnce(Vec<u8>, Option<T>) -> FileResult<T>,
    ) -> FileResult<T> {
        // If we accessed the file already in this compilation, retrieve it.
        if std::mem::replace(&mut self.accessed, true) {
            if let Some(data) = &self.data {
                return data.clone();
            }
        }

        // Read and hash the file.
        let result = load();
        let fingerprint = typst::utils::hash128(&result);

        // If the file contents didn't change, yield the old processed data.
        if std::mem::replace(&mut self.fingerprint, fingerprint) == fingerprint {
            if let Some(data) = &self.data {
                return data.clone();
            }
        }

        let prev = self.data.take().and_then(Result::ok);
        let value = result.and_then(|data| f(data, prev));
        self.data = Some(value.clone());

        value
    }
}

/// Resolves the path of a file id on the system, downloading a package if
/// necessary.
fn system_path(
    root: &Path,
    id: FileId,
    package_storage: Option<&PackageStorage>,
    progress: &mut dyn Progress,
) -> FileResult<PathBuf> {
    // Determine the root path relative to which the file path
    // will be resolved.
    let buf;
    let mut root = root;

    match (id.package(), package_storage) {
        (Some(spec), Some(storage)) => {
            tracing::trace!(?spec, "preparing package");
            buf = storage.prepare_package(spec, progress)?;
            root = &buf;
        }
        (Some(spec), None) => {
            tracing::error!(?spec, "cannot prepare package, no package storage provided");
            return Err(FileError::Package(PackageError::Other(Some(eco_format!(
                "cannot access package {spec}"
            )))));
        }
        (None, _) => {}
    }

    // Join the path to the root. If it tries to escape, deny
    // access. Note: It can still escape via symlinks.
    id.vpath().resolve(root).ok_or(FileError::AccessDenied)
}

/// Reads a file from a `FileId`.
///
/// If the ID represents stdin it will read from standard input,
/// otherwise it gets the file path of the ID and reads the file from disk.
fn read(
    id: FileId,
    root: &Path,
    package_storage: Option<&PackageStorage>,
    progress: &mut dyn Progress,
) -> FileResult<Vec<u8>> {
    read_from_disk(&system_path(root, id, package_storage, progress)?)
}

/// Read a file from disk.
fn read_from_disk(path: &Path) -> FileResult<Vec<u8>> {
    let f = |e| FileError::from_io(e, path);
    if std::fs::metadata(path).map_err(f)?.is_dir() {
        Err(FileError::IsDirectory)
    } else {
        std::fs::read(path).map_err(f)
    }
}

/// Decode UTF-8 with an optional BOM.
fn decode_utf8(buf: &[u8]) -> FileResult<&str> {
    // Remove UTF-8 BOM.
    Ok(std::str::from_utf8(
        buf.strip_prefix(b"\xef\xbb\xbf").unwrap_or(buf),
    )?)
}
