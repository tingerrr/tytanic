use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst_kit::fonts::{FontSlot, Fonts};

/// Provides access to fonts from memory.
pub struct VirtualFontProvider {
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
}

impl VirtualFontProvider {
    /// Creates a new font provider with the given fonts.
    pub fn new(book: FontBook, fonts: Vec<Font>) -> Self {
        Self {
            book: LazyHash::new(book),
            fonts,
        }
    }
}

impl VirtualFontProvider {
    /// The font book storing the font metadata.
    pub fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    /// The slots used to store the fonts.
    pub fn fonts(&self) -> &[Font] {
        &self.fonts
    }
}

impl VirtualFontProvider {
    /// Access the font with the given index.
    pub fn font(&self, index: usize) -> Option<&Font> {
        self.fonts.get(index)
    }
}

/// Provides access to fonts from the filesystem.
pub struct FilesystemFontProvider {
    book: LazyHash<FontBook>,
    fonts: Vec<FontSlot>,
}

impl FilesystemFontProvider {
    /// Creates a new font provider with the given fonts.
    pub fn new(book: FontBook, fonts: Vec<FontSlot>) -> Self {
        Self {
            book: LazyHash::new(book),
            fonts,
        }
    }

    /// Creates a new default font provider with the given font searcher result.
    pub fn from_searcher(fonts: Fonts) -> Self {
        Self::new(fonts.book, fonts.fonts)
    }
}

impl FilesystemFontProvider {
    /// The font book storing the font metadata.
    pub fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    /// The slots used to store the fonts.
    pub fn fonts(&self) -> &[FontSlot] {
        &self.fonts
    }
}

impl FilesystemFontProvider {
    /// Access the canonical slot for the font with the given index.
    pub fn font(&self, index: usize) -> Option<&FontSlot> {
        self.fonts.get(index)
    }
}
