//! Test loading and on-disk manipulation.

use std::fmt::Debug;
use std::fs::{self, File};
use std::io::{self, Write};

use ecow::{EcoString, EcoVec};
use thiserror::Error;
use typst::syntax::{FileId, Source, VirtualPath};

use super::{Annotation, Id, ParseAnnotationError};
use crate::doc;
use crate::doc::{Document, SaveError};
use crate::project::{Project, Vcs};

// NOTE(tinger): the order of ignoring and deleting/creating documents is not
// random, this is specifically for VCS like jj with active watchman triggers
// and auto snapshotting.
//
// This is currently untested though.

/// The default test input as source code.
pub const DEFAULT_TEST_INPUT: &str = include_str!("default-test.typ");

/// The default test output as an encouded PNG.
pub const DEFAULT_TEST_OUTPUT: &[u8] = include_bytes!("default-test.png");

/// References for a test.
#[derive(Debug, Clone)]
pub enum Reference {
    /// An ephemeral reference script used to compile the reference document on
    /// the fly.
    Ephemeral(EcoString),

    /// Persistent references which are stored on disk.
    Persistent {
        /// The reference document.
        doc: Document,

        /// The optimization options to use when storing the document, `None`
        /// disabled optimization.
        opt: Option<Box<oxipng::Options>>,
    },
}

/// The kind of a unit test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// Test is compared to ephemeral references, these are compiled on the fly
    /// from a reference script.
    Ephemeral,

    /// Test is compared to persistent references, these are pre-compiled and
    /// loaded for comparison.
    Persistent,

    /// Test is only compiled.
    CompileOnly,
}

impl Kind {
    /// Whether this kind is is ephemeral.
    pub fn is_ephemeral(self) -> bool {
        matches!(self, Kind::Ephemeral)
    }

    /// Whether this kind is persistent.
    pub fn is_persistent(self) -> bool {
        matches!(self, Kind::Persistent)
    }

    /// Whether this kind is compile-only.
    pub fn is_compile_only(self) -> bool {
        matches!(self, Kind::CompileOnly)
    }

    /// Returns a kebab-case string representing this kind.
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Ephemeral => "ephemeral",
            Kind::Persistent => "persistent",
            Kind::CompileOnly => "compile-only",
        }
    }
}

impl Reference {
    /// The kind of this reference.
    pub fn kind(&self) -> Kind {
        match self {
            Self::Ephemeral(_) => Kind::Ephemeral,
            Self::Persistent { doc: _, opt: _ } => Kind::Persistent,
        }
    }
}

/// A standalone test script and its assocaited documents.
#[derive(Debug, Clone, PartialEq)]
pub struct Test {
    id: Id,
    kind: Kind,
    annotations: EcoVec<Annotation>,
}

impl Test {
    #[cfg(test)]
    pub(crate) fn new_test(id: Id, kind: Kind) -> Self {
        use ecow::eco_vec;

        Self {
            id,
            kind,
            annotations: eco_vec![],
        }
    }

    /// Attempt to load a test, returns `None` if no test could be found.
    #[tracing::instrument(skip(project))]
    pub fn load(project: &Project, id: Id) -> Result<Option<Test>, LoadError> {
        let test_script = project.unit_test_script(&id);

        if !test_script.try_exists()? {
            return Ok(None);
        }

        let kind = if project.unit_test_ref_script(&id).try_exists()? {
            Kind::Ephemeral
        } else if project.unit_test_ref_dir(&id).try_exists()? {
            Kind::Persistent
        } else {
            Kind::CompileOnly
        };

        let annotations = Annotation::collect(&fs::read_to_string(test_script)?)?;

        Ok(Some(Test {
            id,
            kind,
            annotations,
        }))
    }
}

impl Test {
    /// The id of this test.
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// The kind of this test.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// This test's annotations.
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    /// Whether this test has a `skip` annotation.
    pub fn is_skip(&self) -> bool {
        self.annotations.contains(&Annotation::Skip)
    }
}

impl Test {
    /// Creates a new test on disk, the kind is inferred from the passed
    /// reference and annotations are parsed from the test script.
    ///
    /// # Panics
    /// Panics if the given id is equal to the unique template test id.
    #[tracing::instrument(skip(project, vcs, source, reference))]
    pub fn create(
        project: &Project,
        vcs: Option<&Vcs>,
        id: Id,
        source: &str,
        reference: Option<Reference>,
    ) -> Result<Test, CreateError> {
        assert_ne!(id, Id::template());

        let test_dir = project.unit_test_dir(&id);
        tytanic_utils::fs::create_dir(test_dir, true)?;

        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(project.unit_test_script(&id))?;

        file.write_all(source.as_bytes())?;

        let kind = reference
            .as_ref()
            .map(Reference::kind)
            .unwrap_or(Kind::CompileOnly);

        let annotations = Annotation::collect(source)?;

        let this = Self {
            id,
            kind,
            annotations,
        };

        // ingore temporaries before creating any
        if let Some(vcs) = vcs {
            vcs.ignore(project, &this)?;
        }

        match reference {
            Some(Reference::Ephemeral(reference)) => {
                this.create_reference_script(project, reference.as_str())?;
            }
            Some(Reference::Persistent {
                doc: reference,
                opt: options,
            }) => {
                this.create_reference_document(project, &reference, options.as_deref())?;
            }
            None => {}
        }

        Ok(this)
    }

