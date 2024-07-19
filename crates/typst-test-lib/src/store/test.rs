use std::fmt::Debug;
use std::fs::File;
use std::io::{self, Write};

use ecow::EcoString;
use typst::syntax::{FileId, Source, VirtualPath};

use super::vcs::Vcs;
use crate::store::project::{Resolver, TestTarget};
use crate::store::{Document, LoadError, SaveError};
use crate::test::id::Identifier;
use crate::test::ReferenceKind;
use crate::util;

pub mod collector;

/// A thin test handle for managing on-disk resources.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Test {
    id: Identifier,
    ref_kind: Option<ReferenceKind>,
    is_ignored: bool,
}

/// References for a test.
#[derive(Debug, Clone)]
pub enum References {
    /// An ephemeral reference script used to compile the reference document on
    /// the fly.
    Ephemeral(EcoString),

    /// Persistent references which are stored on disk.
    Persistent(Document),
}

impl Test {
    /// Generates a new test which does not exist on disk yet.
    pub fn new(id: Identifier) -> Self {
        Self {
            id,
            ref_kind: None,
            is_ignored: false,
        }
    }

    #[cfg(test)]
    pub fn new_test(id: Identifier, ref_kind: Option<ReferenceKind>, is_ignored: bool) -> Self {
        Self {
            id,
            ref_kind,
            is_ignored,
        }
    }

    /// Returns a reference to the identifier of this test.
    pub fn id(&self) -> &Identifier {
        &self.id
    }

    /// Returns a reference to the reference kind of this test.
    pub fn ref_kind(&self) -> Option<&ReferenceKind> {
        self.ref_kind.as_ref()
    }

    /// Returns whether this test is compared to a reference script.
    pub fn is_ephemeral(&self) -> bool {
        matches!(self.ref_kind, Some(ReferenceKind::Ephemeral))
    }

    /// Returns whether this test is compared to reference images directly.
    pub fn is_persistent(&self) -> bool {
        matches!(self.ref_kind, Some(ReferenceKind::Persistent))
    }

    /// Returns whether this test is not compared, but only compiled.
    pub fn is_compile_only(&self) -> bool {
        matches!(self.ref_kind, None)
    }

    /// Returns whether this test is marked as ignored.
    pub fn is_ignored(&self) -> bool {
        self.is_ignored
    }

    /// Creates a new test directly on disk.
    pub fn create<R: Resolver, V: Vcs>(
        resolver: &R,
        vcs: &V,
        id: Identifier,
        source: &str,
        references: Option<References>,
    ) -> Result<Self, SaveError> {
        let test_dir = resolver.resolve(&id, TestTarget::TestDir);
        util::fs::create_dir(test_dir, true)?;

        let mut file = File::options()
            .write(true)
            .create_new(true)
            .open(resolver.resolve(&id, TestTarget::TestScript))?;

        file.write_all(source.as_bytes())?;

        let ref_kind = match references {
            Some(References::Ephemeral(_)) => Some(ReferenceKind::Ephemeral),
            Some(References::Persistent(_)) => Some(ReferenceKind::Persistent),
            None => None,
        };

        let is_ignored = source
            .lines()
            .take_while(|&l| l.starts_with("///"))
            .filter(|l| {
                l.strip_prefix("///")
                    .is_some_and(|l| l.trim() == "[ignored]")
            })
            .next()
            .is_some();

        let test = Self {
            id,
            ref_kind,
            is_ignored,
        };

        match references {
            Some(References::Ephemeral(reference)) => {
                test.create_reference_script(resolver, reference.as_str())?;
            }
            Some(References::Persistent(reference)) => {
                test.create_reference_document(resolver, &reference)?;
            }
            None => {}
        }

        test.ignore_temporary_directories(resolver, vcs)?;

        Ok(test)
    }

    /// Creates this test's temporary directories.
    pub fn create_temporary_directories<R: Resolver>(&self, resolver: &R) -> io::Result<()> {
        if self.is_ephemeral() {
            util::fs::create_dir(resolver.resolve(&self.id, TestTarget::RefDir), true)?;
        }

        util::fs::create_dir(resolver.resolve(&self.id, TestTarget::OutDir), true)?;
        util::fs::create_dir(resolver.resolve(&self.id, TestTarget::DiffDir), true)?;
        Ok(())
    }

