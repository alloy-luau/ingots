//! Enamel: utility classes for Alloy markup. `ClassName="flex gap-2
//! bg-red-500 rounded-lg"` on a `.alx` element becomes the Roblox
//! properties and the layout children behind those names, at compile
//! time. The names are Tailwind's; the values are Roblox's.
//!
//! The editor gets the classes too: hover on one shows what it sets,
//! completion lists every utility with a swatch beside each color, and a
//! color class shows its swatch in the text.

mod classes;
mod emit;
mod markup;
mod palette;

use alloy_ingot::{
    Color, ColorInfo, CompletionItem, Edit, File, Finding, Handler, Hover, ItemKind, Settings,
    serve,
};

use classes::{Class, Element, Fonts, Problem};

struct Enamel {
    fonts: Fonts,
    helper: String,
    catalog: Vec<classes::Entry>,
}

impl Enamel {
    fn new() -> Self {
        let fonts = Fonts::default();
        let catalog = classes::catalog(&fonts);

        Self {
            fonts,
            helper: "__enamel".into(),
            catalog,
        }
    }

    fn plans<'a>(&self, found: &'a [markup::Found]) -> Vec<emit::Plan<'a>> {
        found.iter().map(|f| emit::plan(f, &self.fonts)).collect()
    }
}

/// The hover text of one class on an element.
fn describe(class: &Class, element: Option<Element>, fonts: &Fonts) -> String {
    let mut out = format!("```alx\n{}\n```\n", class.token);

    if let Some(v) = &class.unknown_variant {
        out.push_str(&format!(
            "\n`{v}:` is not a state Enamel knows: `hover:`, `active:`, `focus:`, `group-hover:`."
        ));

        return out;
    }

    let Some(u) = classes::parse(class, fonts) else {
        out.push_str("\nNot a utility Enamel knows.");

        return out;
    };

    out.push_str(&format!("\n{}.", capitalize(&u.summary)));

    if let Some(e) = element {
        let r = classes::resolve(e, std::slice::from_ref(class), fonts);
        let mut lines = Vec::new();

        for (k, v) in &r.props {
            lines.push(format!("{k} = {v}"));
        }

        for props in r.states.values() {
            for (k, v) in props {
                lines.push(format!("{k} = {v}"));
            }
        }

        for c in &r.children {
            let props = c
                .props
                .iter()
                .map(|(k, v)| format!(" {k}={{{v}}}"))
                .collect::<String>();
            lines.push(format!("<{}{props} />", c.class));
        }

        if !lines.is_empty() {
            out.push_str(&format!("\n\n```luau\n{}\n```", lines.join("\n")));
        }

        if let Some(state) = class.variants.last() {
            out.push_str(&format!(
                "\n\nApplied on `{}`, through the state helper.",
                state.key()
            ));
        }
    }

    out
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();

    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),

        None => String::new(),
    }
}

impl Handler for Enamel {
    fn init(&mut self, settings: &Settings) -> Result<(), String> {
        let get = |key: &str| {
            settings
                .options
                .get(key)
                .and_then(|v| v.as_str())
                .map(str::to_string)
        };

        if let Some(v) = get("font_sans") {
            self.fonts.sans = v;
        }

        if let Some(v) = get("font_serif") {
            self.fonts.serif = v;
        }

        if let Some(v) = get("font_mono") {
            self.fonts.mono = v;
        }

        if let Some(v) = get("helper") {
            self.helper = v;
        }

        self.catalog = classes::catalog(&self.fonts);

        Ok(())
    }

    fn transform(&mut self, file: &File) -> Result<Vec<Edit>, String> {
        if file.kind != "alx" {
            return Ok(Vec::new());
        }

        let found = markup::find(&file.source);
        let plans = self.plans(&found);
        let mut edits = Vec::new();

        for plan in &plans {
            edits.extend(plan.edits(&self.helper));
        }

        if plans.iter().any(emit::Plan::uses_helper) {
            edits.push(Edit::insert(
                emit::helper_at(&file.source),
                emit::helper_text(&self.helper),
            ));
        }

        Ok(edits)
    }

