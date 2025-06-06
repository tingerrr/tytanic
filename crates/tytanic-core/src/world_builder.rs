use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime};
use typst::syntax::{FileId, Source};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use typst_kit::download::{Progress, ProgressSink};
use typst_kit::fonts::FontSlot;

use self::file::{FilesystemFileProvider, VirtualFileProvider};
use self::font::{FilesystemFontProvider, VirtualFontProvider};
use self::library::DefaultLibraryProvider;
use self::time::{FixedDateProvider, SystemDateProvider};

pub mod file;
pub mod font;
pub mod library;
pub mod time;

/// A trait for providing access to files.
pub trait ProvideFile: Send + Sync {
    /// Provides a Typst source with the given file id.
    ///
    /// This may download a package, for which the progress callbacks will be
    /// used.
    fn provide_source(&self, id: FileId, progress: &mut dyn Progress) -> FileResult<Source>;

    /// Provides a generic file with the given file id.
    ///
    /// This may download a package, for which the progress callbacks will be
    /// used.
    fn provide_bytes(&self, id: FileId, progress: &mut dyn Progress) -> FileResult<Bytes>;

    /// Reset the cached files for the next compilation.
    fn reset_all(&self);
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

/// A trait for providing access to fonts.
pub trait ProvideFont: Send + Sync {
    /// Provides the font book which stores metadata about fonts.
    fn provide_font_book(&self) -> &LazyHash<FontBook>;

    /// Provides a font with the given index.
    fn provide_font(&self, index: usize) -> Option<Font>;
}

impl ProvideFont for VirtualFontProvider {
    fn provide_font_book(&self) -> &LazyHash<FontBook> {
        self.book()
    }

    fn provide_font(&self, index: usize) -> Option<Font> {
        self.font(index).map(Font::clone)
    }
}

impl ProvideFont for FilesystemFontProvider {
    fn provide_font_book(&self) -> &LazyHash<FontBook> {
        self.book()
    }

    fn provide_font(&self, index: usize) -> Option<Font> {
        self.font(index).and_then(FontSlot::get)
    }
}

/// A trait for providing access to libraries.
pub trait ProvideLibrary: Send + Sync {
    /// Provides the library.
    fn provide_library(&self) -> &LazyHash<Library>;
}

impl ProvideLibrary for DefaultLibraryProvider {
    fn provide_library(&self) -> &LazyHash<Library> {
        self.library()
    }
}

/// A trait for providing access to date.
pub trait ProvideDatetime: Send + Sync {
    /// Provides the current date.
    ///
    /// If no offset is specified, the local date should be chosen. Otherwise,
    /// the UTC date should be chosen with the corresponding offset in hours.
    ///
    /// If this function returns `None`, Typst's `datetime` function will
    /// return an error.
    ///
    /// Note that most implementations should provide a date only or only very
    /// course time increments to ensure Typst's incremental compilation cache
    /// is not disrupted too much.
    fn provide_today(&self, offset: Option<i64>) -> Option<Datetime>;

    /// Reset the current date for the next compilation.
    ///
    /// Note that this is only relevant for those providers which actually
    /// provide the current date.
    fn reset_today(&self);
}

impl ProvideDatetime for SystemDateProvider {
    fn provide_today(&self, offset: Option<i64>) -> Option<Datetime> {
        self.today_with_offset(offset)
    }

    fn reset_today(&self) {
        self.reset();
    }
}

impl ProvideDatetime for FixedDateProvider {
    fn provide_today(&self, offset: Option<i64>) -> Option<Datetime> {
        self.date_with_offset(offset)
    }

    fn reset_today(&self) {}
}

/// A shim around the various provider traits which can be used to compile a
/// test.
pub struct TestWorldBuilder<'w> {
    files: Option<&'w dyn ProvideFile>,
    fonts: Option<&'w dyn ProvideFont>,
    library: Option<&'w dyn ProvideLibrary>,
    datetime: Option<&'w dyn ProvideDatetime>,
    test_id: FileId,
}

