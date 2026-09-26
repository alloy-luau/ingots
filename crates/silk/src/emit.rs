//! The transform. Each HTML element becomes the Roblox element behind it,
//! with the properties its tag, its attributes, and its `style` give; a
//! `<style>` element becomes a StyleLink to a StyleSheet of StyleRules.
//! Every edit keeps the line count, so a problem in the output points at
//! the line the author wrote.

use std::collections::{BTreeMap, HashMap, HashSet};

use alloy_ingot::{Edit, Finding};

use crate::css::{self, Compound, Rgba, Selector, Sheet};
use crate::html::{self, Kind, Mapped, Raw, Tag};
use crate::markup::{Child, Element, Markup, Value};
use crate::props::{self, Axis, Ctx, FontParts, Fonts, Layout, Out, Target};
use crate::roblox;

pub struct Options {
    /// Whether the project allows Roblox classes and properties.
    pub roblox: bool,
    /// Whether the project loads Enamel.
    pub enamel: bool,
    /// Whether `class` becomes CollectionService tags.
    pub tags: bool,
    pub helper: String,
    pub fonts: Fonts,
    /// The text color an element has by default.
    pub color: Rgba,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            roblox: true,
            enamel: false,
            tags: true,
            helper: "__silk".into(),
            fonts: Fonts::default(),
            color: Rgba {
                r: 0,
                g: 0,
                b: 0,
                a: 1.0,
            },
        }
    }
}

/// What the transform found in one file.
#[derive(Debug, Default)]
pub struct Output {
    pub edits: Vec<Edit>,
    pub findings: Vec<Finding>,
}

/// Transforms one file.
pub fn run(source: &str, path: &str, opts: &Options) -> Output {
    let markup = Markup::read(source);
    let mut w = Writer::new(source, path, &markup, opts);
    w.write();
    let mut out = w.finish();
    out.findings.extend(void_findings(source));

    // An inherited declaration reports once, not once per text inside.
    let mut seen = HashSet::new();
    out.findings
        .retain(|f| seen.insert((f.lint.clone(), f.span, f.message.clone())));

    out
}

/// A void tag written the HTML way, `<br>`: React closes it itself,
/// `<br />`, and the markup does not read without the slash.
fn void_findings(src: &str) -> Vec<Finding> {
    const VOID: &[&str] = &[
        "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
        "source", "track", "wbr",
    ];
    let mut out = Vec::new();
    let comments = crate::markup::block_comments(src);

    for (lt, _) in src.match_indices('<') {
        if comments.iter().any(|(s, e)| *s < lt && lt < *e) {
            continue;
        }

        let rest = &src[lt + 1..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect();

        if !VOID.contains(&name.as_str()) {
            continue;
        }

        let line = &src[src[..lt].rfind('\n').map_or(0, |n| n + 1)..lt];

        // A comment or a string on the line holds no tag.
        if line.contains("--") || line.matches('"').count() % 2 == 1 {
            continue;
        }

        let Some(gt) = rest.find('>').map(|n| lt + 1 + n) else {
            continue;
        };

        if src[..gt].ends_with('/') || src[lt + 1..gt].contains('<') {
            continue;
        }

        out.push(
            Finding::new(
                "void_tag",
                (lt as u32, (gt + 1) as u32),
                format!("React closes a void tag itself: write `<{name} ... />`"),
            )
            .with_fix(Edit::insert(gt as u32, " /")),
        );
    }

    out
}

/// The CSS of every `<style>` element of a file, merged, for the editor
/// and for Enamel: which classes the file styles, and its variables.
pub fn sheets(source: &str) -> Vec<(usize, Sheet)> {
    let m = Markup::read(source);

    m.elements
        .iter()
        .enumerate()
        .filter(|(_, e)| e.name == "style")
        .filter_map(|(i, e)| {
            e.css(source)
                .map(|(s, t)| (i, css::parse_sheet(&source[s..t], s)))
        })
        .collect()
}

struct Writer<'a> {
    src: &'a str,
    path: &'a str,
    m: &'a Markup,
    opts: &'a Options,
    sheets: HashMap<usize, Sheet>,
    vars: BTreeMap<String, String>,
    foldable: Vec<bool>,
    /// Elements another element's edits cover: folded, blanked, or inside
    /// a `<style>`.
    covered: Vec<bool>,
    /// The `LayoutOrder` a parent gives each child.
    order: HashMap<usize, usize>,
    /// Elements whose children run in a row: a flex row, or a button
    /// that holds an icon beside its text.
    row_parents: HashSet<usize>,
    uses_rich: bool,
    reps: Vec<(usize, usize, String)>,
    ins: Vec<(usize, i64, usize, String)>,
    seq: usize,
    findings: Vec<Finding>,
    uses_helper: bool,
    uses_sheet: bool,
    style_count: usize,
}

/// Ranks for insertions at one offset: an outer opening comes first, an
/// inner closing comes first.
const RANK_PRELUDE: i64 = -10_000;
const RANK_WRAP_OPEN: i64 = 10;
const RANK_RUN_OPEN: i64 = 20;
const RANK_TEXT: i64 = 30;
const RANK_RUN_CLOSE: i64 = -20;
const RANK_WRAP_CLOSE: i64 = -30;

impl<'a> Writer<'a> {
    fn new(src: &'a str, path: &'a str, m: &'a Markup, opts: &'a Options) -> Self {
        let mut sheets = HashMap::new();
        let mut vars = BTreeMap::new();

        for (i, e) in m.elements.iter().enumerate() {
            if e.name == "style"
                && let Some((s, t)) = e.css(src)
            {
                let sheet = css::parse_sheet(&src[s..t], s);
                vars.extend(sheet.vars.clone());
                sheets.insert(i, sheet);
            }
        }

        let n = m.elements.len();
        let mut w = Self {
            src,
            path,
            m,
            opts,
            sheets,
            vars,
            foldable: vec![false; n],
            covered: vec![false; n],
            order: HashMap::new(),
            row_parents: HashSet::new(),
            uses_rich: false,
            reps: Vec::new(),
            ins: Vec::new(),
            seq: 0,
            findings: Vec::new(),
            uses_helper: false,
            uses_sheet: false,
            style_count: 0,
        };
        w.find_foldable();

        w
    }