    fn lint(&mut self, file: &File) -> Result<Vec<Finding>, String> {
        if file.kind != "alx" {
            return Ok(Vec::new());
        }

        let found = markup::find(&file.source);
        let mut out = Vec::new();

        for plan in self.plans(&found) {
            let f = plan.found;

            if plan.element.is_none() {
                out.push(Finding::new(
                    "not_an_element",
                    (f.attr.0 as u32, f.attr.1 as u32),
                    format!(
                        "`ClassName` on `<{}>`: not a Roblox GUI element, so the classes set nothing; put them on the element it renders",
                        f.tag
                    ),
                ));

                continue;
            }

            for (i, problem) in &plan.resolved.problems {
                let (token, (s, e)) = &f.classes[*i];
                let span = (*s as u32, *e as u32);
                let finding = match problem {
                    Problem::Unknown => Finding::new(
                        "unknown_class",
                        span,
                        format!("`{token}` is not a utility Enamel knows"),
                    ),
                    Problem::UnknownVariant(v) => Finding::new(
                        "unknown_class",
                        span,
                        format!(
                            "`{v}:` is not a state Enamel knows; the states are `hover:`, `active:`, `focus:`, and `group-hover:`"
                        ),
                    ),
                    Problem::NoEffect(why) => {
                        Finding::new("no_effect", span, format!("`{token}` sets nothing: {why}"))
                    }
                    Problem::WrongElement(prop, needs) => Finding::new(
                        "wrong_element",
                        span,
                        format!(
                            "`{token}` sets `{prop}`, which `<{}>` lacks; it belongs on {}",
                            f.tag,
                            needs.word()
                        ),
                    ),
                    Problem::VariantNeedsProperty => Finding::new(
                        "unknown_class",
                        span,
                        format!(
                            "`{token}` adds a child or a marker; only a property can change with a state"
                        ),
                    ),
                };
                out.push(finding);
            }
        }

        Ok(out)
    }

    fn hover(&mut self, file: &File, offset: u32) -> Result<Option<Hover>, String> {
        if file.kind != "alx" {
            return Ok(None);
        }

        let found = markup::find(&file.source);
        let Some((f, k)) = markup::class_at(&found, offset as usize) else {
            return Ok(None);
        };
        let (token, (s, e)) = &f.classes[k];
        let class = Class::parse(token);
        let text = describe(&class, Element::parse(&f.tag), &self.fonts);

        Ok(Some(Hover::new(text).over((*s as u32, *e as u32))))
    }

    fn complete(
        &mut self,
        file: &File,
        offset: u32,
        _trigger: Option<&str>,
    ) -> Result<Vec<CompletionItem>, String> {
        if file.kind != "alx" {
            return Ok(Vec::new());
        }

        let found = markup::find(&file.source);
        let Some((s, e)) = markup::string_at(&found, &file.source, offset as usize) else {
            return Ok(Vec::new());
        };
        // The variants typed so far stay in front of every item.
        let typed = &file.source[s..(offset as usize).max(s)];
        let prefix = typed.rfind(':').map_or("", |i| &typed[..=i]);
        let span = (s as u32, e as u32);

        Ok(self
            .catalog
            .iter()
            .map(|(name, summary, color)| {
                let label = format!("{prefix}{name}");
                let mut item = CompletionItem::new(label.clone())
                    .detail(summary.clone())
                    .over(span)
                    .insert(label);

                if let Some(c) = color {
                    let hex = palette::hex(*c);
                    item = item
                        .kind(ItemKind::Color)
                        .detail(hex.clone())
                        .documentation(hex);
                } else {
                    item = item.kind(ItemKind::Property);
                }

                item
            })
            .collect())
    }

    fn colors(&mut self, file: &File) -> Result<Vec<ColorInfo>, String> {
        if file.kind != "alx" {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();

        for f in markup::find(&file.source) {
            for (token, (s, e)) in &f.classes {
                let class = Class::parse(token);

                if let Some(u) = classes::parse(&class, &self.fonts)
                    && let Some((rgb, alpha)) = u.color
                {
                    out.push(ColorInfo::rgb((*s as u32, *e as u32), rgb, alpha));
                }
            }
        }

        Ok(out)
    }

    fn present(
        &mut self,
        file: &File,
        span: (u32, u32),
        color: Color,
    ) -> Result<Vec<String>, String> {
        if file.kind != "alx" {
            return Ok(Vec::new());
        }

        let found = markup::find(&file.source);
        let Some((f, k)) = markup::class_at(&found, span.0 as usize) else {
            return Ok(Vec::new());
        };
        let (token, _) = &f.classes[k];
        let class = Class::parse(token);
        let Some((head, _)) = class.base.split_once('-') else {
            return Ok(Vec::new());
        };
        let variants = class
            .variants
            .iter()
            .map(|v| match v {
                classes::Variant::Hover => "hover:",
                classes::Variant::Active => "active:",
                classes::Variant::Focus => "focus:",
                classes::Variant::GroupHover => "group-hover:",
            })
            .collect::<String>();
        let rgb = color.rgb8();
        let alpha = if color.alpha < 1.0 {
            format!("/{}", (color.alpha * 100.0).round())
        } else {
            String::new()
        };

        Ok(vec![
            format!("{variants}{head}-{}{alpha}", palette::nearest(rgb)),
            format!("{variants}{head}-[{}]{alpha}", palette::hex(rgb)),
        ])
    }

    fn manifest(&self) -> Option<&'static str> {
        Some(include_str!("../ingot.toml"))
    }
}

fn main() {
    serve(Enamel::new());
}
