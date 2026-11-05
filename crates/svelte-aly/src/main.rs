//! The svelte-aly ingot: the protocol side of the library.

use alloy_ingot::{Completions, Edit, File, Finding, Handler, Hover, Settings, serve};
use svelte_aly::editor;
use svelte_aly::rewrite::{self, Options};

struct Svelte {
    opts: Options,
}

impl Svelte {
    fn run(&self, file: &File) -> Result<rewrite::Output, String> {
        if file.path.ends_with(".d.aly") {
            return Ok(rewrite::Output::default());
        }

        rewrite::run(&file.source, &file.kind, &self.opts)
    }
}

impl Handler for Svelte {
    fn init(&mut self, settings: &Settings) -> Result<(), String> {
        let text = |k: &str| {
            settings
                .options
                .get(k)
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };

        if let Some(i) = text("import") {
            self.opts.import = i;
        }

        if let Some(h) = text("helper") {
            self.opts.helper = h;
        }

        // A script file binds the factory, so it must be a plain name.
        let create = settings
            .alx
            .pointer("/factory/create")
            .and_then(|v| v.as_str());
        self.opts.create = match create {
            None => Some("create".into()),

            Some(c) if c.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') => Some(c.into()),

            Some(_) => None,
        };
        self.opts.silk = settings.ingots.iter().any(|n| n == "silk");

        Ok(())
    }

    fn transform(&mut self, file: &File) -> Result<Vec<Edit>, String> {
        Ok(self.run(file)?.edits)
    }

    fn lint(&mut self, file: &File) -> Result<Vec<Finding>, String> {
        Ok(self.run(file)?.findings)
    }

    fn hover(&mut self, file: &File, offset: u32) -> Result<Option<Hover>, String> {
        if file.kind != "alx" {
            return Ok(None);
        }

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

        Ok(editor::complete(&file.source, offset as usize))
    }

    fn manifest(&self) -> Option<&'static str> {
        Some(include_str!("../ingot.toml"))
    }
}

fn main() {
    serve(Svelte {
        opts: Options::default(),
    });
}