    fn el(&self, i: usize) -> &'a Element {
        &self.m.elements[i]
    }

    fn tag(&self, i: usize) -> Option<&'static Tag> {
        let name = &self.el(i).name;

        match name.chars().next() {
            Some(c) if c.is_ascii_lowercase() => html::tag(name),

            _ => None,
        }
    }

    fn replace(&mut self, s: usize, e: usize, text: impl Into<String>) {
        let text = pad(&self.src[s..e], text.into());
        self.reps.push((s, e, text));
    }

    /// Removes an attribute and the spaces before it.
    fn remove_attr(&mut self, span: (usize, usize)) {
        let b = self.src.as_bytes();
        let mut s = span.0;

        while s > 0 && matches!(b[s - 1], b' ' | b'\t') {
            s -= 1;
        }

        self.replace(s, span.1, "");
    }

    fn insert(&mut self, at: usize, rank: i64, text: impl Into<String>) {
        self.seq += 1;
        self.ins.push((at, rank, self.seq, text.into()));
    }

    fn find(&mut self, lint: &str, span: (usize, usize), message: impl Into<String>) {
        self.findings
            .push(Finding::new(lint, (span.0 as u32, span.1 as u32), message));
    }

    fn finish(mut self) -> Output {
        let mut lead = String::new();

        if self.uses_helper {
            lead.push_str(&helper_text(&self.opts.helper));
        }

        if self.uses_sheet {
            lead.push_str(&sheet_text(&self.opts.helper));
        }

        if self.uses_rich {
            lead.push_str(&rich_text(&self.opts.helper));
        }

        if !lead.is_empty() {
            let at = helper_at(self.src);
            self.insert(at, RANK_PRELUDE, lead);
        }

        let mut edits: Vec<(usize, usize, i64, usize, String)> = self
            .reps
            .into_iter()
            .map(|(s, e, t)| (s, e, 0, 0, t))
            .collect();
        edits.extend(self.ins.into_iter().map(|(at, r, q, t)| (at, at, r, q, t)));
        edits.sort_by_key(|(s, e, r, q, _)| (*s, *e != *s, *r, *q));

        // Inserts at one offset join into one edit, in rank order.
        let mut out: Vec<Edit> = Vec::new();

        for (s, e, _, _, t) in edits {
            match out.last_mut() {
                Some(last) if s == e && last.0 == s as u32 && last.1 == s as u32 => {
                    last.2.push_str(&t)
                }

                _ => out.push(Edit::replace(s as u32, e as u32, t)),
            }
        }

        Output {
            edits: out,
            findings: self.findings,
        }
    }

    // ---------------------------------------------------------- structure

    /// Marks the elements that fold into their parent's text as RichText.
    fn find_foldable(&mut self) {
        for i in (0..self.m.elements.len()).rev() {
            let e = self.el(i);
            let Some(tag) = self.tag(i) else {
                continue;
            };
            let events = e.attrs.iter().any(|a| html::event(&a.name).is_some());
            let children_fold = e.children.iter().all(|c| match c {
                Child::Element(k) => self.foldable[*k],

                _ => true,
            });
            let parent_text = e.parent.is_some_and(|p| self.has_literal(p));
            let parent_holds_text = e.parent.and_then(|p| self.tag(p)).is_some_and(|t| {
                matches!(
                    t.kind,
                    Kind::Text | Kind::Inline | Kind::Cell | Kind::Button | Kind::Link
                )
            });

            self.foldable[i] = !events
                && e.hole_owner.is_none_or(|_| e.parent.is_some())
                && e.parent.is_some()
                && children_fold
                && match tag.kind {
                    Kind::Inline | Kind::Break => true,

                    Kind::Link => parent_text || parent_holds_text,

                    _ => false,
                };
        }
    }

    fn has_literal(&self, i: usize) -> bool {
        self.el(i)
            .children
            .iter()
            .any(|c| matches!(c, Child::Text(s, e) if !self.src[*s..*e].trim().is_empty()))
    }

    /// The children that stand as their own boxes: not folded, not a
    /// `<style>`, and not removed.
    fn boxes(&self, i: usize) -> Vec<usize> {
        self.m
            .element_children(i)
            .filter(|k| !self.foldable[*k])
            .filter(|k| {
                !self
                    .tag(*k)
                    .is_some_and(|t| matches!(t.kind, Kind::Style | Kind::Removed | Kind::Break))
            })
            .collect()
    }

    /// Whether an element's own children hold text: literal text, or an
    /// element that folds into it.
    fn holds_text(&self, i: usize) -> bool {
        self.has_literal(i)
            || self
                .m
                .element_children(i)
                .any(|k| self.foldable[k] && self.tag(k).is_some_and(|t| t.kind != Kind::Break))
    }

    // -------------------------------------------------------------- write

    fn write(&mut self) {
        for i in 0..self.m.elements.len() {
            if self.covered[i] {
                continue;
            }

            let e = self.el(i);

            if e.name.is_empty() {
                continue;
            }

            let first = e.name.chars().next().unwrap_or('a');

            if first.is_ascii_uppercase() || e.name.contains('.') {
                if !self.opts.roblox && roblox::is_class(&e.name) {
                    self.find(
                        "roblox_instance",
                        e.name_span,
                        format!("`<{}>` is a Roblox class, and this project turns Roblox names off; write the HTML element", e.name),
                    );
                }

                // A Roblox element among the children of a Silk box
                // takes its place in the order too. A component takes
                // only the props it declares.
                if let Some(k) = self.order.get(&i).copied()
                    && e.attr("LayoutOrder").is_none()
                    && roblox::is_class(&e.name)
                {
                    self.insert(e.name_span.1, RANK_TEXT, format!(" LayoutOrder={{{k}}}"));
                }

                continue;
            }

            let Some(tag) = self.tag(i) else {
                continue;
            };

            if self.foldable[i] {
                continue;
            }

            match tag.kind {
                Kind::Style => self.style(i),

                Kind::Removed => {
                    if tag.name == "source" {
                        self.find(
                            "no_effect",
                            e.name_span,
                            "`<source>` sets nothing; put `src` on the `video` or `audio` element",
                        );
                    }

                    self.blank(i);
                }

                Kind::Break => self.blank(i),

                Kind::Unsupported => self.unsupported(i, tag.doc),

                Kind::Group => self.group(i),

                Kind::Input => {
                    let kind = e
                        .text(self.src, "type")
                        .unwrap_or("text")
                        .to_ascii_lowercase();
                    let kind = kind.as_str();

                    if kind == "hidden" {
                        self.blank(i);
                    } else if html::NO_INPUT.contains(&kind) {
                        self.unsupported(i, &format!("`<input type=\"{kind}\">` has no Roblox form; build it from buttons"));
                    } else {
                        self.element(i, tag);
                    }
                }

                _ => self.element(i, tag),
            }
        }
    }

    /// Blanks an element and marks what it holds as covered.
    fn blank(&mut self, i: usize) {
        let e = self.el(i);
        self.replace(e.start, e.end, "");
        self.cover_inside(i);
    }

    fn cover_inside(&mut self, i: usize) {
        let (s, t) = (self.el(i).start, self.el(i).end);

        for (k, e) in self.m.elements.iter().enumerate() {
            if k != i && e.start >= s && e.end <= t {
                self.covered[k] = true;
            }
        }
    }

    fn unsupported(&mut self, i: usize, why: &str) {
        let e = self.el(i);
        self.find("unsupported_tag", e.name_span, why.to_string());
        // A Frame stands in, so the markup still compiles and the lint is
        // the one report.
        let text = match e.self_close {
            Some(_) => "<Frame />",

            None => "<Frame></Frame>",
        };
        self.replace(e.start, e.end, text);
        self.cover_inside(i);
    }

    fn group(&mut self, i: usize) {
        let e = self.el(i);

        // `<tbody>` becomes `<>`: the rows reach the table's layout.
        self.replace(e.name_span.0, e.open_end - 1, "");

        if let Some((s, t)) = e.close {
            self.replace(s + 2, t - 1, "");
        }
    }

    // -------------------------------------------------------------- style

    fn style(&mut self, i: usize) {
        let e = self.el(i);
        self.cover_inside(i);
        let Some(sheet) = self.sheets.get(&i).cloned() else {
            self.blank(i);

            return;
        };

        for p in &sheet.problems {
            self.find(p.lint, p.span, p.message.clone());
        }

        self.style_count += 1;
        let id = format!("{}#{}", self.path, self.style_count);
        let mut entries: Vec<(usize, String)> = Vec::new();

        for (index, rule) in sheet.rules.iter().enumerate() {
            let only_vars = rule.decls.iter().all(|d| d.name.starts_with("--"));

            if only_vars {
                continue;
            }

            for sel in &rule.selectors {
                match self.rule(sel, &rule.decls, index) {
                    Ok(list) => {
                        for text in list {
                            entries.push((rule.span.0, text));
                        }
                    }

                    Err(why) => self.find("unsupported_css", sel.span, why),
                }
            }
        }

        // Each rule sits on the line of the CSS it came from.
        let mut text = format!(
            "<StyleLink StyleSheet={{{}_sheet(\"{}\", {{",
            self.opts.helper,
            id.replace('\\', "/").replace('"', "")
        );
        let mut line_at = e.start;

        for (at, entry) in entries {
            let newlines = self.src[line_at..at.max(line_at)].matches('\n').count();
            text.push_str(&"\n".repeat(newlines));
            line_at = at.max(line_at);
            text.push_str(&entry);
            text.push_str(", ");
        }

        text.push_str("})} />");
        self.uses_sheet = true;
        self.replace(e.start, e.end, text);
    }

    /// The StyleRule entries of one selector: `{ selector, priority, { props } }`.
    fn rule(
        &mut self,
        sel: &Selector,
        decls: &[css::Decl],
        index: usize,
    ) -> Result<Vec<String>, String> {
        let last = sel.last().ok_or("an empty selector")?;

        if !self.opts.roblox {
            let named = sel.parts.iter().find_map(|(_, c)| {
                c.tag
                    .as_deref()
                    .filter(|t| t.starts_with(|ch: char| ch.is_ascii_uppercase()))
                    .or(c
                        .pseudo_element
                        .as_deref()
                        .filter(|p| p.starts_with(|ch: char| ch.is_ascii_uppercase())))
            });

            if let Some(name) = named {
                self.find(
                    "roblox_instance",
                    sel.span,
                    format!("`{name}` is a Roblox class, and this project turns Roblox names off"),
                );

                return Ok(Vec::new());
            }
        }
        let placeholder = last.pseudo_element.as_deref() == Some("placeholder");
        let roblox_sel = roblox_selector(sel, placeholder)?;
        let target = match &last.tag {
            Some(t) if t.chars().next().is_some_and(char::is_uppercase) => target_of(t),

            Some(t) => html::tag(t)
                .and_then(|tag| html::class_of(tag, None))
                .map_or(Target::Any, target_of),

            None => Target::Any,
        };
        let (a, b, c) = sel.specificity();
        let mut out_rules = Vec::new();

        for important in [false, true] {
            let group: Vec<css::Decl> = decls
                .iter()
                .filter(|d| d.important == important && !d.name.starts_with("--"))
                .cloned()
                .map(|mut d| {
                    if placeholder && d.name == "color" {
                        d.name = "PlaceholderColor3".into();
                    }

                    d
                })
                .collect();

            if group.is_empty() {
                continue;
            }

            let ctx = Ctx {
                vars: &self.vars,
                fonts: &self.opts.fonts,
                roblox: true,
                target,
            };
            let mut out = Out::default();
            props::apply(&group, &ctx, &mut out);
            props::finish_colors(&mut out, target, true);

            for p in std::mem::take(&mut out.problems) {
                self.find(p.lint, p.span, p.message);
            }

            for d in &group {
                if d.name.chars().next().is_some_and(char::is_uppercase) && !self.opts.roblox {
                    self.find(
                        "roblox_instance",
                        d.name_span,
                        format!(
                            "`{}` is a Roblox property, and this project turns Roblox names off",
                            d.name
                        ),
                    );
                }
            }

            if !out.rich.is_empty()
                && let Some(d) = group.iter().find(|d| d.name.starts_with("text-"))
            {
                self.find("no_effect", d.name_span, "RichText tags live in the text, which a StyleRule cannot reach; put this in the element's `style`");
            }

            if out.font.is_set() {
                out.set(
                    "FontFace",
                    out.font.luau(&FontParts::default(), &self.opts.fonts),
                );
            }

            // An axis the rule leaves out takes the element's own: a box
            // is as wide as its parent, and an inline tag sizes to its text.
            let inline = last.tag.as_deref().and_then(html::tag).is_some_and(|t| {
                matches!(
                    t.kind,
                    Kind::Inline | Kind::Button | Kind::Link | Kind::Input
                )
            });
            let base_width = match inline {
                true => Axis::Auto,

                false => Axis::Len(css::Len {
                    scale: 1.0,
                    offset: 0.0,
                }),
            };

            if let Some((size, auto)) = out.size((base_width, Axis::Auto)) {
                out.set("Size", size);
                out.set("AutomaticSize", auto);
            }

            let priority = |extra: u64| {
                ((((u64::from(a) + extra) * 100 + u64::from(b)) * 100 + u64::from(c)) * 1000
                    + index as u64)
                    .min(2_000_000_000)
            };
            let pri = priority(if important { 20 } else { 0 });
            let entry = |selector: &str, props: &[(String, String)]| {
                let fields = props
                    .iter()
                    .map(|(k, v)| format!("{k} = {v}"))
                    .collect::<Vec<_>>()
                    .join(", ");

                format!(
                    "{{ \"{}\", {pri}, {{ {fields} }} }}",
                    selector.replace('"', "\\\"")
                )
            };

            if !out.props.is_empty() {
                out_rules.push(entry(&roblox_sel, &out.props));
            }

            for (class, props) in &out.mods {
                out_rules.push(entry(&format!("{roblox_sel}::{class}"), props));
            }

            if !out.layout.is_empty() {
                let layout = match out.layout_kind {
                    Some(Layout::Grid) => "UIGridLayout",

                    _ => "UIListLayout",
                };
                out_rules.push(entry(&format!("{roblox_sel} > {layout}"), &out.layout));
            }
        }

        Ok(out_rules)
    }

    // ------------------------------------------------------------ element

    /// The declarations of rules whose selector is one compound that
    /// matches the element as written. Silk reads the layout and the
    /// scrolling from them, which decide the element's class.
    fn static_decls(&self, i: usize) -> Vec<css::Decl> {
        let e = self.el(i);
        let id = e.text(self.src, "id");
        let classes: Vec<&str> = ["class", "className"]
            .iter()
            .filter_map(|n| e.text(self.src, n))
            .flat_map(str::split_whitespace)
            .collect();
        let mut out = Vec::new();

        for sheet in self.sheets.values() {
            for rule in &sheet.rules {
                let hit = rule.selectors.iter().any(|s| {
                    s.parts.len() == 1
                        && s.parts[0].1.pseudo.is_empty()
                        && s.parts[0].1.pseudo_element.is_none()
                        && s.parts[0].1.unsupported.is_none()
                        && matches_static(&s.parts[0].1, &e.name, id, &classes)
                });

                if hit {
                    out.extend(
                        rule.decls
                            .iter()
                            .filter(|d| {
                                matches!(
                                    d.name.as_str(),
                                    "display"
                                        | "overflow"
                                        | "overflow-x"
                                        | "overflow-y"
                                        | "list-style"
                                        | "list-style-type"
                                        | "flex-direction"
                                        | "height"
                                        | "max-height"
                                )
                            })
                            .cloned(),
                    );
                }
            }
        }

        out
    }

    #[allow(clippy::too_many_lines)]
    fn element(&mut self, i: usize, tag: &'static Tag) {
        let e = self.el(i);
        let src = self.src;
        let input_type_owned = e.text(src, "type").map(str::to_ascii_lowercase);
        let input_type = input_type_owned.as_deref();
        let boxes = self.boxes(i);
        let holds_text = self.holds_text(i);
        let holes: Vec<(usize, usize)> = e
            .children
            .iter()
            .filter_map(|c| match c {
                Child::Hole(s, t) => Some((*s, *t)),

                _ => None,
            })
            .collect();

        // The kind the element takes with the content it has.
        let mut kind = tag.kind;

        match kind {
            Kind::Block if holds_text && boxes.is_empty() => kind = Kind::Text,

            Kind::Text | Kind::Cell | Kind::Inline if !boxes.is_empty() => kind = Kind::Block,

            _ => {}
        }

        let demoted =
            matches!(tag.kind, Kind::Text | Kind::Cell | Kind::Inline) && kind == Kind::Block;
        // A button or a link that holds a box, an icon beside its text,
        // lays both out in a row.
        let button_row = matches!(tag.kind, Kind::Button | Kind::Link) && !boxes.is_empty();
        // Text beside a box stands in a TextLabel of its own, as a
        // browser puts it in an anonymous box.
        let wrap_runs = holds_text
            && !boxes.is_empty()
            && (demoted
                || button_row
                || matches!(
                    kind,
                    Kind::Block | Kind::List | Kind::Table | Kind::Row | Kind::Canvas
                ));
        let clicks = e.attrs.iter().any(|a| {
            matches!(
                html::event(&a.name).map(|(ev, _)| ev),
                Some("Activated" | "MouseButton1Down" | "MouseButton1Up" | "MouseButton2Click")
            )
        });

        // The style: the element's own, and what the file's rules decide
        // about its class.
        let mut statics = Out::default();
        let static_decls = self.static_decls(i);
        let mut list_style_none = false;

        let own_decls = self.style_of(i, true);

        for d in static_decls.iter().chain(&own_decls) {
            if matches!(d.name.as_str(), "list-style" | "list-style-type") {
                list_style_none = d.value.trim() == "none";
            }
        }

        let base_class = html::class_of(tag, input_type).unwrap_or("Frame");
        let mut class = match kind {
            Kind::Text | Kind::Inline | Kind::Cell => "TextLabel",

            Kind::Block => "Frame",

            _ => base_class,
        };

        if clicks {
            class = match class {
                "Frame" | "TextLabel" => "TextButton",

                "ImageLabel" => "ImageButton",

                c => c,
            };
        }

        let target = target_of(class);
        let ctx = Ctx {
            vars: &self.vars,
            fonts: &self.opts.fonts,
            roblox: self.opts.roblox,
            target,
        };
        props::apply(
            &static_decls
                .iter()
                .filter(|d| d.name.starts_with("display") || d.name.starts_with("overflow"))
                .cloned()
                .collect::<Vec<_>>(),
            &ctx,
            &mut statics,
        );
        let mut style = Out::default();
        let mut decls = self.inherited(i);
        decls.extend(own_decls.iter().cloned());
        props::apply(&decls, &ctx, &mut style);

        for p in std::mem::take(&mut style.problems) {
            self.find(p.lint, p.span, p.message);
        }

        let scroll = style.scroll.or(statics.scroll);

        if scroll.is_some() && class == "Frame" {
            class = "ScrollingFrame";
        }

        let layout_kind = style.layout_kind.or(statics.layout_kind);
        let text_class = matches!(class, "TextLabel" | "TextButton" | "TextBox");

        // Enamel reads the same classes; Silk leaves the properties its
        // utilities set to it.
        let class_tokens: Vec<&str> = ["class", "className"]
            .iter()
            .filter_map(|n| e.text(src, n))
            .flat_map(str::split_whitespace)
            .collect();
        let enamel = self.opts.enamel && !class_tokens.is_empty();
        // A utility with a state variant applies on that state alone, so
        // the element keeps the defaults for its resting look.
        let has = |prefixes: &[&str]| {
            enamel
                && class_tokens.iter().any(|t| {
                    !t.contains(':')
                        && prefixes
                            .iter()
                            .any(|p| *t == *p || t.starts_with(&format!("{p}-")))
                })
        };
        let enamel_direction = has(&["flex", "inline-flex", "grid"]);
        let enamel_arranges = has(&["gap", "justify", "items", "sort"]);
        // Enamel lays out children in a row unless a class says the
        // direction; a box stacks them, so Silk asks for a column.
        let enamel_column = enamel_arranges && !enamel_direction;
        let enamel_layout = enamel_direction || enamel_arranges;
        let enamel_padding = has(&["p", "px", "py", "pt", "pr", "pb", "pl"]);
        let enamel_corner = has(&["rounded"]);
        let enamel_stroke = has(&["border", "ring"]);
        let enamel_bg = has(&["bg"]) || e.attr("BackgroundColor3").is_some();
        let enamel_text = has(&["text"]);
        let enamel_font = has(&["font", "italic", "not-italic"]);
        let enamel_leading = has(&["leading"]);
        let enamel_truncate = has(&["truncate", "whitespace"]);

        // ---- the defaults a browser gives the tag
        let text_look = html::text_style(tag.name);
        let mut d = Out::default();

        if class != "Sound" {
            d.set("BorderSizePixel", "0");
        }

        let background = match (tag.kind, class) {
            (Kind::Button, _) => Some("Color3.fromRGB(239, 239, 239)"),

            (Kind::Input | Kind::TextArea, "TextBox") => Some("Color3.fromRGB(255, 255, 255)"),

            (Kind::Input, _) => Some("Color3.fromRGB(239, 239, 239)"),

            (Kind::Rule, _) => Some("Color3.fromRGB(128, 128, 128)"),

            (Kind::Progress, _) => Some("Color3.fromRGB(224, 224, 224)"),

            _ if tag.name == "mark" => Some("Color3.fromRGB(255, 255, 0)"),

            _ if tag.name == "dialog" => Some("Color3.fromRGB(255, 255, 255)"),

            _ => None,
        };

        if !enamel_bg && class != "Sound" {
            match background {
                Some(c) => {
                    d.set("BackgroundColor3", c);
                    d.set("BackgroundTransparency", "0");
                }

                None => d.set("BackgroundTransparency", "1"),
            }
        }

        let mut base_font = FontParts {
            family: Some(match text_look.mono {
                true => self.opts.fonts.mono.clone(),

                false => self.opts.fonts.sans.clone(),
            }),
            weight: Some(text_look.weight),
            italic: Some(text_look.italic),
        };

        if text_class {
            let color = match tag.kind {
                Kind::Link => Rgba {
                    r: 0,
                    g: 0,
                    b: 238,
                    a: 1.0,
                },

                _ => self.opts.color,
            };
            d.set("TextColor3", color.luau());
            d.set("TextSize", css::num(text_look.size));

            let block_text = matches!(kind, Kind::Text | Kind::TextArea) || demoted;
            d.set("TextWrapped", block_text.to_string());
            let centered = matches!(tag.kind, Kind::Button)
                || class == "TextButton" && tag.kind == Kind::Input
                || tag.name == "th"
                || tag.name == "center";
            d.set(
                "TextXAlignment",
                match centered {
                    true => "Enum.TextXAlignment.Center",

                    false => "Enum.TextXAlignment.Left",
                },
            );
            d.set(
                "TextYAlignment",
                match matches!(kind, Kind::Text | Kind::TextArea) {
                    true => "Enum.TextYAlignment.Top",

                    false => "Enum.TextYAlignment.Center",
                },
            );
        }

        // The text defaults an Enamel utility sets itself.
        d.props.retain(|(k, _)| {
            !(enamel_text
                && matches!(
                    k.as_str(),
                    "TextSize"
                        | "TextColor3"
                        | "TextTransparency"
                        | "TextXAlignment"
                        | "TextWrapped"
                ))
                && !(enamel_font && k == "FontFace")
                && !(enamel_leading && k == "LineHeight")
                && !(enamel_truncate && matches!(k.as_str(), "TextTruncate" | "TextWrapped"))
        });

        if class == "TextBox" {
            d.set("ClearTextOnFocus", "false");
            d.set("PlaceholderColor3", "Color3.fromRGB(117, 117, 117)");

            if tag.kind == Kind::TextArea {
                d.set("MultiLine", "true");

                if e.text(src, "wrap") == Some("off") {
                    d.set("TextWrapped", "false");
                }
            }
        }

        if class == "TextButton" && (tag.kind != Kind::Button && tag.kind != Kind::Input) {
            d.set("AutoButtonColor", "false");
        }

        if tag.name == "dialog" {
            d.set("Visible", "false");
        }

        if tag.name == "center" {
            d.layout("HorizontalAlignment", "Enum.HorizontalAlignment.Center");
        }

        // The size a browser gives the tag, over what the author set.
        let px = |name: &str| -> Option<Axis> {
            e.text(src, name)
                .and_then(|v| v.trim_end_matches("px").parse::<f64>().ok())
                .map(|v| Axis::Len(css::Len::px(v)))
        };
        let full = Axis::Len(css::Len {
            scale: 1.0,
            offset: 0.0,
        });
        let fixed = |w: f64, h: f64| (Axis::Len(css::Len::px(w)), Axis::Len(css::Len::px(h)));
        let base_size: Option<(Axis, Axis)> = match tag.kind {
            _ if tag.name == "body" => Some((full, full)),

            _ if class == "Sound" => None,

            Kind::Block | Kind::List | Kind::Table | Kind::Text => Some((full, Axis::Auto)),

            _ if demoted => Some((full, Axis::Auto)),

            Kind::Rule => Some((full, Axis::Len(css::Len::px(1.0)))),

            Kind::Inline | Kind::Button | Kind::Link | Kind::Row | Kind::Cell => {
                Some((Axis::Auto, Axis::Auto))
            }

            Kind::Input if class == "TextButton" => Some((Axis::Auto, Axis::Auto)),

            Kind::Input => Some(fixed(150.0, 21.0)),

            Kind::TextArea => {
                let rows = e
                    .text(src, "rows")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(2.0);
                let cols = e
                    .text(src, "cols")
                    .and_then(|v| v.parse::<f64>().ok())
                    .unwrap_or(20.0);

                Some(fixed(cols * 8.0 + 4.0, rows * 16.0 + 4.0))
            }

            Kind::Image => Some((
                px("width").unwrap_or(Axis::Len(css::Len::px(100.0))),
                px("height").unwrap_or(Axis::Len(css::Len::px(100.0))),
            )),

            Kind::Video | Kind::Canvas => Some((
                px("width").unwrap_or(Axis::Len(css::Len::px(300.0))),
                px("height").unwrap_or(Axis::Len(css::Len::px(150.0))),
            )),

            Kind::Progress => Some(fixed(160.0, 16.0)),

            _ => None,
        };

        // A child of a row sizes to its content, as a flex item does.
        let in_row = e
            .parent
            .or(e.hole_owner)
            .is_some_and(|p| self.row_parents.contains(&p));
        let base_size = match base_size {
            Some((w, h)) if in_row && w == full => Some((Axis::Auto, h)),

            other => other,
        };
        let column = own_decls
            .iter()
            .chain(&static_decls)
            .any(|d| d.name == "flex-direction" && d.value.trim().starts_with("column"));

        if button_row || (layout_kind == Some(Layout::Flex) && !column) {
            self.row_parents.insert(i);
        }

        // The modifier children a browser's look needs.
        let stroke = |d: &mut Out, color: &str| {
            d.modifier("UIStroke", "Color", color);
            d.modifier("UIStroke", "Thickness", "1");
            d.modifier("UIStroke", "ApplyStrokeMode", "Enum.ApplyStrokeMode.Border");
        };
        let pad = |d: &mut Out, t: f64, r: f64, b: f64, l: f64| {
            for (k, v) in [
                ("PaddingTop", t),
                ("PaddingRight", r),
                ("PaddingBottom", b),
                ("PaddingLeft", l),
            ] {
                d.modifier("UIPadding", k, css::Len::px(v).udim());
            }
        };

        match tag.kind {
            Kind::Button => {
                pad(&mut d, 1.0, 6.0, 1.0, 6.0);
                stroke(&mut d, "Color3.fromRGB(118, 118, 118)");
                d.modifier("UICorner", "CornerRadius", "UDim.new(0, 3)");
            }

            Kind::Input | Kind::TextArea => {
                pad(&mut d, 1.0, 2.0, 1.0, 2.0);
                stroke(&mut d, "Color3.fromRGB(118, 118, 118)");
                d.modifier("UICorner", "CornerRadius", "UDim.new(0, 2)");
            }

            Kind::List => d.modifier("UIPadding", "PaddingLeft", "UDim.new(0, 40)"),

            Kind::Cell => pad(&mut d, 1.0, 1.0, 1.0, 1.0),

            Kind::Progress => d.modifier("UICorner", "CornerRadius", "UDim.new(0, 4)"),

            _ if tag.name == "blockquote" => {
                d.modifier("UIPadding", "PaddingLeft", "UDim.new(0, 40)");
                d.modifier("UIPadding", "PaddingRight", "UDim.new(0, 40)");
            }

            _ if tag.name == "fieldset" || tag.name == "dialog" => {
                pad(&mut d, 8.0, 12.0, 8.0, 12.0);
                stroke(&mut d, "Color3.fromRGB(192, 192, 192)");
            }

            _ => {}
        }

        if enamel_padding {
            d.mods.retain(|(c, _)| *c != "UIPadding");
        }

        if enamel_corner {
            d.mods.retain(|(c, _)| *c != "UICorner");
        }

        if enamel_stroke {
            d.mods.retain(|(c, _)| *c != "UIStroke");
        }

        // ---- the attributes
        let mut written: HashSet<String> = HashSet::new();
        let mut extra: Vec<(String, String)> = Vec::new();
        let mut name_from_id = false;
        let mut tags_expr: Option<String> = None;
        let mut href: Option<String> = None;

        for a in &e.attrs {
            if let Some(react) = html::react_name(&a.name) {
                self.findings.push(
                    Finding::new(
                        "react_name",
                        (a.name_span.0 as u32, a.name_span.1 as u32),
                        format!("React writes `{react}`, not `{}`", a.name),
                    )
                    .with_fix(Edit::replace(
                        a.name_span.0 as u32,
                        a.name_span.1 as u32,
                        react,
                    )),
                );
            }

            let raw = match a.value {
                Value::Bare => Raw::Bare,

                Value::Str(s, t) => Raw::Str(&src[s..t]),

                Value::Expr(s, t) => Raw::Expr(&src[s..t]),
            };

            match html::map(tag, &a.name, raw, input_type) {
                Mapped::Rename(to) => {
                    self.replace(a.name_span.0, a.name_span.1, to);
                    written.insert(to.to_string());
                }

                Mapped::Event(to) => {
                    let button = matches!(class, "TextButton" | "ImageButton");
                    let fits = match to {
                        "Activated" | "MouseButton1Down" | "MouseButton1Up"
                        | "MouseButton2Click" => button,

                        "Focused" | "FocusLost" => class == "TextBox",

                        _ => class != "Sound",
                    };

                    if fits {
                        self.replace(a.name_span.0, a.name_span.1, to);
                        written.insert(to.to_string());
                    } else {
                        self.find(
                            "no_effect",
                            a.name_span,
                            format!("`{}` sets nothing: a {class} has no `{to}` event", a.name),
                        );
                        self.remove_attr(a.span);
                    }
                }

                Mapped::Props(list) => {
                    let text = list
                        .iter()
                        .map(|(k, v)| format!("{k}={{{v}}}"))
                        .collect::<Vec<_>>()
                        .join(" ");
                    self.replace(a.span.0, a.span.1, text);

                    for (k, _) in list {
                        written.insert(k.to_string());
                    }
                }

                Mapped::Drop => self.remove_attr(a.span),

                Mapped::NoEffect(why) => {
                    self.find(
                        "no_effect",
                        a.name_span,
                        format!("`{}` sets nothing: {why}", a.name),
                    );
                    self.remove_attr(a.span);
                }

                Mapped::Unknown => {
                    self.find(
                        "unknown_attribute",
                        a.name_span,
                        format!(
                            "Silk does not know `{}` on `<{}>`; it sets nothing",
                            a.name, tag.name
                        ),
                    );
                    self.remove_attr(a.span);
                }

                Mapped::Keep => {
                    if a.name.chars().next().is_some_and(char::is_uppercase) {
                        if !self.opts.roblox {
                            self.find("roblox_instance", a.name_span, format!("`{}` is a Roblox property, and this project turns Roblox names off", a.name));
                        }

                        written.insert(a.name.clone());
                    }
                }

                Mapped::Read => match a.name.as_str() {
                    "id" => {
                        self.replace(a.name_span.0, a.name_span.1, "Name");
                        name_from_id = true;
                    }

                    "class" | "className" => match a.value {
                        Value::Str(_, t) if enamel => {
                            self.replace(a.name_span.0, a.name_span.1, "ClassName");

                            if enamel_column {
                                self.insert(t, RANK_TEXT, " flex-col");
                            }
                        }

                        Value::Expr(s, t) => {
                            tags_expr = Some(src[s..t].trim().to_string());
                            self.remove_attr(a.span);
                        }

                        _ => self.remove_attr(a.span),
                    },

                    "href" => {
                        match raw {
                            Raw::Str(h) if h.starts_with('#') => href = Some(format!("\"{}\"", &h[1..])),

                            Raw::Str(h) => self.find("link_out", a.span, format!("a Roblox game cannot open `{h}`; `href=\"#id\"` scrolls to the element with that `id`")),

                            Raw::Expr(x) => href = Some(format!("string.gsub({}, \"^#\", \"\")", x.trim())),

                            Raw::Bare => {}
                        }

                        self.remove_attr(a.span);
                    }

                    _ => self.remove_attr(a.span),
                },
            }
        }

        if !name_from_id {
            extra.push(("Name".into(), format!("\"{}\"", tag.name)));
        }

        // ---- merge: the browser's look, then the author's style
        let mut m = d;

        for (k, v) in style.props.drain(..) {
            m.set(&k, v);
        }

        for (class_name, props) in style.mods.drain(..) {
            for (k, v) in props {
                m.modifier(class_name, &k, v);
            }
        }

        for (k, v) in style.layout.drain(..) {
            m.layout(&k, v);
        }

        m.background = style.background;
        m.opacity = style.opacity;
        props::finish_colors(&mut m, target, false);

        if let Some(base) = base_size {
            let w = style.width.unwrap_or(base.0);
            let h = style.height.unwrap_or(base.1);
            let (size, auto) = props::size_props(w, h);
            m.set("Size", size);
            m.set("AutomaticSize", auto);

            // A rule of the file may give the height.
            let ruled = static_decls
                .iter()
                .any(|d| matches!(d.name.as_str(), "height" | "max-height"));

            if scroll.is_some() && matches!(h, Axis::Auto) && !ruled {
                self.find("scroll_height", e.name_span, "a ScrollingFrame with an automatic height grows with its content and never scrolls; give it a `height`");
            }
        }

        if let Some(dir) = scroll {
            m.set(
                "ScrollingDirection",
                format!("Enum.ScrollingDirection.{dir}"),
            );
            m.set("CanvasSize", "UDim2.new()");
            m.set("AutomaticCanvasSize", format!("Enum.AutomaticSize.{dir}"));
            m.set("ScrollBarThickness", "6");
        }

        if text_class && (!enamel_font || style.font.is_set()) {
            base_font = FontParts {
                family: style.font.family.clone().or(base_font.family),
                weight: style.font.weight.or(base_font.weight),
                italic: style.font.italic.or(base_font.italic),
            };
            m.set(
                "FontFace",
                base_font.luau(&FontParts::default(), &self.opts.fonts),
            );
        }

        if tag.kind == Kind::Progress {
            let num_or = |name: &str, default: &str| -> String {
                match e.attr(name).map(|a| a.value) {
                    Some(Value::Str(s, t)) => src[s..t].trim().to_string(),

                    Some(Value::Expr(s, t)) => format!("({})", src[s..t].trim()),

                    _ => default.to_string(),
                }
            };
            let value = num_or("value", "0");
            let max = num_or("max", "1");
            let min = num_or("min", "0");
            let fill = match (value.parse::<f64>(), max.parse::<f64>(), min.parse::<f64>()) {
                (Ok(v), Ok(x), Ok(n)) if x > n => css::num(((v - n) / (x - n)).clamp(0.0, 1.0)),

                _ => format!("math.clamp(({value} - {min}) / ({max} - {min}), 0, 1)"),
            };
            m.mods.push((
                "Frame",
                vec![
                    ("Size".into(), format!("UDim2.fromScale({fill}, 1)")),
                    (
                        "BackgroundColor3".into(),
                        "Color3.fromRGB(0, 117, 255)".into(),
                    ),
                    ("BorderSizePixel".into(), "0".into()),
                ],
            ));
            m.set("ClipsDescendants", "true");
        }

        // ---- the order of the children
        let has_holes = !holes.is_empty();
        let flow = matches!(kind, Kind::Block | Kind::List | Kind::Table) || demoted || button_row;
        // A table row orders its cells; the table's layout places them.
        let ordered = flow || kind == Kind::Row;

        if let Some(k) = self.order.get(&i).copied()
            && !written.contains("LayoutOrder")
            && m.get("LayoutOrder").is_none()
            && class != "Sound"
        {
            m.set("LayoutOrder", k.to_string());
        }

        // ---- the text
        let mut rich = false;
        let mut text_attr: Option<String> = None;
        let mut runs: Vec<(usize, usize)> = Vec::new();
        let marker = self.marker(i, tag, list_style_none);

        if text_class && !demoted && !wrap_runs {
            if tag.name == "pre" || tag.kind == Kind::TextArea {
                if let Some((s, t)) = e.inner() {
                    text_attr = Some(self.preformatted(i));
                    self.replace(s, t, "");
                }

                self.cover_inside(i);
            } else {
                let wraps = self.wraps(tag, &style);

                if let Some((open, _)) = e.inner() {
                    let (fold_rich, _) = self.fold(i, None);
                    rich |= fold_rich;

                    let mut lead = marker.clone().unwrap_or_default();

                    for (o, _) in &wraps {
                        lead.push_str(&escape_tag(o));
                    }

                    if !lead.is_empty() {
                        self.insert(open, RANK_TEXT, lead);
                    }

                    if let Some((close_start, _)) = e.close {
                        let tail: String = wraps.iter().rev().map(|(_, c)| escape_tag(c)).collect();

                        if !tail.is_empty() {
                            self.insert(close_start, -RANK_TEXT, tail);
                        }
                    }

                    rich |= !wraps.is_empty();
                }

                let content =
                    holds_text || has_holes || marker.is_some() || written.contains("Text");
                let nodes = !m.mods.is_empty() || !boxes.is_empty() || (flow && !boxes.is_empty());

                // Holes alone beside element children are ambiguous to the
                // markup compiler; one hole moves to `Text`.
                if !holds_text && marker.is_none() && wraps.is_empty() && has_holes && nodes {
                    let exprs: Vec<String> = holes
                        .iter()
                        .map(|(s, t)| src[s + 1..t - 1].trim().to_string())
                        .collect();
                    text_attr = Some(match exprs.as_slice() {
                        [one] => one.clone(),

                        many => format!(
                            "`{}`",
                            many.iter().map(|x| format!("{{{x}}}")).collect::<String>()
                        ),
                    });

                    for (s, t) in &holes {
                        self.replace(*s, *t, "");
                    }
                }

                if !content && !written.contains("Text") && e.attr("value").is_none() {
                    text_attr.get_or_insert_with(|| "\"\"".into());
                }
            }
        } else if class == "TextBox" && !written.contains("Text") {
            text_attr = Some("\"\"".into());
        } else if wrap_runs && text_class && !written.contains("Text") {
            // The runs carry the text; the button itself shows none.
            text_attr = Some("\"\"".into());
        }

        if wrap_runs {
            runs = self.runs(i);
        }

        if let Some(t) = text_attr.take()
            && !written.contains("Text")
        {
            m.set("Text", t);
        }

        if rich {
            m.set("RichText", "true");
            self.escape_holes(i, None);
        }

        // The children get their order: boxes and anonymous text. The
        // rows of a table's `thead` and `tbody` are the table's.
        if ordered && !has_holes {
            let mut k = 0;
            let mut slots: Vec<(usize, Option<usize>)> = Vec::new();

            for c in &e.children {
                match c {
                    Child::Element(x) if self.tag(*x).is_some_and(|t| t.kind == Kind::Group) => {
                        for row in self.m.element_children(*x) {
                            slots.push((self.el(row).start, Some(row)));
                        }
                    }

                    Child::Element(x) if boxes.contains(x) => {
                        slots.push((self.el(*x).start, Some(*x)))
                    }

                    _ => {}
                }
            }

            for (s, _) in &runs {
                slots.push((*s, None));
            }

            slots.sort_by_key(|(at, _)| *at);
            let mut run_orders = HashMap::new();

            for (at, x) in slots {
                k += 1;

                match x {
                    Some(x) => {
                        self.order.insert(x, k);
                    }

                    None => {
                        run_orders.insert(at, k);
                    }
                }
            }

            self.write_runs(i, &runs, &run_orders, tag, &style);
        } else if wrap_runs {
            self.write_runs(i, &runs, &HashMap::new(), tag, &style);
        }

        // A box holds no text: a space between two boxes, or a break with
        // no text beside it, writes nothing.
        if !text_class || wrap_runs {
            let children = e.children.clone();

            for c in children {
                match c {
                    Child::Text(s, t)
                        if self.src[s..t].trim().is_empty()
                            && !runs.iter().any(|(a, b)| *a <= s && t <= *b) =>
                    {
                        self.replace(s, t, "");
                    }

                    Child::Element(k) if self.foldable[k] && !self.covered[k] => self.blank(k),

                    _ => {}
                }
            }
        }

        // ---- the layout child
        let layout_child = if tag.kind == Kind::Table {
            Some((
                "UITableLayout",
                vec![
                    (
                        "SortOrder".to_string(),
                        "Enum.SortOrder.LayoutOrder".to_string(),
                    ),
                    ("FillEmptySpaceColumns".to_string(), "true".to_string()),
                ],
            ))
        } else if flow
            && !enamel_layout
            && layout_kind != Some(Layout::None)
            && (!e.children.is_empty())
        {
            let class_name = match layout_kind {
                Some(Layout::Grid) => "UIGridLayout",

                _ => "UIListLayout",
            };
            let mut props = vec![(
                "SortOrder".to_string(),
                "Enum.SortOrder.LayoutOrder".to_string(),
            )];

            if button_row {
                props.push((
                    "FillDirection".into(),
                    "Enum.FillDirection.Horizontal".into(),
                ));
                props.push((
                    "VerticalAlignment".into(),
                    "Enum.VerticalAlignment.Center".into(),
                ));
                props.push(("Padding".into(), "UDim.new(0, 4)".into()));
            }

            props.extend(m.layout.iter().cloned());

            Some((class_name, props))
        } else {
            None
        };

        // ---- write the element
        let mut generated: Vec<(String, String)> = extra;
        generated.extend(m.props.iter().cloned());
        generated.retain(|(k, _)| !written.contains(k));
        let mut seen = HashSet::new();
        generated.retain(|(k, _)| seen.insert(k.clone()));

        let attrs: String = generated
            .iter()
            .map(|(k, v)| format!(" {k}={{{v}}}"))
            .collect();
        self.replace(e.name_span.0, e.name_span.1, format!("{class}{attrs}"));

        if let Some((s, _)) = e.close {
            self.replace(s + 2, s + 2 + e.name.len(), class);
        }

        let mut children = String::new();

        for (c, props) in m
            .mods
            .iter()
            .map(|(c, p)| (*c, p))
            .chain(layout_child.iter().map(|(c, p)| (*c, p)))
        {
            let explicit = self.m.element_children(i).any(|k| self.el(k).name == *c);

            if explicit {
                continue;
            }

            let p: String = props.iter().map(|(k, v)| format!(" {k}={{{v}}}")).collect();
            children.push_str(&format!("<{c}{p} />"));
        }

        match e.self_close {
            Some((s, t)) => {
                let text = match children.is_empty() {
                    true => "/>".to_string(),

                    false => format!(">{children}</{class}>"),
                };
                self.replace(s, t, text);
            }

            None if !children.is_empty() => self.insert(e.open_end, RANK_RUN_OPEN - 5, children),

            None => {}
        }

        // ---- the helper: tags and a link
        let tags = match (&tags_expr, self.opts.tags && !class_tokens.is_empty()) {
            (Some(x), _) => Some(x.clone()),

            (None, true) => Some(format!("\"{}\"", class_tokens.join(" "))),

            _ => None,
        };

        if tags.is_some() && !self.opts.tags {
            // Tags are off; nothing to wrap.
        }

        let tags = tags.filter(|_| self.opts.tags);

        if tags.is_some() || href.is_some() {
            self.uses_helper = true;
            let (open, close) = match e.in_children {
                true => ("{", "}"),

                false => ("", ""),
            };
            // The element builds inside a function the helper calls once:
            // the markup compiler reads markup outside a function in a
            // child as a condition that never updates.
            self.insert(
                e.start,
                RANK_WRAP_OPEN,
                format!("{open}{}(function() return ", self.opts.helper),
            );
            self.insert(
                e.end,
                RANK_WRAP_CLOSE,
                format!(
                    " end, {}, {}){close}",
                    tags.unwrap_or_else(|| "nil".into()),
                    href.unwrap_or_else(|| "nil".into())
                ),
            );
        }
    }

    /// The text declarations an element inherits from the `style` of the
    /// HTML boxes around it, the outermost first, as CSS inherits them.
    fn inherited(&self, i: usize) -> Vec<css::Decl> {
        let mut chain = Vec::new();
        let mut at = self.el(i).parent.or(self.el(i).hole_owner);

        while let Some(p) = at {
            if self.tag(p).is_none() {
                break;
            }

            chain.push(p);
            at = self.el(p).parent.or(self.el(p).hole_owner);
        }

        chain
            .into_iter()
            .rev()
            .flat_map(|p| decls_of(self.src, self.el(p)))
            .filter(|d| props::INHERITED.contains(&d.name.as_str()))
            .collect()
    }

    /// The text of a `<pre>` or a `<textarea>` as one Luau string with
    /// its line breaks: the text of every element inside it, and each
    /// hole interpolated.
    fn preformatted(&self, i: usize) -> String {
        fn collect(w: &Writer, i: usize, out: &mut String, holes: &mut bool) {
            for c in &w.el(i).children {
                match c {
                    Child::Text(s, t) => out.push_str(&w.src[*s..*t]),

                    Child::Hole(s, t) => {
                        *holes = true;
                        out.push('\u{0}');
                        out.push_str(w.src[s + 1..t - 1].trim());
                        out.push('\u{1}');
                    }

                    Child::Element(k) => collect(w, *k, out, holes),

                    Child::Comment(..) => {}
                }
            }
        }

        let mut raw = String::new();
        let mut holes = false;
        collect(self, i, &mut raw, &mut holes);
        let raw = raw.strip_prefix('\n').unwrap_or(&raw);
        let raw = raw.strip_suffix('\n').unwrap_or(raw);

        if !holes {
            return luau_string(raw);
        }

        // A hole reads as `{expr}` in a backtick string.
        let mut out = String::from("`");
        let mut in_hole = false;

        for ch in raw.chars() {
            match (ch, in_hole) {
                ('\u{0}', _) => {
                    in_hole = true;
                    out.push('{');
                }

                ('\u{1}', _) => {
                    in_hole = false;
                    out.push('}');
                }

                (c, true) => out.push(c),

                ('`' | '{' | '}' | '\\', false) => {
                    out.push('\\');
                    out.push(ch);
                }

                ('\n', false) => out.push_str("\\n"),

                ('\r', false) => {}

                (c, false) => out.push(c),
            }
        }

        out.push('`');

        out
    }

    /// Wraps each text hole of a RichText element in the escape helper,
    /// so a `<` in a value shows as itself. `range` limits the holes to
    /// a run.
    fn escape_holes(&mut self, i: usize, range: Option<(usize, usize)>) {
        let mut holes = Vec::new();
        let mut stack = vec![i];

        while let Some(k) = stack.pop() {
            for c in &self.el(k).children {
                match c {
                    Child::Hole(s, t) => holes.push((*s, *t)),

                    Child::Element(x) if self.foldable[*x] => stack.push(*x),

                    _ => {}
                }
            }
        }

        for (s, t) in holes {
            if range.is_some_and(|(a, b)| s < a || t > b) {
                continue;
            }

            self.uses_rich = true;
            let helper = format!("{}_rich(", self.opts.helper);
            self.insert(s + 1, RANK_WRAP_OPEN, helper);
            self.insert(t - 1, RANK_WRAP_CLOSE, ")");
        }
    }

    /// The declarations of an element's `style`: a React table, or a CSS
    /// string that the `react_style` lint reports. `report` sends the
    /// problems to the findings.
    fn style_of(&mut self, i: usize, report: bool) -> Vec<css::Decl> {
        let e = self.el(i);
        let Some(a) = e.attr("style") else {
            return Vec::new();
        };

        match a.value {
            Value::Expr(s, t) => {
                let (decls, problems) = css::parse_table(&self.src[s..t], s);

                if report {
                    for p in problems {
                        self.find(p.lint, p.span, p.message);
                    }
                }

                decls
            }

            Value::Str(s, t) => {
                if report {
                    self.find(
                        "react_style",
                        a.span,
                        "React takes `style` as a table: `style={{ backgroundColor = \"#fff\" }}`",
                    );
                }

                css::parse_decls(&self.src[s..t], s)
            }

            Value::Bare => Vec::new(),
        }
    }

    /// The marker text an `li` starts with.
    fn marker(&self, i: usize, tag: &Tag, none: bool) -> Option<String> {
        if tag.name != "li" || none {
            return None;
        }

        // An item a hole builds sits in the list that holds the hole.
        let parent = self.el(i).parent.or(self.el(i).hole_owner)?;
        let list = &self.el(parent);

        // The list's own `list-style: none` turns its markers off.
        let parent_none = decls_of(self.src, list)
            .iter()
            .any(|d| d.name.starts_with("list-style") && d.value.trim() == "none")
            || self
                .static_decls(parent)
                .iter()
                .any(|d| d.name.starts_with("list-style") && d.value.trim() == "none");

        if parent_none {
            return None;
        }

        match list.name.as_str() {
            "ol" => {
                let start = list
                    .text(self.src, "start")
                    .and_then(|s| s.parse::<i64>().ok())
                    .unwrap_or(1);
                let index = self
                    .m
                    .element_children(parent)
                    .filter(|k| self.el(*k).name == "li")
                    .position(|k| k == i)? as i64;
                let n = self
                    .el(i)
                    .text(self.src, "value")
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(start + index);

                Some(format!("{n}. "))
            }

            "ul" | "menu" => Some("\u{2022} ".into()),

            _ => None,
        }
    }

    /// The RichText tags that wrap an element's whole text: a standalone
    /// `<u>`, a link, and the element's `text-decoration`.
    fn wraps(&self, tag: &Tag, style: &Out) -> Vec<(String, String)> {
        let mut out = Vec::new();

        if matches!(tag.name, "u" | "ins" | "s" | "del" | "strike" | "q") || tag.kind == Kind::Link
        {
            let (o, c) = match tag.kind {
                Kind::Link => ("<u>", "</u>"),

                _ => html::rich(tag.name),
            };
            out.push((o.to_string(), c.to_string()));
        }

        for (o, c) in &style.rich {
            out.push((o.to_string(), c.to_string()));
        }

        out
    }

    /// Folds the inline children of element `i` into its text: each
    /// inline tag becomes a RichText tag. `range` limits the fold to the
    /// children in a span. Returns whether RichText is needed.
    fn fold(&mut self, i: usize, range: Option<(usize, usize)>) -> (bool, bool) {
        let e = self.el(i);
        let mut rich = false;
        let mut any = false;
        let children: Vec<Child> = e.children.clone();

        for c in children {
            let (s, t) = match c {
                Child::Element(k) => (self.el(k).start, self.el(k).end),

                Child::Text(s, t) | Child::Hole(s, t) | Child::Comment(s, t) => (s, t),
            };

            if let Some((a, b)) = range
                && (s < a || t > b)
            {
                continue;
            }

            match c {
                Child::Text(s, t) => {
                    rich |= self.entities(s, t);
                    any |= !self.src[s..t].trim().is_empty();
                }

                Child::Element(k) if self.foldable[k] => {
                    rich |= self.fold_inline(k);
                    any = true;
                }

                _ => {}
            }
        }

        (rich, any)
    }

    /// Writes one inline element as RichText tags around its text.
    fn fold_inline(&mut self, k: usize) -> bool {
        let e = self.el(k);
        self.covered[k] = true;
        let Some(tag) = self.tag(k) else {
            return false;
        };

        if tag.kind == Kind::Break {
            let text = match tag.name {
                "br" => "\\<br />",

                _ => "",
            };
            self.replace(e.start, e.end, text);

            return tag.name == "br";
        }

        if e.attr("class").is_some() || e.attr("className").is_some() {
            let span = e
                .attr("class")
                .or(e.attr("className"))
                .map(|a| a.span)
                .unwrap_or_default();
            self.find("no_effect", span, "a class on text inside text has no instance to style; RichText takes the element's `style`");
        }

        let (mut open, mut close) = match tag.name {
            // Code takes the project's monospace family.
            "code" | "kbd" | "samp" | "tt" => (
                format!(
                    "<font face=\"{}\">",
                    props::rich_face(&self.opts.fonts.mono)
                ),
                "</font>".to_string(),
            ),

            _ => {
                let (o, c) = html::rich(tag.name);
                (o.to_string(), c.to_string())
            }
        };

        // `style` on an inline element becomes a `<font>` tag and friends.
        let decls = self.style_of(k, true);

        if !decls.is_empty() {
            let (o, c) = self.rich_style(&decls);
            open.push_str(&o);
            close = format!("{c}{close}");
        }

        if tag.kind == Kind::Link {
            self.find("inline_link", e.name_span, "a link inside text is RichText, and RichText takes no click; put the link on its own");
        }

        let rich = !open.is_empty() || !close.is_empty();
        self.replace(e.start, e.open_end, escape_tag(&open));

        if let Some((s, t)) = e.close {
            self.replace(s, t, escape_tag(&close));
        }

        let (inner_rich, _) = self.fold(k, None);

        rich || inner_rich
    }

    /// The RichText tags a `style` on inline text writes.
    fn rich_style(&mut self, decls: &[css::Decl]) -> (String, String) {
        let mut font = String::new();
        let mut open = String::new();
        let mut close = String::new();

        for d in decls {
            let v = css::substitute(&d.value, &self.vars);
            let v = v.trim();

            match d.name.as_str() {
                "color" => match css::color(v) {
                    Some(c) => {
                        font.push_str(&format!(" color=\"{}\"", c.hex()));

                        if c.a < 1.0 {
                            font.push_str(&format!(" transparency=\"{}\"", css::num(1.0 - c.a)));
                        }
                    }

                    None => self.find("bad_value", d.value_span, format!("`{v}` is not a color")),
                },

                "font-size" => match props::font_size(v) {
                    Some(px) => font.push_str(&format!(" size=\"{}\"", css::num(px))),

                    None => self.find("bad_value", d.value_span, format!("`{v}` is not a size")),
                },

                "font-family" => {
                    let face = props::rich_face(&props::family(v, &self.opts.fonts));
                    font.push_str(&format!(" face=\"{face}\""));
                }

                "font-weight" => match props::weight(v) {
                    Some(w) => font.push_str(&format!(" weight=\"{}\"", w.to_ascii_lowercase())),

                    None => self.find("bad_value", d.value_span, format!("`{v}` is not a weight")),
                },

                "font-style" if v == "italic" => {
                    open.push_str("<i>");
                    close.insert_str(0, "</i>");
                }

                "background-color" => match css::color(v) {
                    Some(c) => {
                        open.push_str(&format!("<mark color=\"{}\">", c.hex()));
                        close.insert_str(0, "</mark>");
                    }

                    None => self.find("bad_value", d.value_span, format!("`{v}` is not a color")),
                },

                "text-decoration" | "text-decoration-line" => {
                    for w in css::words(v) {
                        let (o, c) = match w {
                            "underline" => ("<u>", "</u>"),

                            "line-through" => ("<s>", "</s>"),

                            _ => continue,
                        };
                        open.push_str(o);
                        close.insert_str(0, c);
                    }
                }

                "text-transform" if v == "uppercase" => {
                    open.push_str("<uc>");
                    close.insert_str(0, "</uc>");
                }

                "opacity" => {
                    if let Ok(o) = v.parse::<f64>() {
                        font.push_str(&format!(" transparency=\"{}\"", css::num(1.0 - o)));
                    }
                }

                other => self.find(
                    "no_effect",
                    d.name_span,
                    format!("RichText has no form for `{other}`"),
                ),
            }
        }

        if !font.is_empty() {
            open = format!("<font{font}>{open}");
            close.push_str("</font>");
        }

        (open, close)
    }

    /// Decodes the character references of a text run. Returns whether a
    /// reference RichText reads itself stays.
    fn entities(&mut self, s: usize, t: usize) -> bool {
        let text = &self.src[s..t];
        let mut rich = false;
        let mut edits = Vec::new();
        let mut from = 0;

        while let Some(at) = text[from..].find('&') {
            let at = from + at;
            let Some(semi) = text[at..].find(';').filter(|n| *n <= 10) else {
                from = at + 1;

                continue;
            };
            let name = &text[at + 1..at + semi];
            let decoded = if let Some(num) = name.strip_prefix('#') {
                let code = match num.strip_prefix('x').or_else(|| num.strip_prefix('X')) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok(),

                    None => num.parse::<u32>().ok(),
                };

                match code.and_then(char::from_u32) {
                    Some('<') => Some("&lt;".to_string()),

                    Some('>') => Some("&gt;".to_string()),

                    Some('&') => Some("&amp;".to_string()),

                    // A brace would open a hole: the markup text escapes it.
                    Some(c @ ('{' | '}' | '`' | '\\')) => Some(format!("\\{c}")),

                    Some(c) => Some(c.to_string()),

                    None => None,
                }
            } else if html::RICH_ENTITIES.contains(&name) {
                rich = true;
                None
            } else {
                html::ENTITIES
                    .iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, c)| (*c).to_string())
            };

            if let Some(d) = decoded {
                rich |= d.starts_with('&');
                edits.push((s + at, s + at + semi + 1, d));
            }

            from = at + semi + 1;
        }

        for (a, b, d) in edits {
            self.replace(a, b, d);
        }

        rich
    }

    /// The runs of text among the children of a box: literal text and
    /// inline elements, next to each other.
    fn runs(&self, i: usize) -> Vec<(usize, usize)> {
        let e = self.el(i);
        let mut out = Vec::new();
        let mut current: Option<(usize, usize, bool)> = None;

        let close = |current: &mut Option<(usize, usize, bool)>, out: &mut Vec<(usize, usize)>| {
            if let Some((s, t, text)) = current.take()
                && text
            {
                out.push((s, t));
            }
        };

        for c in &e.children {
            let (s, t, part, text) = match c {
                Child::Text(s, t) => (*s, *t, true, !self.src[*s..*t].trim().is_empty()),

                Child::Hole(s, t) | Child::Comment(s, t) => (*s, *t, true, false),

                Child::Element(k) if self.foldable[*k] => {
                    let el = self.el(*k);

                    (
                        el.start,
                        el.end,
                        true,
                        self.tag(*k).is_some_and(|t| t.kind != Kind::Break),
                    )
                }

                Child::Element(k) => (self.el(*k).start, self.el(*k).end, false, false),
            };

            if part {
                current = Some(match current {
                    Some((a, _, had)) => (a, t, had || text),

                    None => (s, t, text),
                });
            } else {
                close(&mut current, &mut out);
            }
        }

        close(&mut current, &mut out);

        // A run keeps the space at its ends: a space between a box and the
        // text is part of the text, and nothing is left between them.
        out
    }

    /// Wraps each run in a TextLabel with the element's text look.
    fn write_runs(
        &mut self,
        i: usize,
        runs: &[(usize, usize)],
        orders: &HashMap<usize, usize>,
        tag: &Tag,
        style: &Out,
    ) {
        let look = html::text_style(tag.name);
        let font = FontParts {
            family: style.font.family.clone().or(Some(match look.mono {
                true => self.opts.fonts.mono.clone(),

                false => self.opts.fonts.sans.clone(),
            })),
            weight: style.font.weight.or(Some(look.weight)),
            italic: style.font.italic.or(Some(look.italic)),
        };
        let color = style
            .get("TextColor3")
            .map_or_else(|| self.opts.color.luau(), str::to_string);
        let text_size = style
            .get("TextSize")
            .map_or_else(|| css::num(look.size), str::to_string);

        for (n, (s, t)) in runs.iter().enumerate() {
            let (rich, _) = self.fold(i, Some((*s, *t)));
            let marker = match n {
                0 => self.marker(i, tag, false),

                _ => None,
            };
            let order = orders
                .get(s)
                .map_or(String::new(), |k| format!(" LayoutOrder={{{k}}}"));
            // In a row a run sizes to its text; in a stack it takes the
            // width and wraps.
            let size = match self.row_parents.contains(&i) {
                true => {
                    "Size={UDim2.new()} AutomaticSize={Enum.AutomaticSize.XY} TextWrapped={false}"
                }

                false => {
                    "Size={UDim2.fromScale(1, 0)} AutomaticSize={Enum.AutomaticSize.Y} TextWrapped={true}"
                }
            };

            if rich {
                self.escape_holes(i, Some((*s, *t)));
            }

            let open = format!(
                "<TextLabel Name={{\"text\"}} BackgroundTransparency={{1}} BorderSizePixel={{0}} {size} TextXAlignment={{Enum.TextXAlignment.Left}} TextYAlignment={{Enum.TextYAlignment.Top}} TextColor3={{{color}}} TextSize={{{text_size}}} FontFace={{{}}}{}{order}>{}",
                font.luau(&FontParts::default(), &self.opts.fonts),
                if rich { " RichText={true}" } else { "" },
                marker.unwrap_or_default(),
            );
            self.insert(*s, RANK_RUN_OPEN, open);
            self.insert(*t, RANK_RUN_CLOSE, "</TextLabel>");
        }
    }
}