    /// Creates the temporary directories of this test.
    #[tracing::instrument(skip(project))]
    pub fn create_temporary_directories(&self, project: &Project) -> io::Result<()> {
        if self.kind.is_ephemeral() {
            tytanic_utils::fs::remove_dir(project.unit_test_ref_dir(&self.id), true)?;
            tytanic_utils::fs::create_dir(project.unit_test_ref_dir(&self.id), true)?;
        }

        tytanic_utils::fs::create_dir(project.unit_test_out_dir(&self.id), true)?;
        tytanic_utils::fs::create_dir(project.unit_test_diff_dir(&self.id), true)?;

        Ok(())
    }

    /// Creates the test script of this test, this will truncate the file if it
    /// already exists.
    #[tracing::instrument(skip(project, source))]
    pub fn create_script(&self, project: &Project, source: &str) -> io::Result<()> {
        std::fs::write(project.unit_test_script(&self.id), source)?;
        Ok(())
    }

    /// Creates reference script of this test, this will truncate the file if it
    /// already exists.
    #[tracing::instrument(skip(project, source))]
    pub fn create_reference_script(&self, project: &Project, source: &str) -> io::Result<()> {
        std::fs::write(project.unit_test_ref_script(&self.id), source)?;
        Ok(())
    }

    /// Creates the persistent reference document of this test.
    #[tracing::instrument(skip(project, reference, optimize_options))]
    pub fn create_reference_document(
        &self,
        project: &Project,
        reference: &Document,
        optimize_options: Option<&oxipng::Options>,
    ) -> Result<(), SaveError> {
        // NOTE(tinger): if there are already more pages than we want to create,
        // the surplus pages would persist and make every comparison fail due to
        // a page count mismatch, so we clear them to be sure.
        self.delete_reference_document(project)?;

        let ref_dir = project.unit_test_ref_dir(&self.id);
        tytanic_utils::fs::create_dir(&ref_dir, true)?;
        reference.save(&ref_dir, optimize_options)?;

        Ok(())
    }

    /// Deletes all directories and scripts of this test.
    #[tracing::instrument(skip(project))]
    pub fn delete(&self, project: &Project) -> io::Result<()> {
        self.delete_reference_document(project)?;
        self.delete_reference_script(project)?;
        self.delete_temporary_directories(project)?;

        tytanic_utils::fs::remove_file(project.unit_test_script(&self.id))?;
        tytanic_utils::fs::remove_dir(project.unit_test_dir(&self.id), true)?;

        Ok(())
    }

    /// Deletes the temporary directories of this test.
    #[tracing::instrument(skip(project))]
    pub fn delete_temporary_directories(&self, project: &Project) -> io::Result<()> {
        if !self.kind.is_compile_only() {
            tytanic_utils::fs::remove_dir(project.unit_test_ref_dir(&self.id), true)?;
        }

        tytanic_utils::fs::remove_dir(project.unit_test_out_dir(&self.id), true)?;
        tytanic_utils::fs::remove_dir(project.unit_test_diff_dir(&self.id), true)?;
        Ok(())
    }

    /// Deletes the test script of of this test.
    #[tracing::instrument(skip(project))]
    pub fn delete_script(&self, project: &Project) -> io::Result<()> {
        tytanic_utils::fs::remove_file(project.unit_test_script(&self.id))?;
        Ok(())
    }

    /// Deletes reference script of of this test.
    #[tracing::instrument(skip(project))]
    pub fn delete_reference_script(&self, project: &Project) -> io::Result<()> {
        tytanic_utils::fs::remove_file(project.unit_test_ref_script(&self.id))?;
        Ok(())
    }

    /// Deletes persistent reference document of this test.
    #[tracing::instrument(skip(project))]
    pub fn delete_reference_document(&self, project: &Project) -> io::Result<()> {
        tytanic_utils::fs::remove_dir(project.unit_test_ref_dir(&self.id), true)?;
        Ok(())
    }