impl TestWorldBuilder<'_> {
    /// Creates a new test world builder with the given test file id and no
    /// providers.
    pub fn new(test_id: FileId) -> Self {
        Self {
            files: None,
            fonts: None,
            library: None,
            datetime: None,
            test_id,
        }
    }
}

impl<'w> TestWorldBuilder<'w> {
    /// Configure the file provider.
    pub fn file_provider(self, value: &'w dyn ProvideFile) -> Self {
        Self {
            files: value.into(),
            ..self
        }
    }

    /// Configure the font provider.
    pub fn font_provider(self, value: &'w dyn ProvideFont) -> Self {
        Self {
            fonts: value.into(),
            ..self
        }
    }

    /// Configure the library provider.
    pub fn library_provider(self, value: &'w dyn ProvideLibrary) -> Self {
        Self {
            library: value.into(),
            ..self
        }
    }

    /// Configure the datetime provider.
    pub fn datetime_provider(self, value: &'w dyn ProvideDatetime) -> Self {
        Self {
            datetime: value.into(),
            ..self
        }
    }

    /// Build the test world with the configured providers.
    ///
    /// Returns `None` if a provider is missing.
    pub fn build(self) -> Option<TestWorld<'w>> {
        Some(TestWorld {
            files: self.files?,
            fonts: self.fonts?,
            library: self.library?,
            datetime: self.datetime?,
            test_id: self.test_id,
        })
    }
}

/// A shim around the various provider traits which can be used to compile a
/// test.
pub struct TestWorld<'w> {
    files: &'w dyn ProvideFile,
    fonts: &'w dyn ProvideFont,
    library: &'w dyn ProvideLibrary,
    datetime: &'w dyn ProvideDatetime,
    test_id: FileId,
}

impl<'w> TestWorld<'w> {
    /// Creates a new test world builder with the given test file id and no
    /// providers.
    pub fn builder(test_id: FileId) -> TestWorldBuilder<'w> {
        TestWorldBuilder::new(test_id)
    }
}

impl TestWorld<'_> {
    /// Resets the inner providers.
    pub fn reset(&self) {
        // TODO(tinger): We probably really want exclusive access here, no
        // provider should be used while it's being reset.
        self.files.reset_all();
        self.datetime.reset_today();
    }
}

impl World for TestWorld<'_> {
    fn library(&self) -> &LazyHash<Library> {
        self.library.provide_library()
    }

    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.provide_font_book()
    }

    fn main(&self) -> FileId {
        self.test_id
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        self.files.provide_source(id, &mut ProgressSink)
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.provide_bytes(id, &mut ProgressSink)
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.provide_font(index)
    }

    fn today(&self, offset: Option<i64>) -> Option<Datetime> {
        self.datetime.provide_today(offset)
    }
}

#[cfg(test)]
pub(crate) fn virtual_world<'w>(
    source: Source,
    files: &'w mut VirtualFileProvider,
    library: &'w DefaultLibraryProvider,
) -> TestWorld<'w> {
    use std::sync::LazyLock;

    use chrono::DateTime;

    use self::file::VirtualFileSlot;

    static FONTS: LazyLock<VirtualFontProvider> = LazyLock::new(|| {
        let fonts: Vec<_> = typst_assets::fonts()
            .flat_map(|data| Font::iter(Bytes::new(data)))
            .collect();

        let book = FontBook::from_fonts(&fonts);
        VirtualFontProvider::new(book, fonts)
    });

    static TIME: LazyLock<FixedDateProvider> =
        LazyLock::new(|| FixedDateProvider::new(DateTime::from_timestamp(0, 0).unwrap()));

    files
        .slots_mut()
        .insert(source.id(), VirtualFileSlot::from_source(source.clone()));

    TestWorld::builder(source.id())
        .file_provider(files)
        .library_provider(library)
        .font_provider(&*FONTS)
        .datetime_provider(&*TIME)
        .build()
        .unwrap()
}