    /// Creates this test's reference script.
    pub fn create_reference_script<R: Resolver>(
        &self,
        resolver: &R,
        reference: &str,
    ) -> io::Result<()> {
        std::fs::write(resolver.resolve(&self.id, TestTarget::RefScript), reference)?;
        Ok(())
    }

    /// Creates this test's persistent references.
    pub fn create_reference_document<R: Resolver>(
        &self,
        resolver: &R,
        reference: &Document,
    ) -> Result<(), SaveError> {
        let ref_dir = resolver.resolve(&self.id, TestTarget::RefDir);
        util::fs::create_dir(ref_dir, true)?;
        reference.save(ref_dir)?;
        Ok(())
    }

    /// Deletes this test's directories and scripts.
    pub fn delete<R: Resolver>(&self, resolver: &R) -> io::Result<()> {
        self.delete_reference_documents(resolver)?;
        self.delete_reference_script(resolver)?;
        self.delete_temporary_directories(resolver)?;

        util::fs::remove_file(resolver.resolve(&self.id, TestTarget::TestScript))?;
        util::fs::remove_dir(resolver.resolve(&self.id, TestTarget::TestDir), true)?;

        Ok(())
    }

    /// Deletes this test's temporary directories.
    pub fn delete_temporary_directories<R: Resolver>(&self, resolver: &R) -> io::Result<()> {
        if self.is_ephemeral() {
            util::fs::remove_dir(resolver.resolve(&self.id, TestTarget::RefDir), true)?;
        }

        util::fs::remove_dir(resolver.resolve(&self.id, TestTarget::OutDir), true)?;
        util::fs::remove_dir(resolver.resolve(&self.id, TestTarget::DiffDir), true)?;
        Ok(())
    }

    /// Deletes this test's reference script.
    pub fn delete_reference_script<R: Resolver>(&self, resolver: &R) -> io::Result<()> {
        util::fs::remove_file(resolver.resolve(&self.id, TestTarget::RefScript))?;
        Ok(())
    }

    /// Deletes this test's persistent reference documents.
    pub fn delete_reference_documents<R: Resolver>(&self, resolver: &R) -> io::Result<()> {
        util::fs::remove_dir(resolver.resolve(&self.id, TestTarget::RefDir), true)?;
        Ok(())
    }

    /// Ignores this test's temporary directories in the vcs.
    pub fn ignore_temporary_directories<R: Resolver, V: Vcs>(
        &self,
        resolver: &R,
        vcs: &V,
    ) -> io::Result<()> {
        if self.is_ephemeral() {
            vcs.ignore_target(resolver, &self.id, TestTarget::RefDir)?;
        }

        vcs.ignore_target(resolver, &self.id, TestTarget::OutDir)?;
        vcs.ignore_target(resolver, &self.id, TestTarget::DiffDir)?;
        Ok(())
    }

    /// Ignores this test's persistent reference documents in the vcs.
    pub fn ignore_reference_documents<R: Resolver, V: Vcs>(
        &self,
        resolver: &R,
        vcs: &V,
    ) -> io::Result<()> {
        vcs.ignore_target(resolver, &self.id, TestTarget::RefDir)?;
        Ok(())
    }

    /// Ignores this test's persistent reference documents in the vcs.
    pub fn unignore_reference_documents<R: Resolver, V: Vcs>(
        &self,
        resolver: &R,
        vcs: &V,
    ) -> io::Result<()> {
        vcs.unignore_target(resolver, &self.id, TestTarget::RefDir)?;
        Ok(())
    }

    /// Removes any previous references and creates a reference script by
    /// copying the test script.
    pub fn make_ephemeral<R: Resolver, V: Vcs>(&mut self, resolver: &R, vcs: &V) -> io::Result<()> {
        self.delete_reference_script(resolver)?;
        self.delete_reference_documents(resolver)?;
        self.ignore_reference_documents(resolver, vcs)?;

        std::fs::copy(
            resolver.resolve(&self.id, TestTarget::TestScript),
            resolver.resolve(&self.id, TestTarget::RefScript),
        )?;

        self.ref_kind = Some(ReferenceKind::Ephemeral);
        Ok(())
    }