    /// Removes any previous references, if they exist and creates a reference
    /// script by copying the test script.
    #[tracing::instrument(skip(project, vcs))]
    pub fn make_ephemeral(&mut self, project: &Project, vcs: Option<&Vcs>) -> io::Result<()> {
        self.kind = Kind::Ephemeral;

        // ensure deletion is recorded before ignore file is updated
        self.delete_reference_script(project)?;
        self.delete_reference_document(project)?;

        if let Some(vcs) = vcs {
            vcs.ignore(project, self)?;
        }

        // copy refernces after ignore file is updated
        std::fs::copy(
            project.unit_test_script(&self.id),
            project.unit_test_ref_script(&self.id),
        )?;

        Ok(())
    }

    /// Removes any previous references, if they exist and creates persistent
    /// references from the given pages.
    #[tracing::instrument(skip(project, vcs))]
    pub fn make_persistent(
        &mut self,
        project: &Project,
        vcs: Option<&Vcs>,
        reference: &Document,
        optimize_options: Option<&oxipng::Options>,
    ) -> Result<(), SaveError> {
        self.kind = Kind::Persistent;

        // ensure deletion/creation is recorded before ignore file is updated
        self.delete_reference_script(project)?;
        self.create_reference_document(project, reference, optimize_options)?;

        if let Some(vcs) = vcs {
            vcs.ignore(project, self)?;
        }

        Ok(())
    }

    /// Removes any previous references, if they exist.
    #[tracing::instrument(skip(project, vcs))]
    pub fn make_compile_only(&mut self, project: &Project, vcs: Option<&Vcs>) -> io::Result<()> {
        self.kind = Kind::CompileOnly;

        // ensure deletion is recorded before ignore file is updated
        self.delete_reference_document(project)?;
        self.delete_reference_script(project)?;

        if let Some(vcs) = vcs {
            vcs.ignore(project, self)?;
        }

        Ok(())
    }

    /// Loads the test script source of this test.
    #[tracing::instrument(skip(project))]
    pub fn load_source(&self, project: &Project) -> io::Result<Source> {
        let test_script = project.unit_test_script(&self.id);

        Ok(Source::new(
            FileId::new(
                None,
                VirtualPath::new(
                    test_script
                        .strip_prefix(project.root())
                        .unwrap_or(&test_script),
                ),
            ),
            std::fs::read_to_string(test_script)?,
        ))
    }

    /// Loads the reference test script source of this test, if this test is
    /// ephemeral.
    #[tracing::instrument(skip(project))]
    pub fn load_reference_source(&self, project: &Project) -> io::Result<Option<Source>> {
        if !self.kind().is_ephemeral() {
            return Ok(None);
        }

        let ref_script = project.unit_test_ref_script(&self.id);
        Ok(Some(Source::new(
            FileId::new(
                None,
                VirtualPath::new(
                    ref_script
                        .strip_prefix(project.root())
                        .unwrap_or(&ref_script),
                ),
            ),
            std::fs::read_to_string(ref_script)?,
        )))
    }

    /// Loads the test document of this test.
    #[tracing::instrument(skip(project))]
    pub fn load_document(&self, project: &Project) -> Result<Document, doc::LoadError> {
        Document::load(project.unit_test_out_dir(&self.id))
    }

    /// Loads the persistent reference document of this test.
    #[tracing::instrument(skip(project))]
    pub fn load_reference_document(&self, project: &Project) -> Result<Document, doc::LoadError> {
        Document::load(project.unit_test_ref_dir(&self.id))
    }
}

