use typst::utils::LazyHash;
use typst::Library;
use typst::LibraryBuilder;

use super::ProvideLibrary;

/// Provides access to a library.
#[derive(Debug)]
pub struct LibraryProvider {
    library: LazyHash<Library>,
}

impl LibraryProvider {
    /// Creates a new library provider with the default library.
    pub fn new() -> Self {
        Self::with_library(Library::default())
    }

    /// Creates a new library provider with the given library.
    pub fn with_library(library: Library) -> Self {
        Self {
            library: LazyHash::new(library),
        }
    }

    /// Creates a new library provider with the given library builder callback.
    pub fn with_builder(f: impl FnOnce(&mut LibraryBuilder) -> &mut LibraryBuilder) -> Self {
        let mut builder = Library::builder();
        f(&mut builder);
        Self::with_library(builder.build())
    }
}

impl LibraryProvider {
    /// The library.
    pub fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
}

impl Default for LibraryProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ProvideLibrary for LibraryProvider {
    fn provide_library(&self) -> &LazyHash<Library> {
        self.library()
    }
}
