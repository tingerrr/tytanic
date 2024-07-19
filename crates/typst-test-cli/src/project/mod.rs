use std::collections::BTreeMap;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::{fs, io};

use rayon::prelude::*;
use tiny_skia::Pixmap;
use typst_project::manifest::Manifest;
use typst_test_lib::config::Config;
use typst_test_lib::store::project::v1::ResolverV1;
use typst_test_lib::store::project::Resolver;
use typst_test_lib::store::test::collector::Collector;
use typst_test_lib::store::test::matcher::Matcher;
use typst_test_lib::store::test::{References, Test};
use typst_test_lib::store::vcs::{Git, NoVcs};
use typst_test_lib::store::Document;
use typst_test_lib::test::id::Identifier;
use typst_test_lib::test::ReferenceKind;

use crate::util;

pub mod test;

const DEFAULT_TEST_INPUT: &str = include_str!("../../../../assets/default-test/test.typ");
const DEFAULT_TEST_OUTPUT: &[u8] = include_bytes!("../../../../assets/default-test/test.png");

pub fn try_open_manifest(root: &Path) -> Result<Option<Manifest>, Error> {
    if is_project_root(root)? {
        let content = std::fs::read_to_string(root.join(typst_project::heuristics::MANIFEST_FILE))?;
        let manifest = Manifest::from_str(&content)?;
        Ok(Some(manifest))
    } else {
        Ok(None)
    }
}

pub fn is_project_root(path: &Path) -> io::Result<bool> {
    typst_project::is_project_root(path)
}

pub fn try_find_project_root(path: &Path) -> io::Result<Option<&Path>> {
    typst_project::try_find_project_root(path)
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct ScaffoldOptions: u32 {
        /// Create a default example test.
        const EXAMPLE = 0;
    }
}

#[derive(Debug)]
pub struct Project {
    config: Config,
    manifest: Option<Manifest>,
    resovler: ResolverV1,
    vcs: Option<Git>,
    tests: BTreeMap<Identifier, Test>,
    filtered: BTreeMap<Identifier, Test>,
    template: Option<String>,
}

impl Project {
    pub fn new(root: PathBuf, config: Config, manifest: Option<Manifest>) -> Self {
        let resovler = ResolverV1::new(root, &config.tests_root);
        Self {
            config,
            manifest,
            resovler,
            // TODO: vcs support
            vcs: None,
            tests: BTreeMap::new(),
            filtered: BTreeMap::new(),
            template: None,
        }
    }

    pub fn name(&self) -> &str {
        self.manifest
            .as_ref()
            .map(|m| &m.package.name[..])
            .unwrap_or("<unknown package>")
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    pub fn manifest(&self) -> Option<&Manifest> {
        self.manifest.as_ref()
    }

    pub fn matched(&self) -> &BTreeMap<Identifier, Test> {
        &self.tests
    }

    pub fn tests_mut(&mut self) -> &mut BTreeMap<Identifier, Test> {
        &mut self.tests
    }

    pub fn filtered(&self) -> &BTreeMap<Identifier, Test> {
        &self.filtered
    }

    pub fn filtered_mut(&mut self) -> &mut BTreeMap<Identifier, Test> {
        &mut self.filtered
    }

    pub fn template_path(&self) -> Option<PathBuf> {
        self.config
            .template
            .as_ref()
            .map(|t| self.resovler.project_root().join(t))
    }

    pub fn template(&self) -> Option<&str> {
        self.template.as_deref()
    }

    pub fn resolver(&self) -> &ResolverV1 {
        &self.resovler
    }

    pub fn root(&self) -> &Path {
        self.resovler.project_root()
    }

    pub fn tests_root(&self) -> &Path {
        self.resovler.test_root()
    }

    pub fn root_exists(&self) -> io::Result<bool> {
        self.resovler.project_root().try_exists()
    }

    pub fn test_root_exists(&self) -> io::Result<bool> {
        self.resovler.test_root().try_exists()
    }

    pub fn unique_test(&self) -> Result<&Test, ()> {
        if self.tests.len() != 1 {
            return Err(());
        }

        let (_, test) = self.tests.first_key_value().ok_or(())?;

        Ok(test)
    }

    pub fn is_init(&self) -> io::Result<bool> {
        self.test_root_exists()
    }

    pub fn init(&mut self, options: ScaffoldOptions) -> Result<(), Error> {
        let tests_root_dir = self.tests_root();
        tracing::trace!(path = ?tests_root_dir, "creating tests root dir");
        util::fs::create_dir(&tests_root_dir, false)?;

        if options.contains(ScaffoldOptions::EXAMPLE) {
            tracing::debug!("adding default test");
            self.create_test(
                Identifier::new("example").unwrap(),
                Some(ReferenceKind::Persistent),
                false,
            )?;
            Ok(())
        } else {
            tracing::debug!("skipping default test");
            Ok(())
        }
    }

    pub fn uninit(&self) -> Result<(), Error> {
        util::fs::remove_dir(self.tests_root(), true)?;
        Ok(())
    }

    pub fn clean_artifacts(&self) -> Result<(), Error> {
        self.tests
            .par_iter()
            .try_for_each(|(_, test)| test.delete_temporary_directories(&self.resovler))?;

        Ok(())
    }

    pub fn load_template(&mut self) -> Result<(), Error> {
        if let Some(template) = self.template_path() {
            match fs::read_to_string(template) {
                Ok(template) => self.template = Some(template),
                Err(err) if err.kind() == io::ErrorKind::NotFound => {}
                Err(err) => return Err(Error::Io(err)),
            }
        }

        Ok(())
    }

    pub fn create_test(
        &mut self,
        id: Identifier,
        kind: Option<ReferenceKind>,
        use_template: bool,
    ) -> Result<(), Error> {
        if self.tests.contains_key(&id) {
            return Err(Error::TestAlreadyExists(id));
        }

        let source = if !use_template {
            DEFAULT_TEST_INPUT
        } else if let Some(template) = &self.template {
            template
        } else {
            DEFAULT_TEST_INPUT
        };

        let reference = match kind {
            Some(ReferenceKind::Ephemeral) => Some(References::Ephemeral(source.into())),
            Some(ReferenceKind::Persistent) if use_template && self.template.is_some() => {
                Some(References::Persistent(Document::new(vec![
                    Pixmap::decode_png(DEFAULT_TEST_OUTPUT).unwrap(),
                ])))
            }
            Some(ReferenceKind::Persistent) => {
                todo!("compile ")
            }
            None => None,
        };

        // TODO: error handling
        let test = if let Some(git) = &self.vcs {
            Test::create(&self.resovler, git, id, source, reference)
        } else {
            Test::create(&self.resovler, &NoVcs, id, source, reference)
        }
        .unwrap();

        self.tests.insert(test.id().clone(), test);

        Ok(())
    }

    pub fn delete_tests(&mut self) -> Result<(), Error> {
        self.tests
            .par_iter()
            .try_for_each(|(_, test)| test.delete(&self.resovler))?;

        self.tests.clear();
        Ok(())
    }

    pub fn collect_tests(&mut self, matcher: Matcher) -> Result<(), Error> {
        // TODO: error handling
        let mut collector = Collector::new(&self.resovler);
        collector.with_matcher(matcher);
        collector.collect();
        self.tests = collector.take_tests();

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid manifest")]
    InvalidManifest(#[from] toml::de::Error),

    #[error("test already exsits: {0:?}")]
    TestAlreadyExists(Identifier),

    #[error("an io error occurred")]
    Io(#[from] io::Error),
}
