use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use color_eyre::eyre;
use termcolor::Color;
use tytanic_core::suite::Suite;
use tytanic_core::test::Id;
use tytanic_core::Project;

use crate::cli::Context;
use crate::{cwrite, ui};

#[derive(clap::Args, Debug, Clone)]
#[group(id = "util-migrate-args")]
pub struct Args {
    /// Confirm the migration
    #[arg(long)]
    pub confirm: bool,

    /// The name of the new sub directories the tests get moved to
    #[arg(long, default_value = "self")]
    pub name: String,
}

pub fn run(ctx: &mut Context, args: &Args) -> eyre::Result<()> {
    let project = ctx.project()?;
    let suite = Suite::collect(&project)?;

    let mut w = ctx.ui.stderr();

    if suite.nested().is_empty() {
        writeln!(w, "No tests need to be moved")?;
        return Ok(());
    }

    if args.confirm {
        writeln!(w, "Moving tests:")?;
    } else {
        writeln!(w, "These tests would be moved:")?;
    }

    let mut has_colission = false;
    let mut mappings = BTreeMap::new();
    for old in suite.nested().keys() {
        let new = Id::new(format!("{old}/{}", args.name))?;
        let collision = suite.contains(&new);

        has_colission |= collision;
        mappings.insert(old.clone(), (new, collision));
    }

    for (old, (new, collision)) in &mappings {
        if *collision {
            cwrite!(bold_colored(w, Color::Red), "*")?;
            write!(w, " ")?;
        } else {
            write!(w, "  ")?;
        }
        ui::write_test_id(&mut w, old)?;
        write!(w, " -> ")?;
        ui::write_test_id(&mut w, new)?;
        writeln!(w)?;
    }

    writeln!(w)?;

    if has_colission {
        let mut w = ctx.ui.hint()?;
        cwrite!(bold_colored(w, Color::Red), "*")?;
        writeln!(
            w,
            " denotes paths which were excluded because of another test with the same id."
        )?;
        write!(w, "Try another name using ")?;
        cwrite!(colored(w, Color::Cyan), "--name")?;
        writeln!(w)?;
    }

    if args.confirm {
        for (old, (new, collision)) in &mappings {
            if *collision {
                continue;
            }

            migrate_test(&project, old, new)?;
        }
    } else {
        writeln!(ctx.ui.warn()?, "Make sure to back up your code!")?;

        {
            let mut w = ctx.ui.hint()?;
            write!(w, "Use ")?;
            cwrite!(colored(w, Color::Cyan), "--confirm")?;
            writeln!(w, " to move the tests automatically")?;
        }

        {
            let mut w = ctx.ui.hint()?;
            write!(w, "Use ")?;
            cwrite!(colored(w, Color::Cyan), "--name")?;
            writeln!(w, " to configure the sub directory name")?;
        }

        if project.vcs().is_some() {
            let mut w = ctx.ui.hint()?;
            write!(w, "VCS detected, consider also running ")?;
            cwrite!(colored(w, Color::Cyan), "tt util vcs ignore")?;
            writeln!(w, " after you've migrated")?;
        }
    }

    Ok(())
}

// NOTE(tinger): I have no idea why simply renaming the test directory doesn't
// work, but renaming the ref directory works

fn migrate_test_part(
    project: &Project,
    old: &Id,
    new: &Id,
    f: fn(&Project, &Id) -> PathBuf,
) -> eyre::Result<()> {
    let old = f(project, old);
    let new = f(project, new);

    if old.try_exists()? {
        fs::rename(&old, &new)?;
    }

    Ok(())
}

fn migrate_test(project: &Project, old: &Id, new: &Id) -> eyre::Result<()> {
    let test_dir = project.unit_test_dir(new);
    tytanic_utils::fs::create_dir(&test_dir, true)?;
    migrate_test_part(project, old, new, Project::unit_test_script)?;
    migrate_test_part(project, old, new, Project::unit_test_ref_script)?;
    migrate_test_part(project, old, new, Project::unit_test_ref_dir)?;
    let out_dir = project.unit_test_out_dir(old);
    tytanic_utils::fs::remove_dir(&out_dir, true)?;
    let diff_dir = project.unit_test_diff_dir(old);
    tytanic_utils::fs::remove_dir(&diff_dir, true)?;
    Ok(())
}