/// The Roblox selector for a CSS one, or why it has none.
pub fn roblox_selector(sel: &Selector, drop_placeholder: bool) -> Result<String, String> {
    let mut out = String::new();

    for (k, (comb, part)) in sel.parts.iter().enumerate() {
        if k > 0 {
            out.push_str(match comb {
                css::Combinator::Descendant => " >> ",

                css::Combinator::Child => " > ",

                _ => return Err("Roblox selectors have no sibling combinator, `+` or `~`".into()),
            });
        }

        out.push_str(&compound(
            part,
            drop_placeholder && k + 1 == sel.parts.len(),
        )?);
    }

    Ok(out)
}

fn compound(c: &Compound, drop_placeholder: bool) -> Result<String, String> {
    if let Some(u) = &c.unsupported {
        return Err(format!("`{u}` has no Roblox selector form"));
    }

    let mut out = String::new();

    match (&c.tag, c.ids.first()) {
        // An id is the instance's `Name`, and so is a tag with no id;
        // `div#main` is `#main`.
        (_, Some(id)) => out.push_str(&format!("#{id}")),

        (Some(t), None) if t.chars().next().is_some_and(char::is_uppercase) => out.push_str(t),

        (Some(t), None) => {
            if html::tag(t).is_none() {
                return Err(format!("`{t}` is not an HTML element Silk knows"));
            }

            out.push_str(&format!("#{t}"));
        }

        (None, None) => {}
    }

    for class in &c.classes {
        out.push_str(&format!(".{class}"));
    }

    for p in &c.pseudo {
        out.push_str(match p.as_str() {
            "hover" => ":Hover",

            "active" => ":Press",

            "disabled" => ":NonInteractable",

            "root" => return Err("`:root` holds variables; Silk reads its `--` properties and writes no rule".into()),

            other => return Err(format!("`:{other}` has no Roblox state; Roblox has `:hover`, `:active`, and `:disabled`")),
        });
    }

    match c.pseudo_element.as_deref() {
        None => {}

        Some("placeholder") if drop_placeholder => {}

        Some(p) if p.chars().next().is_some_and(char::is_uppercase) => {
            out.push_str(&format!("::{p}"))
        }

        Some(p) => return Err(format!("`::{p}` has no Roblox form")),
    }

    if out.is_empty() {
        return Err("`*` matches every instance, which a Roblox selector cannot say".into());
    }

    Ok(out)
}

