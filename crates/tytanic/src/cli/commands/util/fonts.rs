use std::io::Write;

use color_eyre::eyre;
use termcolor::Color;
use typst::text::FontStyle;

use crate::cli::Context;
use crate::cwrite;
use crate::cwriteln;
use crate::json::FontJson;
use crate::json::FontVariantJson;
use crate::ui::Indented;
use crate::world;

#[derive(clap::Args, Debug, Clone)]
#[group(id = "util-font-args")]
pub struct Args {
    /// List variants alongside fonts.
    ///
    /// Variants are listed as their weight, followed by their style and
    /// optionally their stretch, if it is not 1.
    #[arg(long)]
    pub variants: bool,

    /// Print a JSON describing the project to stdout.
    #[arg(long)]
    pub json: bool,
}

pub fn run(ctx: &mut Context, args: &Args) -> eyre::Result<()> {
    let fonts = world::font_provider(&ctx.args.font);

    let fonts = fonts
        .provide_font_book()
        .families()
        .map(|(name, info)| FontJson {
            name,
            variants: if args.variants {
                let mut variants = info
                    .map(|info| FontVariantJson {
                        weight: info.variant.weight.to_number(),
                        style: match info.variant.style {
                            FontStyle::Normal => "normal",
                            FontStyle::Italic => "italic",
                            FontStyle::Oblique => "oblique",
                        },
                        stretch: info.variant.stretch.to_ratio().get(),
                    })
                    .collect::<Vec<_>>();

                variants.sort_by_key(|v| v.weight);
                variants
            } else {
                vec![]
            },
        })
        .collect::<Vec<_>>();

    if args.json {
        serde_json::to_writer_pretty(ctx.ui.stdout(), &fonts)?;
        return Ok(());
    }

    let mut w = ctx.ui.stderr();

    for font in fonts {
        cwriteln!(bold_colored(w, Color::Cyan), "{}", font.name)?;

        let mut w = Indented::new(&mut w, 2);
        for variant in &font.variants {
            match variant.weight {
                0..700 => write!(w, "{}", variant.weight)?,
                700.. => cwrite!(bold(w), "{}", variant.weight)?,
            }

            write!(w, " ")?;

            match variant.style {
                "italic" | "oblique" => cwrite!(italic(w), "{}", variant.style)?,
                _ => write!(w, "normal")?,
            }

            if variant.stretch != 1.0 {
                write!(w, " {}", variant.stretch)?;
            }

            writeln!(w)?;
        }
    }

    Ok(())
}