    /// Removes any previous references and creates a persistent references from the
    /// given pages.
    pub fn make_persistent<R: Resolver, V: Vcs>(
        &mut self,
        resolver: &R,
        vcs: &V,
        reference: &Document,
    ) -> Result<(), SaveError> {
        self.delete_reference_script(resolver)?;
        self.delete_reference_documents(resolver)?;
        self.create_reference_document(resolver, reference)?;
        self.unignore_reference_documents(resolver, vcs)?;

        self.ref_kind = Some(ReferenceKind::Persistent);
        Ok(())
    }

    /// Removes any previous references.
    pub fn make_compile_only<R: Resolver, V: Vcs>(
        &mut self,
        resolver: &R,
        _vcs: &V,
    ) -> io::Result<()> {
        self.delete_reference_documents(resolver)?;
        self.delete_reference_script(resolver)?;

        self.ref_kind = None;
        Ok(())
    }

    /// Loads the test script source of this test.
    pub fn load_source<R: Resolver>(&self, resolver: &R) -> io::Result<Source> {
        let test_script = resolver.resolve(&self.id, TestTarget::TestScript);

        Ok(Source::new(
            FileId::new(
                None,
                VirtualPath::new(
                    test_script
                        .strip_prefix(resolver.project_root())
                        .unwrap_or(test_script),
                ),
            ),
            std::fs::read_to_string(test_script)?,
        ))
    }

    /// Loads the reference test script source of this test, if one exists.
    pub fn load_reference_source<R: Resolver>(&self, resolver: &R) -> io::Result<Option<Source>> {
        match self.ref_kind {
            Some(ReferenceKind::Ephemeral) => {
                let ref_script = resolver.resolve(&self.id, TestTarget::RefScript);
                Ok(Some(Source::new(
                    FileId::new(
                        None,
                        VirtualPath::new(
                            ref_script
                                .strip_prefix(resolver.project_root())
                                .unwrap_or(ref_script),
                        ),
                    ),
                    std::fs::read_to_string(ref_script)?,
                )))
            }
            _ => Ok(None),
        }
    }