/// The declarations of an element's `style`, either form, with no report.
fn decls_of(src: &str, e: &Element) -> Vec<css::Decl> {
    match e.attr("style").map(|a| a.value) {
        Some(Value::Expr(s, t)) => css::parse_table(&src[s..t], s).0,

        Some(Value::Str(s, t)) => css::parse_decls(&src[s..t], s),

        _ => Vec::new(),
    }
}

/// Whether a compound selector matches an element as written.
fn matches_static(c: &Compound, name: &str, id: Option<&str>, classes: &[&str]) -> bool {
    let tag_ok = match &c.tag {
        Some(t) => t == name,

        None => true,
    };
    let id_ok = c.ids.iter().all(|x| id == Some(x.as_str()));
    let class_ok = c.classes.iter().all(|x| classes.contains(&x.as_str()));

    tag_ok && id_ok && class_ok && (c.tag.is_some() || !c.ids.is_empty() || !c.classes.is_empty())
}

pub fn target_of(class: &str) -> Target {
    match class {
        "TextLabel" | "TextButton" => Target::Text,

        "TextBox" => Target::Input,

        "ImageLabel" | "ImageButton" => Target::Image,

        "VideoFrame" => Target::Video,

        "CanvasGroup" => Target::Canvas,

        _ => Target::Container,
    }
}

