// SPDX-License-Identifier: Apache-2.0
// Credits: The Typst Authors

#![allow(dead_code)]

// TODO(tinger): Upstream this to typst-kit.

use std::path::PathBuf;

use typst::syntax::FileId;
use typst_kit::download::Downloader;
use typst_kit::fonts::FontSearcher;
use typst_kit::package::PackageStorage;
use typst_syntax::package::PackageSpec;
use typst_syntax::VirtualPath;
use tytanic_core::library::augmented_default_library;
use tytanic_core::world_builder::datetime::FixedDateProvider;
use tytanic_core::world_builder::file::FilesystemFileProvider;
use tytanic_core::world_builder::font::FilesystemFontProvider;
use tytanic_core::world_builder::library::LibraryProvider;
use tytanic_core::world_builder::ComposedWorld;
use tytanic_core::world_builder::ProvideDatetime;
use tytanic_core::world_builder::ProvideFile;
use tytanic_core::world_builder::ProvideFont;
use tytanic_core::world_builder::ProvideLibrary;
use tytanic_core::world_builder::TemplateFileProviderShim;
use tytanic_core::Project;
use tytanic_core::TemplateTest;
use tytanic_core::UnitTest;

use crate::cli::commands::CompileOptions;
use crate::cli::commands::FontOptions;
use crate::cli::commands::PackageOptions;
use crate::cli::commands::Switch;

#[tracing::instrument]
fn package_storage(package_opts: &PackageOptions) -> PackageStorage {
    let agent = format!("{}/{}", tytanic_core::TOOL_NAME, env!("CARGO_PKG_VERSION"));

    let downloader = match package_opts.certificate.clone() {
        Some(path) => Downloader::with_path(agent, path),
        None => Downloader::new(agent),
    };

    PackageStorage::new(
        package_opts.package_cache_path.clone(),
        package_opts.package_path.clone(),
        downloader,
    )
}

/// A file provider which is rooted at a project's root and provides access to
/// all files in that project as well as access to packages on demand.
#[tracing::instrument(skip(project))]
pub fn project_file_provider(
    project: &Project,
    package_opts: &PackageOptions,
) -> Box<dyn ProvideFile> {
    Box::new(FilesystemFileProvider::new(
        project.root(),
        Some(package_storage(package_opts)),
    )) as _
}

/// Provides access as if in a freshly created template from the given template
/// package project.
///
/// This means that only files within the template root can be accessed, not
/// the whole project. Additionally, imports to packages matching the current
/// template project's version and name in the `preview` namespace are routed
/// to the current package and are subject to the same access rules as a normal
/// package.
///
/// Panics if the project has no manifest.
#[tracing::instrument(skip(project))]
pub fn template_file_provider(
    project: &Project,
    package_opts: &PackageOptions,
) -> Box<dyn ProvideFile> {
    let manifest = project.manifest().unwrap();

    let spec = PackageSpec {
        namespace: "preview".into(),
        name: manifest.package.name.clone(),
        version: manifest.package.version,
    };

    Box::new(TemplateFileProviderShim::new(
        FilesystemFileProvider::new(project.root(), Some(package_storage(package_opts))),
        FilesystemFileProvider::new(
            project.template_root().unwrap(),
            Some(package_storage(package_opts)),
        ),
        spec,
    )) as _
}

/// A font provider that provides embedded and system fonts.
#[tracing::instrument]
pub fn font_provider(font_opts: &FontOptions) -> Box<dyn ProvideFont> {
    let mut searcher = FontSearcher::new();

    #[cfg(feature = "embed-fonts")]
    searcher.include_embedded_fonts(font_opts.use_embedded_fonts.get_or_default());
    searcher.include_system_fonts(font_opts.use_system_fonts.get_or_default());

    let fonts = searcher.search_with(font_opts.font_paths.iter().map(PathBuf::as_path));

    tracing::debug!(fonts = ?fonts.fonts.len(), "collected fonts");
    Box::new(FilesystemFontProvider::from_searcher(fonts))
}