    /// Loads the persistent reference pages of this test, if they exist.
    pub fn load_reference_document<R: Resolver>(
        &self,
        resolver: &R,
    ) -> Result<Option<Document>, LoadError> {
        match self.ref_kind {
            Some(ReferenceKind::Persistent) => {
                Document::load(resolver.resolve(&self.id, TestTarget::RefDir)).map(Some)
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_dev;
    use crate::_dev::fs::Setup;
    use crate::store::project::v1::ResolverV1;
    use crate::store::vcs::NoVcs;

    fn setup_all(root: &mut Setup) -> &mut Setup {
        root.setup_file("tests/compile-only/test.typ", "Hello World")
            .setup_file("tests/ephemeral/test.typ", "Hello World")
            .setup_file("tests/ephemeral/ref.typ", "Hello\nWorld")
            .setup_file("tests/persistent/test.typ", "Hello World")
            .setup_dir("tests/persistent/ref")
    }

    #[test]
    fn test_create_new() {
        _dev::fs::TempEnv::run(
            |root| root.setup_dir("tests"),
            |root| {
                let project = ResolverV1::new(root, "tests");
                Test::create(
                    &project,
                    &NoVcs,
                    Identifier::new("compile-only").unwrap(),
                    "Hello World",
                    None,
                )
                .unwrap();

                Test::create(
                    &project,
                    &NoVcs,
                    Identifier::new("ephemeral").unwrap(),
                    "Hello World",
                    Some(References::Ephemeral("Hello\nWorld".into())),
                )
                .unwrap();

                Test::create(
                    &project,
                    &NoVcs,
                    Identifier::new("persistent").unwrap(),
                    "Hello World",
                    Some(References::Persistent(Document::new(vec![]))),
                )
                .unwrap();
            },
            |root| {
                root.expect_file("tests/compile-only/test.typ", "Hello World")
                    .expect_file("tests/ephemeral/test.typ", "Hello World")
                    .expect_file("tests/ephemeral/ref.typ", "Hello\nWorld")
                    .expect_file("tests/persistent/test.typ", "Hello World")
                    .expect_dir("tests/persistent/ref")
            },
        );
    }

    #[test]
    fn test_make_ephemeral() {
        _dev::fs::TempEnv::run(
            setup_all,
            |root| {
                let project = ResolverV1::new(root, "tests");
                Test::new(Identifier::new("compile-only").unwrap())
                    .make_ephemeral(&project, &NoVcs)
                    .unwrap();

                Test::new(Identifier::new("ephemeral").unwrap())
                    .make_ephemeral(&project, &NoVcs)
                    .unwrap();

                Test::new(Identifier::new("persistent").unwrap())
                    .make_ephemeral(&project, &NoVcs)
                    .unwrap();
            },
            |root| {
                root.expect_file("tests/compile-only/test.typ", "Hello World")
                    .expect_file("tests/compile-only/ref.typ", "Hello World")
                    .expect_file("tests/ephemeral/test.typ", "Hello World")
                    .expect_file("tests/ephemeral/ref.typ", "Hello World")
                    .expect_file("tests/persistent/test.typ", "Hello World")
                    .expect_file("tests/persistent/ref.typ", "Hello World")
            },
        );
    }

    #[test]
    fn test_make_persistent() {
        _dev::fs::TempEnv::run(
            setup_all,
            |root| {
                let project = ResolverV1::new(root, "tests");
                Test::new(Identifier::new("compile-only").unwrap())
                    .make_persistent(&project, &NoVcs, &Document::new(vec![]))
                    .unwrap();

                Test::new(Identifier::new("ephemeral").unwrap())
                    .make_persistent(&project, &NoVcs, &Document::new(vec![]))
                    .unwrap();

                Test::new(Identifier::new("persistent").unwrap())
                    .make_persistent(&project, &NoVcs, &Document::new(vec![]))
                    .unwrap();
            },
            |root| {
                root.expect_file("tests/compile-only/test.typ", "Hello World")
                    .expect_dir("tests/compile-only/ref")
                    .expect_file("tests/ephemeral/test.typ", "Hello World")
                    .expect_dir("tests/ephemeral/ref")
                    .expect_file("tests/persistent/test.typ", "Hello World")
                    .expect_dir("tests/persistent/ref")
            },
        );
    }

    #[test]
    fn test_make_compile_only() {
        _dev::fs::TempEnv::run(
            setup_all,
            |root| {
                let project = ResolverV1::new(root, "tests");
                Test::new(Identifier::new("compile-only").unwrap())
                    .make_compile_only(&project, &NoVcs)
                    .unwrap();

                Test::new(Identifier::new("ephemeral").unwrap())
                    .make_compile_only(&project, &NoVcs)
                    .unwrap();

                Test::new(Identifier::new("persistent").unwrap())
                    .make_compile_only(&project, &NoVcs)
                    .unwrap();
            },
            |root| {
                root.expect_file("tests/compile-only/test.typ", "Hello World")
                    .expect_file("tests/ephemeral/test.typ", "Hello World")
                    .expect_file("tests/persistent/test.typ", "Hello World")
            },
        );
    }

    #[test]
    fn test_load_sources() {
        _dev::fs::TempEnv::run_no_check(
            |root| {
                root.setup_file("tests/fancy/test.typ", "Hello World")
                    .setup_file("tests/fancy/ref.typ", "Hello\nWorld")
            },
            |root| {
                let project = ResolverV1::new(root, "tests");

                let test = Test {
                    id: Identifier::new("fancy").unwrap(),
                    ref_kind: Some(ReferenceKind::Ephemeral),
                    is_ignored: false,
                };

                test.load_source(&project).unwrap();
                test.load_reference_source(&project).unwrap().unwrap();
            },
        );
    }

    #[test]
    fn test_sources_virtual() {
        _dev::fs::TempEnv::run_no_check(
            |root| root.setup_file_empty("tests/fancy/test.typ"),
            |root| {
                let project = ResolverV1::new(root, "tests");

                let test = Test {
                    id: Identifier::new("fancy").unwrap(),
                    ref_kind: None,
                    is_ignored: false,
                };

                let source = test.load_source(&project).unwrap();
                assert_eq!(
                    source.id().vpath().resolve(root).unwrap(),
                    root.join("tests/fancy/test.typ")
                );
            },
        );
    }
}