/// A RichText tag written as markup text: `\<b>`.
fn escape_tag(tag: &str) -> String {
    tag.replace('<', "\\<")
        .replace('{', "\\{")
        .replace('}', "\\}")
}

/// A Luau string for text written as is, on one line.
fn luau_string(text: &str) -> String {
    let body: String = text
        .chars()
        .map(|c| match c {
            '\\' => "\\\\".to_string(),

            '"' => "\\\"".to_string(),

            '\n' => "\\n".to_string(),

            '\r' => String::new(),

            '\t' => "\\t".to_string(),

            c => c.to_string(),
        })
        .collect();
    let body = body.strip_prefix("\\n").unwrap_or(&body);
    let body = body.strip_suffix("\\n").unwrap_or(body);

    format!("\"{body}\"")
}

/// `text` with the newlines `original` had, so the file keeps its lines.
fn pad(original: &str, mut text: String) -> String {
    let want = original.matches('\n').count();
    let have = text.matches('\n').count();

    if have < want {
        text.push_str(&"\n".repeat(want - have));
    }

    text
}

/// The helper as one line of Alloy: it tags an element and connects a
/// link. A link finds its target by `Name` from the top of its UI tree,
/// as `#id` finds an element in the document, and scrolls the target's
/// nearest ScrollingFrame to it; `#` scrolls the link's own to the top.
/// The helper needs a target whose elements are instances, as Vide and
/// Fusion build them.
pub fn helper_text(helper: &str) -> String {
    format!(
        "local function {helper}(make: () -> any, tags: string?, href: string?): any \
local el = make() \
if tags ~= nil then for tag in string.gmatch(tags, \"%S+\") do el:AddTag(tag) end end \
if href ~= nil then el.Activated:Connect(function() \
local root = el while root.Parent ~= nil and root.Parent:IsA(\"GuiBase2d\") do root = root.Parent end \
local target = if href == \"\" then nil else root:FindFirstChild(href, true) \
if href ~= \"\" and target == nil then return end \
local frame = (target or el).Parent while frame ~= nil and not frame:IsA(\"ScrollingFrame\") do frame = frame.Parent end \
if frame == nil then return end \
local at = if target ~= nil then target.AbsolutePosition - frame.AbsolutePosition + frame.CanvasPosition else Vector2.zero \
frame.CanvasPosition = Vector2.new(math.max(at.X, 0), math.max(at.Y, 0)) end) end \
return el end "
    )
}

