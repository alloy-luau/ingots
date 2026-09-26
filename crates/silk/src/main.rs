//! The Silk ingot: the protocol side of the library.

use alloy_ingot::{
    Color, ColorInfo, Completions, Edit, File, Finding, Handler, Hover, Settings, serve,
};
use silk::{css, editor, emit, enamel};

struct Silk {
    opts: emit::Options,
    /// The project's `enamel.aly`, when the project loads Enamel.
    theme_file: Option<std::path::PathBuf>,
}

impl Silk {
    /// Reads the theme again, since the editor changes it between files.
    fn refresh_theme(&mut self) {
        if let Some(path) = &self.theme_file {
            self.opts.theme = std::fs::read_to_string(path)
                .map(|t| enamel::theme(&t))
                .unwrap_or_default();
        }
    }
}

impl Handler for Silk {
    fn init(&mut self, settings: &Settings) -> Result<(), String> {
        let o = &settings.options;
        let text = |k: &str| o.get(k).and_then(|v| v.as_str()).map(str::to_string);
        let flag = |k: &str, default: bool| {
            o.get(k)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(default)
        };

        self.opts.roblox = flag("roblox", true);
        self.opts.tags = flag("tags", true);
        self.opts.enamel = settings.ingots.iter().any(|n| n == "enamel");
        self.theme_file = (self.opts.enamel && !settings.root.is_empty())
            .then(|| std::path::Path::new(&settings.root).join("enamel.aly"));

        if let Some(h) = text("helper") {
            self.opts.helper = h;
        }

        if let Some(f) = text("font") {
            self.opts.fonts.sans = f;
        }

        if let Some(f) = text("font_serif") {
            self.opts.fonts.serif = f;
        }

        if let Some(f) = text("font_mono") {
            self.opts.fonts.mono = f;
        }

        if let Some(c) = text("color") {
            self.opts.color = css::color(&c)
                .ok_or_else(|| format!("[ingot.silk] color = \"{c}\" is not a CSS color"))?;
        }

        Ok(())
    }

    fn transform(&mut self, file: &File) -> Result<Vec<Edit>, String> {
        if file.kind != "alx" {
            return Ok(Vec::new());
        }

        self.refresh_theme();

        Ok(emit::run(&file.source, &file.path, &self.opts).edits)
    }

    fn lint(&mut self, file: &File) -> Result<Vec<Finding>, String> {
        if file.kind != "alx" {
            return Ok(Vec::new());
        }

        self.refresh_theme();

        Ok(emit::run(&file.source, &file.path, &self.opts).findings)
    }

    fn hover(&mut self, file: &File, offset: u32) -> Result<Option<Hover>, String> {
        Ok(editor::hover(&file.source, offset as usize))
    }

    fn complete(
        &mut self,
        file: &File,
        offset: u32,
        _: Option<&str>,
    ) -> Result<Completions, String> {
        if file.kind != "alx" {
            return Ok(Completions::default());
        }

        Ok(editor::complete(&file.source, offset as usize, &self.opts))
    }

    fn colors(&mut self, file: &File) -> Result<Vec<ColorInfo>, String> {
        Ok(editor::colors(&file.source))
    }

    fn present(
        &mut self,
        file: &File,
        span: (u32, u32),
        color: Color,
    ) -> Result<Vec<String>, String> {
        let ours = editor::colors(&file.source).iter().any(|c| c.span == span);

        Ok(match ours {
            true => editor::present(color),

            false => Vec::new(),
        })
    }

    fn manifest(&self) -> Option<&'static str> {
        Some(include_str!("../ingot.toml"))
    }
}

fn main() {
    serve(Silk {
        opts: emit::Options::default(),
        theme_file: None,
    });
}