/// Returned by [`Test::create`].
#[derive(Debug, Error)]
pub enum CreateError {
    /// An error occurred while parsing a test annotation.
    #[error("an error occurred while parsing a test annotation")]
    Annotation(#[from] ParseAnnotationError),

    /// An error occurred while saving test files.
    #[error("an error occurred while saving test files")]
    Save(#[from] doc::SaveError),

    /// An io error occurred.
    #[error("an io error occurred")]
    Io(#[from] io::Error),
}

/// Returned by [`Test::load`].
#[derive(Debug, Error)]
pub enum LoadError {
    /// An error occurred while parsing a test annotation.
    #[error("an error occurred while parsing a test annotation")]
    Annotation(#[from] ParseAnnotationError),

    /// An io error occurred.
    #[error("an io error occurred")]
    Io(#[from] io::Error),
}

#[cfg(test)]
mod tests {
    use tytanic_utils::fs::{Setup, TempTestEnv};

    use super::*;

    fn id(id: &str) -> Id {
        Id::new(id).unwrap()
    }

    fn test(test_id: &str, kind: Kind) -> Test {
        Test::new_test(id(test_id), kind)
    }

    fn setup_all(root: &mut Setup) -> &mut Setup {
        root.setup_file("tests/compile-only/test.typ", "Hello World")
            .setup_file("tests/ephemeral/test.typ", "Hello World")
            .setup_file("tests/ephemeral/ref.typ", "Hello\nWorld")
            .setup_file("tests/persistent/test.typ", "Hello World")
            .setup_dir("tests/persistent/ref")
    }

    #[test]
    fn test_create() {
        TempTestEnv::run(
            |root| root.setup_dir("tests"),
            |root| {
                let project = Project::new(root);
                Test::create(&project, None, id("compile-only"), "Hello World", None).unwrap();

                Test::create(
                    &project,
                    None,
                    id("ephemeral"),
                    "Hello World",
                    Some(Reference::Ephemeral("Hello\nWorld".into())),
                )
                .unwrap();

                Test::create(
                    &project,
                    None,
                    id("persistent"),
                    "Hello World",
                    Some(Reference::Persistent {
                        doc: Document::new(vec![]),
                        opt: None,
                    }),
                )
                .unwrap();
            },
            |root| {
                root.expect_file_content("tests/compile-only/test.typ", "Hello World")
                    .expect_file_content("tests/ephemeral/test.typ", "Hello World")
                    .expect_file_content("tests/ephemeral/ref.typ", "Hello\nWorld")
                    .expect_file_content("tests/persistent/test.typ", "Hello World")
                    .expect_dir("tests/persistent/ref")
            },
        );
    }

    #[test]
    fn test_make_ephemeral() {
        TempTestEnv::run(
            setup_all,
            |root| {
                let project = Project::new(root);
                test("compile-only", Kind::CompileOnly)
                    .make_ephemeral(&project, None)
                    .unwrap();
                test("ephemeral", Kind::Ephemeral)
                    .make_ephemeral(&project, None)
                    .unwrap();
                test("persistent", Kind::Persistent)
                    .make_ephemeral(&project, None)
                    .unwrap();
            },
            |root| {
                root.expect_file_content("tests/compile-only/test.typ", "Hello World")
                    .expect_file_content("tests/compile-only/ref.typ", "Hello World")
                    .expect_file_content("tests/ephemeral/test.typ", "Hello World")
                    .expect_file_content("tests/ephemeral/ref.typ", "Hello World")
                    .expect_file_content("tests/persistent/test.typ", "Hello World")
                    .expect_file_content("tests/persistent/ref.typ", "Hello World")
            },
        );
    }

    #[test]
    fn test_make_persistent() {
        TempTestEnv::run(
            setup_all,
            |root| {
                let project = Project::new(root);
                test("compile-only", Kind::CompileOnly)
                    .make_persistent(&project, None, &Document::new([]), None)
                    .unwrap();

                test("ephemeral", Kind::Ephemeral)
                    .make_persistent(&project, None, &Document::new([]), None)
                    .unwrap();

                test("persistent", Kind::Persistent)
                    .make_persistent(&project, None, &Document::new([]), None)
                    .unwrap();
            },
            |root| {
                root.expect_file_content("tests/compile-only/test.typ", "Hello World")
                    .expect_dir("tests/compile-only/ref")
                    .expect_file_content("tests/ephemeral/test.typ", "Hello World")
                    .expect_dir("tests/ephemeral/ref")
                    .expect_file_content("tests/persistent/test.typ", "Hello World")
                    .expect_dir("tests/persistent/ref")
            },
        );
    }

    #[test]
    fn test_make_compile_only() {
        TempTestEnv::run(
            setup_all,
            |root| {
                let project = Project::new(root);
                test("compile-only", Kind::CompileOnly)
                    .make_compile_only(&project, None)
                    .unwrap();

                test("ephemeral", Kind::Ephemeral)
                    .make_compile_only(&project, None)
                    .unwrap();

                test("persistent", Kind::Persistent)
                    .make_compile_only(&project, None)
                    .unwrap();
            },
            |root| {
                root.expect_file_content("tests/compile-only/test.typ", "Hello World")
                    .expect_file_content("tests/ephemeral/test.typ", "Hello World")
                    .expect_file_content("tests/persistent/test.typ", "Hello World")
            },
        );
    }

    #[test]
    fn test_load_sources() {
        TempTestEnv::run_no_check(
            |root| {
                root.setup_file("tests/fancy/test.typ", "Hello World")
                    .setup_file("tests/fancy/ref.typ", "Hello\nWorld")
            },
            |root| {
                let project = Project::new(root);

                let mut test = test("fancy", Kind::Ephemeral);
                test.kind = Kind::Ephemeral;

                test.load_source(&project).unwrap();
                test.load_reference_source(&project).unwrap().unwrap();
            },
        );
    }

    #[test]
    fn test_sources_virtual() {
        TempTestEnv::run_no_check(
            |root| root.setup_file_empty("tests/fancy/test.typ"),
            |root| {
                let project = Project::new(root);

                let test = test("fancy", Kind::CompileOnly);

                let source = test.load_source(&project).unwrap();
                assert_eq!(
                    source.id().vpath().resolve(root).unwrap(),
                    root.join("tests/fancy/test.typ")
                );
            },
        );
    }
}