/// The StyleSheet builder as one line of Alloy. Each `<style>` builds its
/// sheet once, however often the component renders.
pub fn sheet_text(helper: &str) -> String {
    format!(
        "local {helper}_sheets: {{ [string]: Instance }} = {{}} \
local function {helper}_sheet(id: string, rules: {{ {{ any }} }}): Instance \
local sheet = {helper}_sheets[id] if sheet ~= nil then return sheet end \
sheet = Instance.new(\"StyleSheet\") sheet.Name = id \
for _, r in rules do local rule = Instance.new(\"StyleRule\") rule.Selector = r[1] rule.Priority = r[2] rule:SetProperties(r[3]) rule.Parent = sheet end \
sheet.Parent = game:GetService(\"ReplicatedStorage\") {helper}_sheets[id] = sheet \
return sheet end "
    )
}

/// The byte where the prelude goes: the start of the first line that is
/// code, past the leading comments and blank lines.
pub fn helper_at(source: &str) -> usize {
    let mut at = 0usize;

    loop {
        let rest = &source[at..];
        let trimmed = rest.trim_start();
        let skipped = rest.len() - trimmed.len();

        if trimmed.is_empty() {
            return source.len();
        }

        if !trimmed.starts_with("--") {
            // Back to the start of the line, so the prelude opens it.
            let line_start = source[..at + skipped].rfind('\n').map_or(0, |n| n + 1);

            return line_start.max(at);
        }

        // A comment: a long one ends at its bracket, a line one at the
        // line's end; the prelude goes on the next line.
        let comment = at + skipped;
        let end = crate::markup::skip_comment(source, comment);
        at = source[end..]
            .find('\n')
            .map_or(source.len(), |n| end + n + 1);
    }
}