/// A datetime provider that provides a fixed date.
#[tracing::instrument]
pub fn datetime_provider(compile_opts: &CompileOptions) -> Box<dyn ProvideDatetime> {
    Box::new(FixedDateProvider::new(compile_opts.timestamp)) as _
}

/// A library providers that provides the augmented library.
#[tracing::instrument]
pub fn augmented_library_provider() -> Box<dyn ProvideLibrary> {
    Box::new(LibraryProvider::with_library(augmented_default_library())) as _
}

/// A library providers that provides the default library.
#[tracing::instrument]
pub fn default_library_provider() -> Box<dyn ProvideLibrary> {
    Box::new(LibraryProvider::new()) as _
}

/// A set of providers used to construct worlds.
pub struct Providers {
    augmented_library: Box<dyn ProvideLibrary>,
    default_library: Box<dyn ProvideLibrary>,
    project_files: Box<dyn ProvideFile>,
    template_files: Option<Box<dyn ProvideFile>>,
    fonts: Box<dyn ProvideFont>,
    datetime: Box<dyn ProvideDatetime>,
}

impl Providers {
    pub fn new(
        project: &Project,
        package_opts: &PackageOptions,
        font_opts: &FontOptions,
        compile_opts: &CompileOptions,
    ) -> Self {
        Self {
            augmented_library: augmented_library_provider(),
            default_library: default_library_provider(),
            project_files: project_file_provider(project, package_opts),
            template_files: project.manifest().and_then(|m| {
                m.template
                    .is_some()
                    .then(|| template_file_provider(project, package_opts))
            }),
            fonts: font_provider(font_opts),
            datetime: datetime_provider(compile_opts),
        }
    }
}

impl Providers {
    /// Constructs a world for unit test creation.
    pub fn system_world(&self, id: FileId) -> ComposedWorld<'_> {
        ComposedWorld::builder()
            .library_provider(&*self.augmented_library)
            .file_provider(&*self.project_files)
            .font_provider(&*self.fonts)
            .datetime_provider(&*self.datetime)
            .build(id)
    }

    /// Constructs a world for unit tests.
    pub fn unit_world<'w>(
        &'w self,
        project: &Project,
        test: &'w UnitTest,
        is_ref: bool,
    ) -> ComposedWorld<'w> {
        // TODO(tinger): Implement more fail safe path handling to ensure we
        // don't use absolute paths here.
        let path = if is_ref {
            project.unit_test_ref_script(test.id())
        } else {
            project.unit_test_script(test.id())
        };

        let prefix = project.root();

        let id = FileId::new(
            None,
            VirtualPath::new(
                path.strip_prefix(prefix)
                    .expect("tests are in project root"),
            ),
        );

        ComposedWorld::builder()
            .library_provider(&*self.augmented_library)
            .file_provider(&*self.project_files)
            .font_provider(&*self.fonts)
            .datetime_provider(&*self.datetime)
            .build(id)
    }

    /// Constructs a world for template tests.
    ///
    /// Panics if the project has no manifest.
    pub fn template_world<'w>(
        &'w self,
        project: &Project,
        _test: &'w TemplateTest,
    ) -> ComposedWorld<'w> {
        // TODO(tinger): Implement more fail safe path handling to ensure we
        // don't use absolute paths here.
        let prefix = project.template_root().unwrap();
        let entrypoint = project.template_entrypoint().unwrap();
        let id = FileId::new(
            None,
            VirtualPath::new(
                entrypoint
                    .strip_prefix(prefix)
                    .expect("entrypoint is created with template root"),
            ),
        );

        ComposedWorld::builder()
            .library_provider(&*self.default_library)
            .file_provider(&**self.template_files.as_ref().unwrap())
            .font_provider(&*self.fonts)
            .datetime_provider(&*self.datetime)
            .build(id)
    }
}