/// The escape helper as one line of Alloy: a value in RichText shows its
/// `<`, `>`, and `&` as themselves. A source reads through it.
pub fn rich_text(helper: &str) -> String {
    format!(
        "local function {helper}_rich(v: any): any \
if type(v) == \"function\" then local f: any = v return function() return {helper}_rich(f()) end end \
return (string.gsub(string.gsub(string.gsub(tostring(v), \"&\", \"&amp;\"), \"<\", \"&lt;\"), \">\", \"&gt;\")) end "
    )
}

/// Applies edits the way the host does, for tests.
#[cfg(test)]
pub fn apply(source: &str, edits: &[Edit]) -> String {
    let mut out = String::new();
    let mut cursor = 0usize;

    for e in edits {
        out.push_str(&source[cursor..e.0 as usize]);
        out.push_str(&e.2);
        cursor = e.1 as usize;
    }

    out.push_str(&source[cursor..]);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silk(src: &str) -> String {
        let out = run(src, "src/ui.alx", &Options::default());
        let text = apply(src, &out.edits);
        assert_eq!(
            text.matches('\n').count(),
            src.matches('\n').count(),
            "{text}"
        );

        text
    }

    #[test]
    fn a_div_is_a_frame_that_stacks_its_children() {
        let out = silk(
            "return <div id=\"root\">\n  <p>Hello</p>\n  <img src=\"rbxassetid://1\" width=\"64\" height=\"32\" />\n</div>\n",
        );

        assert!(
            !out.contains("<Frame Name={\"div\"}"),
            "an id names it: {out}"
        );
        assert!(out.contains("<Frame BorderSizePixel={0}"), "{out}");
        assert!(out.contains("Name=\"root\""), "{out}");
        assert!(
            out.contains("<UIListLayout SortOrder={Enum.SortOrder.LayoutOrder} />"),
            "{out}"
        );
        assert!(out.contains("<TextLabel Name={\"p\"}"), "{out}");
        assert!(out.contains("LayoutOrder={1}"), "{out}");
        assert!(out.contains("<ImageLabel Name={\"img\"}"), "{out}");
        assert!(out.contains("Size={UDim2.fromOffset(64, 32)}"), "{out}");
        assert!(out.contains("Image=\"rbxassetid://1\" />"), "{out}");
        assert!(out.contains("</Frame>"), "{out}");
    }

    #[test]
    fn inline_tags_fold_into_rich_text() {
        let out = silk("return <p>Hi <b>there</b>, {name}<br/>&copy; 2026 &amp; co</p>\n");

        assert!(out.contains("RichText={true}"), "{out}");
        assert!(
            out.contains("Hi \\<b>there\\</b>, {__silk_rich(name)}\\<br />\u{A9} 2026 &amp; co"),
            "{out}"
        );
    }

    #[test]
    fn a_style_becomes_a_style_link() {
        let out = silk(
            "return <div>\n<style>\n.card { background-color: #fff; border-radius: 8px }\n.card:hover { opacity: 0.5 }\n</style>\n</div>\n",
        );

        assert!(
            out.contains("<StyleLink StyleSheet={__silk_sheet(\"src/ui.alx#1\", {"),
            "{out}"
        );
        assert!(out.contains("{ \".card\", 100000, { BackgroundColor3 = Color3.fromRGB(255, 255, 255), BackgroundTransparency = 0 } }"), "{out}");
        assert!(
            out.contains("{ \".card::UICorner\", 100000, { CornerRadius = UDim.new(0, 8) } }"),
            "{out}"
        );
        assert!(out.contains("\".card:Hover\", 200001"), "{out}");
        assert!(out.contains("local function __silk_sheet"), "{out}");
    }

    #[test]
    fn a_class_tags_the_element_through_the_helper() {
        let out = silk(
            "return <div>\n  <button className=\"primary big\" onClick={go}>Go</button>\n</div>\n",
        );

        assert!(
            out.contains("{__silk(function() return <TextButton"),
            "{out}"
        );
        assert!(out.contains("Activated={go}"), "{out}");
        assert!(
            out.contains("</TextButton> end, \"primary big\", nil)}"),
            "{out}"
        );
        assert!(out.starts_with("local function __silk("), "{out}");
    }

    #[test]
    fn a_link_scrolls_to_its_target() {
        let out = silk("return <nav><a href=\"#faq\">FAQ</a></nav>\n");

        assert!(out.contains("<TextButton"), "{out}");
        assert!(out.contains(" end, nil, \"faq\")}"), "{out}");
        assert!(out.contains("\\<u>FAQ\\</u>"), "{out}");
    }

    #[test]
    fn inline_style_sets_properties_and_modifiers() {
        let out = silk(
            "return <div style={{ display = \"flex\", gap = 8, padding = 4, background = \"#222\" }}><span>a</span><button>b</button></div>\n",
        );

        assert!(
            out.contains("BackgroundColor3={Color3.fromRGB(34, 34, 34)}"),
            "{out}"
        );
        assert!(
            out.contains("<UIPadding PaddingTop={UDim.new(0, 4)}"),
            "{out}"
        );
        assert!(
            out.contains("FillDirection={Enum.FillDirection.Horizontal}"),
            "{out}"
        );
        assert!(out.contains("Padding={UDim.new(0, 8)}"), "{out}");
    }

    #[test]
    fn html_spellings_report_the_react_ones() {
        let src = "return <div class=\"a\" style=\"color: red\" onclick={go}></div>\n";
        let out = run(src, "a.alx", &Options::default());
        let lints: Vec<&str> = out.findings.iter().map(|f| f.lint.as_str()).collect();

        assert_eq!(
            lints.iter().filter(|l| **l == "react_name").count(),
            2,
            "{lints:?}"
        );
        assert!(lints.contains(&"react_style"), "{lints:?}");
        let fix = out
            .findings
            .iter()
            .find(|f| f.lint == "react_name")
            .unwrap()
            .fix
            .clone()
            .unwrap();
        assert_eq!(fix.2, "className");
    }

    #[test]
    fn a_list_numbers_its_items() {
        let out = silk("return <ol start=\"3\"><li>a</li><li>b</li></ol>\n");

        assert!(out.contains(">3. a</TextLabel>"), "{out}");
        assert!(out.contains(">4. b</TextLabel>"), "{out}");
    }

    #[test]
    fn a_box_with_text_and_children_wraps_the_text() {
        let out = silk("return <li>Title<ul><li>x</li></ul></li>\n");

        assert!(out.contains("<TextLabel Name={\"text\"}"), "{out}");
        assert!(out.contains("</TextLabel><Frame"), "{out}");
    }

    #[test]
    fn unsupported_tags_and_roblox_names_report() {
        let src = "return <div><select></select><Frame /></div>\n";
        let opts = Options {
            roblox: false,
            ..Options::default()
        };
        let out = run(src, "a.alx", &opts);
        let lints: Vec<&str> = out.findings.iter().map(|f| f.lint.as_str()).collect();

        assert!(lints.contains(&"unsupported_tag"), "{lints:?}");
        assert!(lints.contains(&"roblox_instance"), "{lints:?}");
    }

    #[test]
    fn enamel_takes_the_classes_when_loaded() {
        let src = "return <div className=\"flex gap-2 bg-red-500 card\"></div>\n";
        let opts = Options {
            enamel: true,
            ..Options::default()
        };
        let text = apply(src, &run(src, "a.alx", &opts).edits);

        assert!(
            text.contains("ClassName=\"flex gap-2 bg-red-500 card\""),
            "{text}"
        );
        assert!(!text.contains("BackgroundTransparency"), "{text}");
        assert!(!text.contains("UIListLayout"), "{text}");
    }

    #[test]
    fn a_button_with_one_hole_moves_it_to_text() {
        let out = silk("return <button>{label}</button>\n");

        assert!(out.contains("Text={label}"), "{out}");
        assert!(out.contains("<UIPadding"), "{out}");
    }

    fn lints(src: &str) -> Vec<String> {
        run(src, "a.alx", &Options::default())
            .findings
            .into_iter()
            .map(|f| f.lint)
            .collect()
    }

    /// S1, S6: text beside a box takes a TextLabel of its own, and the
    /// space between a box and its text is part of the text.
    #[test]
    fn text_beside_a_box_takes_its_own_label() {
        let out = silk("return <div>Price: <button>Buy</button></div>\n");
        assert!(out.contains("<TextLabel Name={\"text\"}"), "{out}");
        assert!(out.contains(">Price: </TextLabel>"), "{out}");

        let out = silk("return <section><h2>T</h2><span>alone</span></section>\n");
        assert!(!out.contains("<span"), "{out}");
        assert!(out.contains(">alone</TextLabel>"), "{out}");

        let out = silk("return <label><input type=\"text\" /> Remember me</label>\n");
        assert!(out.contains("> Remember me</TextLabel>"), "{out}");
        assert!(!out.contains("/> <"), "{out}");
    }

    /// S2: a component takes only the props it declares.
    #[test]
    fn a_component_in_a_box_takes_no_layout_order() {
        let out = silk("return <div><h1>T</h1><Card title=\"one\" /></div>\n");
        assert!(out.contains("<Card title=\"one\" />"), "{out}");
    }

    /// S3, S4, S27, S28: the reports the manifest declares.
    #[test]
    fn at_rules_unknown_attributes_and_void_tags_report() {
        assert!(
            lints("return <div><style>@media (x) { .a { color: red } }</style></div>\n")
                .contains(&"unsupported_css".to_string())
        );

        let src = "return <div><label htmlFor=\"n\">N</label><p onPointerDown={f}>x</p></div>\n";
        let out = silk(src);
        assert!(
            !out.contains("htmlFor") && !out.contains("onPointerDown"),
            "{out}"
        );
        assert!(lints(src).contains(&"unknown_attribute".to_string()));

        assert!(lints("return <p>a<br>b</p>\n").contains(&"void_tag".to_string()));

        let opts = Options {
            roblox: false,
            ..Options::default()
        };
        let f = run(
            "return <div><style>.x > UIListLayout { gap: 4px }</style></div>\n",
            "a.alx",
            &opts,
        );
        assert!(
            f.findings.iter().any(|f| f.lint == "roblox_instance"),
            "{:?}",
            f.findings
        );
    }

    /// S5: the prelude goes after a leading block comment.
    #[test]
    fn the_prelude_follows_a_block_comment() {
        let src = "--[[\n    The shop.\n]]\nreturn <div className=\"x\"><p>a</p></div>\n";
        let out = silk(src);
        assert!(
            out.starts_with("--[[\n    The shop.\n]]\nlocal function __silk("),
            "{out}"
        );
    }

    /// S7: text CSS on a box reaches the text inside it.
    #[test]
    fn text_css_on_a_box_is_inherited() {
        let out = silk(
            "return <div style={{ textAlign = \"center\", color = \"red\" }}><p>a</p></div>\n",
        );
        assert!(
            out.starts_with("return <Frame")
                && !out[..out.find("<TextLabel").unwrap()].contains("TextXAlignment"),
            "{out}"
        );
        assert!(
            out.contains("TextXAlignment={Enum.TextXAlignment.Center}"),
            "{out}"
        );
        assert!(
            out.contains("TextColor3={Color3.fromRGB(255, 0, 0)}"),
            "{out}"
        );
    }

    /// S8: the React form of a style reads the string.
    #[test]
    fn the_react_style_form_compiles() {
        let out = silk("return <div><style>{[[ .b { color: red; } ]]}</style></div>\n");
        assert!(out.contains("\".b\""), "{out}");
    }

    /// S10, S11, S12: holes in lists, preformatted text, and braces.
    #[test]
    fn holes_bullets_preformatted_text_and_braces() {
        let out = silk("return <ul>{items:map(function(i) return <li>{i}</li> end)}</ul>\n");
        assert!(out.contains("\u{2022} {i}"), "{out}");

        let out = silk("return <pre>line {n}\nnext</pre>\n");
        assert!(out.contains("Text={`line {n}\\nnext`}"), "{out}");

        let out = silk("return <p>&#123;x&#125;</p>\n");
        assert!(out.contains("\\{x\\}"), "{out}");
    }

    /// S19: a flex row sizes its children to their content, and a button
    /// with an icon lays it beside its text.
    #[test]
    fn a_row_sizes_its_children_to_their_content() {
        let out = silk("return <div style={{ display = \"flex\" }}><p>a</p><p>b</p></div>\n");
        assert!(!out.contains("<TextLabel Name={\"p\"} BorderSizePixel={0} BackgroundTransparency={1} TextColor3={Color3.fromRGB(0, 0, 0)} TextSize={16} TextWrapped={true} TextXAlignment={Enum.TextXAlignment.Left} TextYAlignment={Enum.TextYAlignment.Top} Size={UDim2.fromScale(1, 0)}"), "{out}");
        assert!(
            out.contains("AutomaticSize={Enum.AutomaticSize.XY}"),
            "{out}"
        );

        let out = silk("return <button><img src=\"x\" /> Buy</button>\n");
        assert!(
            out.contains("FillDirection={Enum.FillDirection.Horizontal}"),
            "{out}"
        );
        assert!(out.contains("> Buy</TextLabel>"), "{out}");
    }

    /// S16, S17, S23, S33: Enamel's classes and a written background.
    #[test]
    fn enamel_and_written_properties_keep_the_right_defaults() {
        let opts = Options {
            enamel: true,
            ..Options::default()
        };
        let text = |src: &str| apply(src, &run(src, "a.alx", &opts).edits);

        let out = text("return <div className=\"hover:bg-red-500\"><p>a</p></div>\n");
        assert!(out.contains("BackgroundTransparency={1}"), "{out}");

        let out = text("return <div className=\"items-center\"><p>a</p></div>\n");
        assert!(out.contains("ClassName=\"items-center flex-col\""), "{out}");

        let out = text("return <p className=\"text-sm\">a</p>\n");
        assert!(!out.contains("TextSize"), "{out}");

        let out = silk("return <div BackgroundColor3={Color3.new(1, 0, 0)}><p>a</p></div>\n");
        let own = &out[..out.find("<UIListLayout").unwrap()];
        assert!(!own.contains("BackgroundTransparency={1}"), "{own}");

        assert!(lints("return <input onClick={go} />\n").contains(&"no_effect".to_string()));
    }

    /// S13, S14, S26, S29: values and orders.
    #[test]
    fn roblox_values_media_and_table_order() {
        let out = silk(
            "return <div><style>.b { BackgroundColor3: rgba(0, 0, 0, 0.5); TextSize: 14px; CornerRadius: 4px }</style></div>\n",
        );
        assert!(
            out.contains("BackgroundColor3 = Color3.fromRGB(0, 0, 0)"),
            "{out}"
        );
        assert!(
            out.contains("TextSize = 14,") || out.contains("TextSize = 14 "),
            "{out}"
        );
        assert!(out.contains("CornerRadius = UDim.new(0, 4)"), "{out}");

        let out = silk("return <p style={{ TextSize = 20 }}>a</p>\n");
        assert!(out.contains("TextSize={20}"), "{out}");

        let out = silk("return <video muted />\n");
        assert!(out.contains("Volume={0}"), "{out}");

        let out = silk(
            "return <table><tbody><tr><td>a</td><td>b</td></tr><tr><td>c</td></tr></tbody></table>\n",
        );
        assert!(
            out.contains("<Frame Name={\"tr\"}") && out.contains("LayoutOrder={2}"),
            "{out}"
        );
    }

    /// Game UI 28: a block comment holds no tag, as a line comment does.
    #[test]
    fn a_tag_in_a_block_comment_is_no_tag() {
        let src = "--[[\n  Silk has no handler on `<input>` here.\n]]\nreturn <div><br>x</div>\n";
        let voids = run(src, "a.alx", &Options::default())
            .findings
            .into_iter()
            .filter(|f| f.lint == "void_tag")
            .count();

        assert_eq!(voids, 1, "only the `<br>` outside the comment");
    }

    #[test]
    fn a_progress_bar_fills() {
        let out = silk("return <progress value=\"0.25\" max=\"1\" />\n");

        assert!(
            out.contains("<Frame Size={UDim2.fromScale(0.25, 1)}"),
            "{out}"
        );
    }
}
