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
    /// The classes of the project's `enamel.aly`.
    pub theme: crate::enamel::Theme,
    /// The markup factory of the project.
    pub factory: Factory,
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
            theme: crate::enamel::Theme::new(),
            factory: Factory::default(),
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
    /// Silk elements that become a TextLabel, a TextButton, or a TextBox.
    text_boxes: HashSet<usize>,
    uses_rich: bool,
    uses_not: bool,
    uses_order: bool,
    uses_child: bool,
    uses_on: bool,
    uses_viewport: bool,
    /// The `<body>` of a document with a viewport, and its sizes.
    viewports: HashMap<usize, Viewport>,
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
/// The modifier and layout children of an element go before the wrapper
/// of its first child, which can start at the same byte.
const RANK_CHILDREN: i64 = 5;
const RANK_WRAP_OPEN: i64 = 10;
/// The order helper inside a hole wraps what the hole holds, markup and
/// its tag wrappers included.
const RANK_ORDER_OPEN: i64 = 5;
const RANK_ORDER_CLOSE: i64 = -5;
/// In a box that holds a `{ }` child, one place in the order is this many
/// numbers wide, so the items a hole holds keep their own order.
// ponytail: a hole keeps the order of 999 items; past that they run into
// the next place. Widen the step if a list grows that long.
const HOLE_STEP: usize = 1000;
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
            text_boxes: HashSet::new(),
            uses_rich: false,
            uses_not: false,
            uses_order: false,
            uses_child: false,
            uses_on: false,
            uses_viewport: false,
            viewports: HashMap::new(),
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
        self.cover_span(span);
    }

    /// Marks the elements inside a span as covered: an edit over the
    /// span already writes them.
    fn cover_span(&mut self, (s, t): (usize, usize)) {
        for (k, e) in self.m.elements.iter().enumerate() {
            if e.start >= s && e.end <= t {
                self.covered[k] = true;
            }
        }
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

        if self.uses_not {
            lead.push_str(&not_text(&self.opts.helper));
        }

        if self.uses_order {
            lead.push_str(&order_text(&self.opts.helper));
        }

        if self.uses_child {
            lead.push_str(&child_text(&self.opts.helper));
        }

        if self.uses_on {
            lead.push_str(&on_text(&self.opts.helper));
        }

        if self.uses_viewport {
            lead.push_str(&viewport_text(&self.opts.helper));
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

            // A child of a flex or a grid box is an item of its own, as CSS
            // makes it. An inline tag with a class keeps its instance for
            // the class, unless text stands beside it: RichText then holds
            // it, as a browser runs it on in the line.
            let item = e.parent.is_some_and(|p| self.lays_out_items(p));
            let classed = e.attr("className").or(e.attr("class")).is_some();

            // An element that places itself is out of the text's flow.
            self.foldable[i] = !events
                && !self.places_itself(i)
                && e.hole_owner.is_none_or(|_| e.parent.is_some())
                && e.parent.is_some()
                && children_fold
                && match tag.kind {
                    Kind::Break => true,

                    Kind::Inline => !item && (!classed || parent_text),

                    Kind::Link => {
                        !item && (!classed || parent_text) && (parent_text || parent_holds_text)
                    }

                    _ => false,
                };
        }
    }

    /// Whether a box is a flex or a grid container: its `style`, a rule
    /// of the file, or an Enamel class says so.
    fn lays_out_items(&self, i: usize) -> bool {
        let e = self.el(i);
        let display = decls_of(self.src, e)
            .into_iter()
            .chain(self.static_decls(i))
            .any(|d| {
                d.name == "display"
                    && matches!(
                        d.value.trim(),
                        "flex" | "inline-flex" | "grid" | "inline-grid"
                    )
            });
        let classes = ["class", "className"]
            .iter()
            .filter_map(|n| e.text(self.src, n))
            .flat_map(str::split_whitespace);
        let enamel = self.opts.enamel
            && crate::enamel::sets(classes, &self.opts.theme)
                .iter()
                .any(|k| k.starts_with("layout:") && k != crate::enamel::ARRANGE);

        display || enamel
    }

    /// Whether an element places itself: a `Position` attribute,
    /// `position: absolute` or `fixed`, an offset such as `top`, or an
    /// Enamel class that sets a position, as `absolute` and `inset-0` do.
    fn places_itself(&self, i: usize) -> bool {
        let e = self.el(i);
        let decls: Vec<css::Decl> = decls_of(self.src, e)
            .into_iter()
            .chain(self.static_decls(i))
            .collect();
        let mode = decls
            .iter()
            .rev()
            .find(|d| d.name == "position")
            .map(|d| d.value.trim().to_ascii_lowercase());
        let offset = decls.iter().any(|d| {
            matches!(
                d.name.as_str(),
                "top" | "left" | "right" | "bottom" | "inset"
            )
        });
        let css = matches!(mode.as_deref(), Some("absolute" | "fixed"))
            || (offset && mode.as_deref() != Some("static"));
        let classes = ["class", "className"]
            .iter()
            .filter_map(|n| e.text(self.src, n))
            .flat_map(str::split_whitespace);
        let enamel = self.opts.enamel && {
            let keys = crate::enamel::sets(classes, &self.opts.theme);

            keys.contains("Position") || keys.contains(crate::enamel::PLACED)
        };

        e.attr("Position").is_some() || css || enamel
    }

    fn has_literal(&self, i: usize) -> bool {
        self.el(i)
            .children
            .iter()
            .any(|c| matches!(c, Child::Text(s, e) if !self.src[*s..*e].trim().is_empty()))
    }

    /// The children that stand as their own boxes: not folded, not a
    /// `<style>`, not removed, and not a Roblox modifier such as a
    /// UIScale, which a layout does not place.
    fn boxes(&self, i: usize) -> Vec<usize> {
        self.m
            .element_children(i)
            .filter(|k| !self.foldable[*k])
            .filter(|k| {
                !self.tag(*k).is_some_and(|t| {
                    matches!(
                        t.kind,
                        Kind::Style | Kind::Removed | Kind::Break | Kind::Head | Kind::Meta
                    )
                })
            })
            .filter(|k| {
                let name = &self.el(*k).name;

                !roblox::is_class(name) || roblox::is_gui_object(name)
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
                // only the props it declares, so the order helper sets it
                // on what the component returns.
                if let Some(k) = self.order.get(&i).copied()
                    && e.attr("LayoutOrder").is_none()
                {
                    if roblox::is_gui_object(&e.name) {
                        self.insert(e.name_span.1, RANK_TEXT, format!(" LayoutOrder={{{k}}}"));
                    } else if !roblox::is_class(&e.name) {
                        let (open, close) = self.hole_braces(i);
                        self.uses_order = true;
                        self.insert(
                            e.start,
                            RANK_WRAP_OPEN,
                            format!("{open}{}_order((function() return ", self.opts.helper),
                        );
                        let tail = self.live(&format!("{k})"));
                        self.insert(e.end, RANK_WRAP_CLOSE, format!(" end)(), {tail}{close}"));
                    }
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

                Kind::Document => self.document(i, tag),

                Kind::Head | Kind::Meta => self.unsupported(
                    i,
                    &format!(
                        "`<{}>` belongs in the `<head>` of an `<html>` document",
                        tag.name
                    ),
                ),

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

    // ----------------------------------------------------------- document

    /// `<html>` becomes a ScreenGui. Its `<head>` makes no instance: the
    /// `<title>` and each `<meta>` set a property of the ScreenGui, and a
    /// `<style>` there becomes a StyleLink of the ScreenGui, which styles
    /// the whole document. The `<body>` is a box that fills the screen.
    fn document(&mut self, i: usize, tag: &'static Tag) {
        let e = self.el(i);
        let src = self.src;
        let mut props: Vec<(String, String)> = Vec::new();
        let mut viewport: Option<Viewport> = None;
        let mut name: Option<String> = None;

        for h in self.m.element_children(i).collect::<Vec<_>>() {
            let head = self.el(h);

            if head.name != "head" {
                continue;
            }

            self.covered[h] = true;

            match head.close {
                Some((s, t)) => {
                    self.replace(head.start, head.open_end, "");
                    self.replace(s, t, "");
                }

                None => self.replace(head.start, head.end, ""),
            }

            for c in head.children.clone() {
                match c {
                    Child::Text(s, t) => {
                        if !src[s..t].trim().is_empty() {
                            self.find("no_effect", (s, t), "text in the `<head>` shows nothing");
                        }

                        self.replace(s, t, "");
                    }

                    Child::Element(k) if matches!(self.el(k).name.as_str(), "title" | "meta") => {
                        self.covered[k] = true;

                        match self.el(k).name.as_str() {
                            "title" => name = self.title(k),

                            _ => self.meta(k, &mut props, &mut viewport),
                        }

                        let el = self.el(k);
                        self.replace(el.start, el.end, "");
                        self.cover_inside(k);
                    }

                    _ => {}
                }
            }
        }

        // A ScreenGui has no text: the space between the head and the body
        // goes, and other text reports.
        for c in e.children.clone() {
            if let Child::Text(s, t) = c {
                if !src[s..t].trim().is_empty() {
                    self.find(
                        "no_effect",
                        (s, t),
                        "a document shows text in its `<body>` alone",
                    );
                }

                self.replace(s, t, "");
            }
        }

        if let Some(vp) = viewport {
            match self
                .m
                .element_children(i)
                .find(|k| self.el(*k).name == "body")
            {
                Some(body) => {
                    self.viewports.insert(body, vp);
                }

                None => self.find(
                    "no_effect",
                    e.name_span,
                    "the viewport scales the `<body>`, and this document has none",
                ),
            }
        }

        // The attributes of `<html>`.
        let mut written: HashSet<String> = HashSet::new();

        for a in &e.attrs {
            let raw = match a.value {
                Value::Bare => Raw::Bare,

                Value::Str(s, t) => Raw::Str(&src[s..t]),

                Value::Expr(s, t) => Raw::Expr(&src[s..t]),
            };

            match html::map(tag, &a.name, raw, None, &self.opts.helper) {
                Mapped::Keep => {
                    if a.name.chars().next().is_some_and(char::is_uppercase) {
                        written.insert(a.name.clone());
                    }
                }

                Mapped::Props(list) => {
                    let text = list
                        .iter()
                        .map(|(k, v)| format!("{k}={{{}}}", self.live(v)))
                        .collect::<Vec<_>>()
                        .join(" ");
                    self.uses_not |= text.contains(&format!("{}_not(", self.opts.helper));
                    self.replace(a.span.0, a.span.1, text);

                    for (k, _) in list {
                        written.insert(k.to_string());
                    }
                }

                Mapped::Read if a.name == "id" && name.is_none() => {
                    self.replace(a.name_span.0, a.name_span.1, "Name");
                    written.insert("Name".into());
                }

                Mapped::Read
                    if matches!(a.name.as_str(), "className" | "class")
                        && self.opts.enamel
                        && matches!(a.value, Value::Str(..)) =>
                {
                    // Enamel's classes for a ScreenGui: `sibling-z`, `display-3`.
                    self.replace(a.name_span.0, a.name_span.1, "ClassName");
                }

                Mapped::Drop => self.remove_attr(a.span),

                Mapped::Unknown => {
                    self.find(
                        "unknown_attribute",
                        a.name_span,
                        format!(
                            "Silk does not know `{}` on `<html>`; it sets nothing",
                            a.name
                        ),
                    );
                    self.remove_attr(a.span);
                }

                _ => {
                    self.find(
                        "no_effect",
                        a.name_span,
                        format!(
                            "`{}` sets nothing on a ScreenGui; put it on the `<body>`",
                            a.name
                        ),
                    );
                    self.remove_attr(a.span);
                }
            }
        }

        let replaced = match self.opts.enamel {
            true => crate::enamel::sets(
                e.text(src, "className")
                    .or(e.text(src, "class"))
                    .unwrap_or("")
                    .split_whitespace(),
                &self.opts.theme,
            ),

            false => HashSet::new(),
        };
        // The values move to the open tag, and an edit keeps its line count.
        if name.as_ref().is_some_and(|n| n.contains('\n'))
            || props.iter().any(|(_, v)| v.contains('\n'))
        {
            self.find(
                "dynamic_style",
                e.name_span,
                "a title or a meta value on more than one line cannot move to the ScreenGui; bind it to a local and write the name",
            );
            name = name.filter(|n| !n.contains('\n'));
            props.retain(|(_, v)| !v.contains('\n'));
        }

        let mut generated = vec![
            (
                "Name".to_string(),
                name.unwrap_or_else(|| "\"html\"".into()),
            ),
            // CSS stacks by `z-index` among siblings.
            (
                "ZIndexBehavior".to_string(),
                "Enum.ZIndexBehavior.Sibling".to_string(),
            ),
        ];
        generated.extend(props);
        generated.retain(|(k, _)| !written.contains(k) && !replaced.contains(k));

        let attrs: String = generated
            .iter()
            .map(|(k, v)| format!(" {k}={{{v}}}"))
            .collect();
        self.replace(e.name_span.0, e.name_span.1, format!("ScreenGui{attrs}"));

        if let Some((s, _)) = e.close {
            self.replace(s + 2, s + 2 + e.name.len(), "ScreenGui");
        }
    }

    /// The `Name` a `<title>` gives: its text, its one hole, or both as
    /// an interpolated string.
    fn title(&self, k: usize) -> Option<String> {
        let e = self.el(k);
        let mut text = String::new();
        let mut holes = 0;

        for c in &e.children {
            match c {
                Child::Text(s, t) => {
                    let mut run = self.src[*s..*t].to_string();

                    // A `Name` is no RichText, so `&amp;` decodes here too.
                    let rich = [
                        ("lt", "<"),
                        ("gt", ">"),
                        ("quot", "\""),
                        ("apos", "'"),
                        ("amp", "&"),
                    ];

                    for (name, ch) in html::ENTITIES.iter().chain(&rich) {
                        run = run.replace(&format!("&{name};"), ch);
                    }

                    text.push_str(&run.replace('`', "\\`"));
                }

                Child::Hole(s, t) => {
                    holes += 1;
                    text.push_str(&self.src[*s..*t]);
                }

                _ => {}
            }
        }

        let words = text.split_whitespace().collect::<Vec<_>>().join(" ");

        match holes {
            _ if words.is_empty() => None,

            0 => Some(luau_string(&words.replace("\\`", "`"))),

            1 if words.starts_with('{') && words.ends_with('}') && !words[1..].contains('{') => {
                Some(words[1..words.len() - 1].trim().to_string())
            }

            _ => Some(format!("`{words}`")),
        }
    }

    /// The property one `<meta>` sets, or the viewport it asks for.
    fn meta(&mut self, k: usize, props: &mut Vec<(String, String)>, vp: &mut Option<Viewport>) {
        let e = self.el(k);
        let src = self.src;
        let span = e.name_span;
        let content = e.attr("content").map(|a| a.value);
        let text = e.text(src, "content").map(str::trim);
        let expr = match content {
            Some(Value::Expr(s, t)) => Some(src[s..t].trim().to_string()),

            _ => None,
        };
        let Some(name) = e.text(src, "name").map(str::to_ascii_lowercase) else {
            if e.attr("charset").is_none() && e.attr("http-equiv").is_none() {
                self.find(
                    "bad_value",
                    span,
                    "a `<meta>` sets nothing without a `name`",
                );
            }

            return;
        };
        // A flag is on with no content, as `<meta name="ignore-inset">`.
        let flag = |text: Option<&str>| match text.map(str::to_ascii_lowercase).as_deref() {
            None | Some("" | "true" | "yes" | "on" | "1") => Some(true),

            Some("false" | "no" | "off" | "0") => Some(false),

            _ => None,
        };

        match name.as_str() {
            "display-order" => match (text.map(str::parse::<i64>), expr) {
                (Some(Ok(n)), _) => props.push(("DisplayOrder".into(), n.to_string())),

                (_, Some(x)) => props.push(("DisplayOrder".into(), x)),

                _ => self.find(
                    "bad_value",
                    span,
                    "`display-order` takes a whole number: `content=\"10\"`",
                ),
            },

            "reset-on-spawn" | "ignore-inset" => {
                let value = match (&expr, flag(text)) {
                    (Some(x), _) => Some(Err(x.clone())),

                    (None, Some(on)) => Some(Ok(on)),

                    (None, None) => None,
                };

                match (name.as_str(), value) {
                    (_, None) => self.find(
                        "bad_value",
                        span,
                        format!("`{name}` takes `true` or `false`"),
                    ),

                    ("reset-on-spawn", Some(v)) => props.push((
                        "ResetOnSpawn".into(),
                        v.map_or_else(|x| x, |on| on.to_string()),
                    )),

                    // `ScreenInsets` is the modern property. A source goes to
                    // `IgnoreGuiInset`, which takes a boolean as it is.
                    (_, Some(Ok(on))) => props.push((
                        "ScreenInsets".into(),
                        match on {
                            true => "Enum.ScreenInsets.None",

                            false => "Enum.ScreenInsets.CoreUISafeInsets",
                        }
                        .into(),
                    )),

                    (_, Some(Err(x))) => props.push(("IgnoreGuiInset".into(), x)),
                }
            }

            "viewport" => match (text, expr) {
                (Some(t), _) => {
                    let (v, problems) = Viewport::parse(t);

                    for p in problems {
                        self.find("bad_value", span, p);
                    }

                    *vp = v;
                }

                _ => self.find(
                    "dynamic_style",
                    span,
                    "Silk reads the viewport when it compiles; write `content` as text",
                ),
            },

            other => self.find(
                "no_effect",
                span,
                format!(
                    "`{other}` sets nothing on a ScreenGui; Silk reads `display-order`, `ignore-inset`, `reset-on-spawn`, and `viewport`"
                ),
            ),
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
            "<{} StyleSheet={{{}_sheet(\"{}\", {{",
            self.child_tag("StyleLink"),
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
                // A StyleRule reaches one pseudo-instance, not the gradient
                // inside the stroke.
                if class.contains('.') {
                    if let Some(d) = group.iter().find(|d| d.name.starts_with("border-image")) {
                        self.find("no_effect", d.name_span, "a StyleRule cannot reach the gradient inside the stroke; put `border-image` in the element's `style`");
                    }

                    continue;
                }

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
                                        | "position"
                                        | "appearance"
                                        | "-webkit-appearance"
                                        | "top"
                                        | "left"
                                        | "right"
                                        | "bottom"
                                        | "inset"
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
        // A child that places itself stands out of the flow, as a CSS
        // `position: absolute` does. A UIListLayout would move it, so a box
        // with one writes no layout of its own, and its text stays its own.
        let placed = boxes.iter().any(|k| self.places_itself(*k));
        let flow_boxes: Vec<usize> = boxes
            .iter()
            .copied()
            .filter(|k| !self.places_itself(*k))
            .collect();
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
            Kind::Block if holds_text && flow_boxes.is_empty() => kind = Kind::Text,

            Kind::Text | Kind::Cell | Kind::Inline if !flow_boxes.is_empty() => kind = Kind::Block,

            _ => {}
        }

        let demoted =
            matches!(tag.kind, Kind::Text | Kind::Cell | Kind::Inline) && kind == Kind::Block;
        // A button or a link that holds a box, an icon beside its text,
        // lays both out in a row.
        let button_row = matches!(tag.kind, Kind::Button | Kind::Link) && !flow_boxes.is_empty();
        // Text beside a box stands in a TextLabel of its own, as a
        // browser puts it in an anonymous box.
        let wrap_runs = holds_text
            && !flow_boxes.is_empty()
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

        // An Enamel class that scrolls, `overflow-y-auto`, makes the box a
        // ScrollingFrame too.
        let enamel_scroll = match self.opts.enamel {
            true => crate::enamel::scroll(
                ["class", "className"]
                    .iter()
                    .filter_map(|n| e.text(src, n))
                    .flat_map(str::split_whitespace),
                &self.opts.theme,
            ),

            false => None,
        };
        let scroll = style.scroll.or(statics.scroll).or(enamel_scroll);

        if scroll.is_some() && class == "Frame" {
            class = "ScrollingFrame";
        }

        let layout_kind = style.layout_kind.or(statics.layout_kind);
        let text_class = matches!(class, "TextLabel" | "TextButton" | "TextBox");

        if text_class {
            self.text_boxes.insert(i);
        }

        // Enamel reads the same classes; Silk leaves the properties its
        // utilities set to it, and a background the author wrote keeps
        // its color.
        let class_tokens: Vec<&str> = ["class", "className"]
            .iter()
            .filter_map(|n| e.text(src, n))
            .flat_map(str::split_whitespace)
            .collect();
        let enamel = self.opts.enamel && !class_tokens.is_empty();
        // The end of the class list, where Silk adds a class for Enamel.
        let class_end = ["className", "class"]
            .iter()
            .find_map(|n| match e.attr(n)?.value {
                Value::Str(_, t) => Some(t),

                _ => None,
            })
            .filter(|_| enamel);
        let mut replaced = match enamel {
            true => crate::enamel::sets(class_tokens.iter().copied(), &self.opts.theme),

            false => HashSet::new(),
        };

        // A background color shows the box, so Silk's clear background goes.
        if replaced.contains("BackgroundColor3") || e.attr("BackgroundColor3").is_some() {
            replaced.insert("BackgroundTransparency".into());
        }

        let enamel_direction = [
            crate::enamel::ROW,
            crate::enamel::COLUMN,
            crate::enamel::GRID,
        ]
        .iter()
        .any(|k| replaced.contains(*k));
        let enamel_arranges = replaced.contains(crate::enamel::ARRANGE);
        // Enamel lays out children in a row unless a class says the
        // direction; a box stacks them, so Silk asks for a column.
        let enamel_column = enamel_arranges && !enamel_direction;
        let enamel_layout = enamel_direction || enamel_arranges;

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

        if class != "Sound" {
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

        let enamel_row =
            replaced.contains(crate::enamel::ROW) && !replaced.contains(crate::enamel::COLUMN);

        if button_row || (layout_kind == Some(Layout::Flex) && !column) || enamel_row {
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

        // The defaults a class of the element sets itself.
        d.props.retain(|(k, _)| !replaced.contains(k));
        d.mods.retain(|(c, _)| !replaced.contains(*c));

        // `appearance: none` drops the look a browser gives a control, as
        // Tailwind's reset does: the background, the border, the corner,
        // and the padding.
        let plain = style.plain
            || replaced.contains(crate::enamel::APPEARANCE)
            || static_decls
                .iter()
                .any(|d| d.name.ends_with("appearance") && d.value.trim() == "none");

        if plain && matches!(tag.kind, Kind::Button | Kind::Input | Kind::TextArea) {
            d.mods.clear();
            d.props.retain(|(k, _)| k != "BackgroundColor3");
            d.set("BackgroundTransparency", "1");
        }

        // A Luau value in `style` goes to the one property behind it, so a
        // source stays live: `style={{ scale = grow }}`. A property of the
        // element takes the value where the `style` stands.
        let dynamic = match e.attr("style").map(|a| a.value) {
            Some(Value::Expr(s, t)) => css::dynamic_table(&src[s..t], s),

            _ => Vec::new(),
        };
        let mut in_place: Vec<(&'static str, String)> = Vec::new();

        for (name, value, _) in &dynamic {
            let key = match name.as_str() {
                "background-color" => "BackgroundColor3",

                "color" if target == Target::Image => "ImageColor3",

                "color" => "TextColor3",

                "rotate" => "Rotation",

                "z-index" => "ZIndex",

                _ => continue,
            };
            in_place.push((key, value.clone()));

            // A background color shows the box.
            if key == "BackgroundColor3" && d.get("BackgroundTransparency") == Some("1") {
                in_place.push(("BackgroundTransparency", "0".into()));
            }
        }

        // ---- the attributes
        let mut written: HashSet<String> = HashSet::new();
        let mut extra: Vec<(String, String)> = Vec::new();
        let mut name_from_id = false;
        let mut tags_expr: Option<String> = None;
        let mut href: Option<String> = None;
        let mut change: Option<String> = None;
        // The handler stays where it stands until the helper takes it: an
        // edit keeps its line count, so a handler on more lines cannot move.
        let mut change_span: Option<(usize, usize)> = None;
        let mut limit: Option<String> = None;

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

            match html::map(tag, &a.name, raw, input_type, &self.opts.helper) {
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

                        // Vide types each event's handler by its signal, so a
                        // `() -> ()` for `Activated` fails the type check.
                        if let (true, Value::Expr(s, t)) = (self.opts.factory.table, a.value) {
                            self.uses_on = true;
                            let on = format!("{}_on(", self.opts.helper);
                            self.insert(s, RANK_WRAP_OPEN, on);
                            self.insert(t, RANK_WRAP_CLOSE, ")");
                        }
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
                        .map(|(k, v)| format!("{k}={{{}}}", self.live(v)))
                        .collect::<Vec<_>>()
                        .join(" ");
                    self.uses_not |= text.contains(&format!("{}_not(", self.opts.helper));
                    self.replace(a.span.0, a.span.1, text);
                    self.cover_span(a.span);

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

                    n if html::is_change(n) => {
                        change = Some(raw.luau());
                        change_span = Some(a.span);
                    }

                    n if n.eq_ignore_ascii_case("maxlength") => {
                        limit = Some(raw.luau().trim_matches('"').to_string());
                        self.remove_attr(a.span);
                    }

                    "style" if !in_place.is_empty() => {
                        let text = in_place
                            .iter()
                            .map(|(k, v)| format!("{k}={{{v}}}"))
                            .collect::<Vec<_>>()
                            .join(" ");
                        self.replace(a.span.0, a.span.1, text);
                        self.cover_span(a.span);

                        for (k, _) in &in_place {
                            written.insert((*k).to_string());
                        }
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

        // A Luau value for a modifier goes to its child. The child stands
        // past the open tag, and an edit keeps its line count, so a value
        // on more than one line stays out.
        let mut live: HashSet<&'static str> = HashSet::new();

        for (name, value, span) in &dynamic {
            let (class, key) = match name.as_str() {
                "scale" => ("UIScale", "Scale"),

                "border-color" => ("UIStroke", "Color"),

                "border-width" => ("UIStroke", "Thickness"),

                "border-transparency" => ("UIStroke", "Transparency"),

                "background-gradient" => ("UIGradient", "Color"),

                "gradient-rotation" => ("UIGradient", "Rotation"),

                "gradient-transparency" => ("UIGradient", "Transparency"),

                _ => continue,
            };

            if value.contains('\n') {
                self.find(
                    "dynamic_style",
                    *span,
                    format!("a value for `{name}` on more than one line cannot move to its {class}; bind it to a local and write the name"),
                );

                continue;
            }

            m.modifier(class, key, value.clone());
            live.insert(class);

            if class == "UIStroke" {
                m.modifier(class, "ApplyStrokeMode", "Enum.ApplyStrokeMode.Border");
            }
        }

        // A live gradient takes its direction from `gradientRotation`, or
        // from an Enamel direction class, `bg-gradient-to-b`. Silk writes
        // the one UIGradient, so the class leaves the list Enamel reads.
        // The box shows white under it, as a CSS gradient shows.
        if dynamic
            .iter()
            .any(|(n, v, _)| n == "background-gradient" && !v.contains('\n'))
        {
            let rotated = m
                .mods
                .iter()
                .any(|(c, p)| *c == "UIGradient" && p.iter().any(|(k, _)| k == "Rotation"));

            if let Some((span, deg)) = self.gradient_direction(i).filter(|_| self.opts.enamel) {
                if !rotated {
                    m.modifier("UIGradient", "Rotation", css::num(deg));
                }

                self.replace(span.0, span.1, "");
            }

            let shown = m.get("BackgroundColor3").is_some()
                || replaced.contains("BackgroundColor3")
                || written.contains("BackgroundColor3");

            if !shown {
                m.set("BackgroundColor3", "Color3.new(1, 1, 1)");
                m.set("BackgroundTransparency", "0");
            }
        }

        if let Some(base) = base_size {
            let w = style.width.unwrap_or(base.0);
            let h = style.height.unwrap_or(base.1);
            let named = |size: &str, auto: &str| (replaced.contains(size), replaced.contains(auto));
            let x = named(crate::enamel::SIZE_X, crate::enamel::AUTO_X);
            let y = named(crate::enamel::SIZE_Y, crate::enamel::AUTO_Y);

            let (size, auto) = props::size_props(w, h);
            m.set("Size", size);

            if x.0 || x.1 || y.0 || y.1 {
                // Enamel sets the axes its classes name inside Silk's
                // `Size`, and writes `AutomaticSize` from `w-auto` and
                // `h-auto` alone. So Silk passes its own automatic axis on
                // as that class, and writes no `AutomaticSize` itself.
                let more: String = [("w", x, w), ("h", y, h)]
                    .iter()
                    .filter(|(_, (sized, auto), value)| !sized && !auto && *value == Axis::Auto)
                    .map(|(axis, ..)| format!(" {axis}-auto"))
                    .collect();

                // An `AutomaticSize` the author writes wins.
                if let Some(t) = class_end
                    && !more.is_empty()
                    && !written.contains("AutomaticSize")
                {
                    self.insert(t, RANK_TEXT, more);
                }
            } else {
                m.set("AutomaticSize", auto);
            }

            // A rule of the file may give the height.
            let ruled = static_decls
                .iter()
                .any(|d| matches!(d.name.as_str(), "height" | "max-height"));

            if scroll.is_some() && matches!(h, Axis::Auto) && !ruled && !y.0 {
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

        if text_class {
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
        // A hole in a box is a child, as `{children}` is, and not text. So
        // is a hole beside a `Text` the author wrote.
        let holes_are_children = matches!(kind, Kind::Block | Kind::List | Kind::Table | Kind::Row)
            || written.contains("Text");
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

                let content = holds_text
                    || (has_holes && !holes_are_children)
                    || marker.is_some()
                    || written.contains("Text");
                // Enamel adds modifier and layout children of its own.
                let enamel_children = replaced
                    .iter()
                    .any(|k| k.starts_with("UI") || k.starts_with("layout:"));
                let nodes = !m.mods.is_empty() || !boxes.is_empty() || enamel_children;

                // Holes alone beside element children are ambiguous to the
                // markup compiler; one hole moves to `Text`.
                if !holds_text
                    && marker.is_none()
                    && wraps.is_empty()
                    && has_holes
                    && nodes
                    && !holes_are_children
                {
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

        // The holes that stand as children, not inside a run of text.
        let child_holes: Vec<(usize, usize)> = holes
            .iter()
            .filter(|(s, t)| holes_are_children && !runs.iter().any(|(a, b)| a <= s && t <= b))
            .copied()
            .collect();

        // In a class with a `Text` property the markup compiler reads a
        // hole as text; a fragment keeps it a child.
        if text_class {
            for (s, t) in &child_holes {
                self.insert(*s, RANK_WRAP_OPEN, "<>");
                self.insert(*t, RANK_WRAP_CLOSE, "</>");
            }
        }

        // The children get their order: boxes, anonymous text, and holes,
        // which set the order on what they hold through the helper. The
        // rows of a table's `thead` and `tbody` are the table's.
        if ordered {
            enum Slot {
                Element(usize),
                Run,
                Hole(usize),
            }

            let mut k = 0;
            let mut slots: Vec<(usize, Slot)> = Vec::new();
            let step = match child_holes.is_empty() {
                true => 1,

                false => HOLE_STEP,
            };

            for c in &e.children {
                match c {
                    Child::Element(x) if self.tag(*x).is_some_and(|t| t.kind == Kind::Group) => {
                        for row in self.m.element_children(*x) {
                            slots.push((self.el(row).start, Slot::Element(row)));
                        }
                    }

                    Child::Element(x) if boxes.contains(x) => {
                        slots.push((self.el(*x).start, Slot::Element(*x)))
                    }

                    _ => {}
                }
            }

            for (s, _) in &runs {
                slots.push((*s, Slot::Run));
            }

            for (s, t) in &child_holes {
                slots.push((*s, Slot::Hole(*t)));
            }

            slots.sort_by_key(|(at, _)| *at);
            let mut run_orders = HashMap::new();

            for (at, x) in slots {
                k += 1;
                let order = k * step;

                match x {
                    Slot::Element(x) => {
                        self.order.insert(x, order);
                    }

                    Slot::Run => {
                        run_orders.insert(at, order);
                    }

                    Slot::Hole(t) => {
                        self.uses_order = true;
                        let open = format!("{}_order(", self.opts.helper);
                        self.insert(at + 1, RANK_ORDER_OPEN, open);
                        let tail = self.live(&format!("{order})"));
                        self.insert(t - 1, RANK_ORDER_CLOSE, format!(", {tail}"));
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
            && !(placed && matches!(layout_kind, None | Some(Layout::Flow)))
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

        // The viewport scales the body about the middle of the screen.
        if self.viewports.contains_key(&i) {
            m.set("AnchorPoint", "Vector2.new(0.5, 0.5)");
            m.set("Position", "UDim2.fromScale(0.5, 0.5)");
        }

        // ---- write the element
        let mut generated: Vec<(String, String)> = extra;
        generated.extend(m.props.iter().cloned());
        generated.retain(|(k, _)| !written.contains(k) && !replaced.contains(k));
        let mut seen = HashSet::new();
        generated.retain(|(k, _)| seen.insert(k.clone()));

        let mut attrs: String = generated
            .iter()
            .map(|(k, v)| format!(" {k}={{{v}}}"))
            .collect();

        // The helper takes the tags, a link, a change handler, and a limit.
        let tags = match (&tags_expr, !class_tokens.is_empty()) {
            (Some(x), _) => Some(x.clone()),

            (None, true) => Some(format!("\"{}\"", class_tokens.join(" "))),

            _ => None,
        }
        .filter(|_| self.opts.tags);
        let tagged = tags.is_some() || href.is_some() || change.is_some() || limit.is_some();
        let nil = || "nil".to_string();
        let helper_args = format!(
            "{}, {}, {}, {}",
            tags.unwrap_or_else(nil),
            href.unwrap_or_else(nil),
            change.clone().unwrap_or_else(nil),
            limit.clone().unwrap_or_else(nil)
        );
        let viewport = self.viewports.get(&i).cloned();

        // In the element form an element is no instance, so React hands
        // the instance to the helpers as one `ref`, through a spread.
        if !self.opts.factory.table && (tagged || viewport.is_some()) {
            let fit = viewport.as_ref().map(|vp| {
                self.uses_viewport = true;

                format!("{}_viewport({})", self.opts.helper, vp.args())
            });
            let reference = match tagged {
                true => {
                    self.uses_helper = true;

                    format!(
                        "{}(nil, {helper_args}, {})",
                        self.opts.helper,
                        fit.unwrap_or_else(nil)
                    )
                }

                false => fit.unwrap_or_else(nil),
            };
            let spread = format!("{{{{ ref = {reference} }}}}");

            match change_span.take() {
                Some(span) => {
                    self.replace(span.0, span.1, spread);
                    self.cover_span(span);
                }

                None => attrs.push_str(&format!(" {spread}")),
            }
        }

        // Vide's factory has no VideoFrame and no Sound either.
        let (open_tag, close_tag) =
            match self.opts.factory.table && matches!(class, "VideoFrame" | "Sound") {
                true => (self.child_tag(class), format!("{}_child", self.opts.helper)),

                false => (class.to_string(), class.to_string()),
            };
        self.replace(e.name_span.0, e.name_span.1, format!("{open_tag}{attrs}"));

        if let Some((s, _)) = e.close {
            self.replace(s + 2, s + 2 + e.name.len(), close_tag.clone());
        }

        let mut children = String::new();

        for (c, props) in m
            .mods
            .iter()
            .map(|(c, p)| (*c, p))
            .chain(layout_child.iter().map(|(c, p)| (*c, p)))
        {
            let explicit = self.m.element_children(i).any(|k| self.el(k).name == *c);

            // A modifier inside another one, `UIStroke.UIGradient`, goes
            // into its outer child below.
            if explicit || c.contains('.') {
                continue;
            }

            let inner: String = m
                .mods
                .iter()
                .filter_map(|(k, props)| Some((k.strip_prefix(c)?.strip_prefix('.')?, props)))
                .map(|(class, props)| {
                    let p: String = props.iter().map(|(k, v)| format!(" {k}={{{v}}}")).collect();

                    (class, p)
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|(class, p)| format!("<{}{p} />", self.child_tag(class)))
                .collect();

            let mut p: String = props.iter().map(|(k, v)| format!(" {k}={{{v}}}")).collect();
            let tag = self.child_tag(c);

            // The child component builds a child with a live value through
            // the project's factory, which binds a source.
            if let Some(create) = &self.opts.factory.create
                && live.contains(c)
                && self.opts.factory.table
            {
                p.push_str(&format!(" Make={{{create}}}"));
            }

            match inner.is_empty() {
                true => children.push_str(&format!("<{tag}{p} />")),

                false => {
                    let close = tag.split(' ').next().unwrap_or(&tag).to_string();
                    children.push_str(&format!("<{tag}{p}>{inner}</{close}>"));
                }
            }
        }

        match e.self_close {
            Some((s, t)) => {
                let text = match children.is_empty() {
                    true => "/>".to_string(),

                    false => format!(">{children}</{close_tag}>"),
                };
                self.replace(s, t, text);
            }

            None if !children.is_empty() => self.insert(e.open_end, RANK_CHILDREN, children),

            None => {}
        }

        if let Some(span) = change_span {
            self.remove_attr(span);
        }

        // ---- the helper: tags, a link, a change, and the viewport. The
        // table form builds the element inside a function the helper calls
        // once: the markup compiler reads markup outside a function in a
        // child as a condition that never updates.
        if self.opts.factory.table && (tagged || viewport.is_some()) {
            let (open, close) = self.hole_braces(i);
            let mut pre = open.to_string();
            let mut post = String::new();

            if let Some(vp) = &viewport {
                self.uses_viewport = true;
                pre.push_str(&format!(
                    "{}_viewport({}, function() return ",
                    self.opts.helper,
                    vp.args()
                ));
                post = " end)".into();
            }

            if tagged {
                self.uses_helper = true;
                pre.push_str(&format!("{}(function() return ", self.opts.helper));
                post = format!(" end, {helper_args}){post}");
            }

            self.insert(e.start, RANK_WRAP_OPEN, pre);
            self.insert(e.end, RANK_WRAP_CLOSE, format!("{post}{close}"));
        }
    }

    /// The span of an Enamel direction class in the class list of an
    /// element, `bg-gradient-to-b`, and the rotation it names, as Enamel
    /// reads it.
    fn gradient_direction(&self, i: usize) -> Option<((usize, usize), f64)> {
        let e = self.el(i);
        let (s, t) = ["className", "class"]
            .iter()
            .find_map(|n| match e.attr(n)?.value {
                Value::Str(s, t) => Some((s, t)),

                _ => None,
            })?;
        let text = &self.src[s..t];
        let mut at = 0;

        for word in text.split_whitespace() {
            let start = at + text[at..].find(word)?;
            at = start + word.len();
            let Some(side) = word.strip_prefix("bg-gradient-to-") else {
                continue;
            };
            let deg = match side {
                "r" => 0.0,
                "br" => 45.0,
                "b" => 90.0,
                "bl" => 135.0,
                "l" => 180.0,
                "tl" => 225.0,
                "t" => 270.0,
                "tr" => 315.0,
                _ => continue,
            };

            return Some(((s + start, s + at), deg));
        }

        None
    }

    /// The end of a call of the negation, the escape, or the order
    /// helper, with the `compute` of the factory as its last argument
    /// when the project sets one. The helper derives a Fusion state
    /// through it, as Alloy derives text.
    fn live(&self, call: &str) -> String {
        match (&self.opts.factory.compute, call.strip_suffix(')')) {
            (Some((compute, _)), Some(head))
                if call == ")"
                    || call.starts_with(&format!("{}_not(", self.opts.helper))
                    || head.parse::<usize>().is_ok() =>
            {
                format!("{head}, {compute})")
            }

            _ => call.to_string(),
        }
    }

    /// The tag for a child Silk adds, a UIPadding or a StyleLink. In the
    /// table form the child component makes it: Vide types its factory
    /// over 19 classes, so `create("UIPadding")` fails the type check.
    fn child_tag(&mut self, class: &str) -> String {
        match self.opts.factory.table {
            true => {
                self.uses_child = true;

                format!("{}_child Class=\"{class}\"", self.opts.helper)
            }

            false => class.to_string(),
        }
    }

    /// The text around the helper call that stands for element `i`. Among
    /// children it is a `{ }` hole. In a class with a `Text` property the
    /// markup compiler reads a hole as text, so a fragment holds it there
    /// and it stays a child.
    fn hole_braces(&self, i: usize) -> (&'static str, &'static str) {
        let e = self.el(i);
        let in_text = e.parent.is_some_and(|p| {
            self.text_boxes.contains(&p)
                || matches!(
                    self.el(p).name.as_str(),
                    "TextLabel" | "TextButton" | "TextBox"
                )
        });

        match (e.in_children, in_text) {
            (true, true) => ("<>{", "}</>"),

            (true, false) => ("{", "}"),

            (false, _) => ("", ""),
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
            let close = self.live(")");
            self.insert(s + 1, RANK_WRAP_OPEN, helper);
            self.insert(t - 1, RANK_WRAP_CLOSE, close);
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

/// What `<meta name="viewport">` asks for: the body laid out at a design
/// size and scaled to the screen, as HTML's viewport scales a page.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Viewport {
    /// The design size in pixels; `device-width` leaves it unset.
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// The scale with no design size: `initial-scale`.
    pub initial: Option<f64>,
}

impl Viewport {
    /// Reads `width=1280, height=720, minimum-scale=0.5, maximum-scale=2`.
    /// Returns `None` when nothing scales, as for `width=device-width,
    /// initial-scale=1`, and the problems of the text.
    pub fn parse(content: &str) -> (Option<Self>, Vec<String>) {
        let mut v = Self::default();
        let mut problems = Vec::new();

        for part in content.split([',', ';']) {
            let Some((key, value)) = part.split_once('=') else {
                if !part.trim().is_empty() {
                    problems.push(format!("`{}` is not a `key=value` pair", part.trim()));
                }

                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            let number = value.trim_end_matches("px").parse::<f64>().ok();
            let slot = match key.as_str() {
                "width" if value == "device-width" => continue,

                "height" if value == "device-height" => continue,

                "width" => &mut v.width,

                "height" => &mut v.height,

                "minimum-scale" | "min-scale" => &mut v.min,

                "maximum-scale" | "max-scale" => &mut v.max,

                "initial-scale" => &mut v.initial,

                // A player cannot zoom a Roblox UI.
                "user-scalable" | "interactive-widget" | "viewport-fit" => continue,

                _ => {
                    problems.push(format!(
                        "`{key}` is not a viewport key Silk reads: `width`, `height`, `minimum-scale`, `maximum-scale`, `initial-scale`"
                    ));

                    continue;
                }
            };

            match number.filter(|n| *n > 0.0) {
                Some(n) => *slot = Some(n),

                None => problems.push(format!("`{key}` takes a number above 0, not `{value}`")),
            }
        }

        let scales = v.width.is_some() || v.height.is_some() || v.initial.is_some_and(|s| s != 1.0);

        (scales.then_some(v), problems)
    }

    /// The arguments of the viewport helper.
    fn args(&self) -> String {
        [self.width, self.height, self.min, self.max, self.initial]
            .iter()
            .map(|n| n.map_or("nil".to_string(), css::num))
            .collect::<Vec<_>>()
            .join(", ")
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

/// The helper as one line of Alloy: it tags an element, connects a link,
/// and listens to the text of an input for `onChange` and `maxLength`,
/// as React runs `onChange` on each key. A link finds its target by
/// `Name` from the top of its UI tree, as `#id` finds an element in the
/// document, and scrolls the target's nearest ScrollingFrame to it; `#`
/// scrolls the link's own to the top.
///
/// The table form calls it with `make`, which builds the element. The
/// element form calls it with nil, and it returns the function React
/// calls as a `ref`, again on each render. So it connects once for each
/// instance and keeps the latest handler, as React does. `after` is
/// another `ref` of the element, the viewport's.
pub fn helper_text(helper: &str) -> String {
    format!(
        "local {helper}_state: {{ [any]: any }} = setmetatable({{}}, {{ __mode = \"k\" }}) :: any \
local function {helper}(make: any, tags: string?, href: string?, change: any, limit: number?, after: any): any \
local function apply(value: any): any \
if after ~= nil then local fit: any = after fit(value) end \
if typeof(value) ~= \"Instance\" then return value end \
local el: any = value \
local s: any = {helper}_state[el] local first = s == nil \
if first then s = {{}} {helper}_state[el] = s end \
s.change = change s.limit = limit s.href = href \
if tags ~= nil then for tag in string.gmatch(tags, \"%S+\") do el:AddTag(tag) end end \
if first and (change ~= nil or limit ~= nil) then el:GetPropertyChangedSignal(\"Text\"):Connect(function() \
local cut = if s.limit ~= nil then utf8.offset(el.Text, s.limit + 1) else nil \
if cut ~= nil and cut <= #el.Text then el.Text = string.sub(el.Text, 1, cut - 1) return end \
local call: any = s.change if s.change ~= nil then call(el.Text) end end) end \
if first and href ~= nil then el.Activated:Connect(function() \
local link: string = s.href \
local root = el while root.Parent ~= nil and root.Parent:IsA(\"GuiBase2d\") do root = root.Parent end \
local target = if link == \"\" then nil else root:FindFirstChild(link, true) \
if link ~= \"\" and target == nil then return end \
local frame = (target or el).Parent while frame ~= nil and not frame:IsA(\"ScrollingFrame\") do frame = frame.Parent end \
if frame == nil then return end \
local at = if target ~= nil then target.AbsolutePosition - frame.AbsolutePosition + frame.CanvasPosition else Vector2.zero \
frame.CanvasPosition = Vector2.new(math.max(at.X, 0), math.max(at.Y, 0)) end) end \
return el end \
if make == nil then return apply end \
local build: any = make return apply(build()) end "
    )
}

/// The StyleSheet builder as one line of Alloy. Each `<style>` builds its
/// sheet once, however often the component renders.
pub fn sheet_text(helper: &str) -> String {
    format!(
        "local {helper}_sheets: {{ [string]: StyleSheet }} = {{}} \
local function {helper}_sheet(id: string, rules: {{ {{ any }} }}): StyleSheet \
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
/// `<`, `>`, and `&` as themselves. A reactive value reads through it, as
/// through the negation helper.
pub fn rich_text(helper: &str) -> String {
    format!(
        "local function {helper}_rich(v: any, compute: any?): any \
if type(v) == \"function\" then local f: any = v return function() return {helper}_rich(f()) end end \
if type(v) == \"table\" and v.map ~= nil then local b: any = v return b:map(function(x: any) return {helper}_rich(x) end) end \
if type(v) == \"table\" and compute ~= nil then local derive: any = compute return derive(function(use: any) return {helper}_rich(use(v)) end) end \
return (string.gsub(string.gsub(string.gsub(tostring(v), \"&\", \"&amp;\"), \"<\", \"&lt;\"), \">\", \"&gt;\")) end "
    )
}

/// The negation helper as one line of Alloy: `disabled={busy}` stays
/// live when `busy` is reactive. A Vide or Fluid source is a function, a
/// React binding maps, and a Fusion state derives through `compute`, the
/// function of `[alx.factory] compute` that Silk passes when it is set.
pub fn not_text(helper: &str) -> String {
    format!(
        "local function {helper}_not(v: any, compute: any?): any \
if type(v) == \"function\" then local f: any = v return function() return not f() end end \
if type(v) == \"table\" and v.map ~= nil then local b: any = v return b:map(function(x: any) return not x end) end \
if type(v) == \"table\" and compute ~= nil then local derive: any = compute return derive(function(use: any) return not use(v) end) end \
return not v end "
    )
}

/// The order helper as one line of Alloy. It sets `LayoutOrder` on what
/// a `{ }` child or a component gives: an instance, each instance of a
/// list, or, for a function a reactive library runs again, what the
/// function returns each time.
///
/// An item of a list takes its place in the list, unless it sets its
/// own `LayoutOrder`. Vide's `values` gives its items in the order of a
/// hash, and an item binds `LayoutOrder` to its index to keep the order
/// of the list. So the helper adds the place of the hole to the order an
/// item gives, and again each time the item changes it. `own` holds that
/// order per item, and `false` for an item that gives none. An order of
/// 1000 or more runs into the next place, as for [`HOLE_STEP`].
pub fn order_text(helper: &str) -> String {
    format!(
        "local {helper}_own: {{ [any]: any }} = setmetatable({{}}, {{ __mode = \"k\" }}) :: any \
local function {helper}_order(v: any, k: number, compute: any?): any \
if type(v) == \"function\" then local f: any = v return function() return {helper}_order(f(), k) end end \
if type(v) == \"table\" and compute ~= nil and v.type == \"State\" then local derive: any = compute return derive(function(use: any) return {helper}_order(use(v), k) end) end \
if typeof(v) == \"Instance\" then local g: any = v if g:IsA(\"GuiObject\") then g.LayoutOrder = k end return v end \
if type(v) ~= \"table\" then return v end \
for n, c in ipairs(v) do local g: any = c if typeof(c) == \"Instance\" and g:IsA(\"GuiObject\") then \
local s: any = {helper}_own[g] \
if s == nil then s = if g.LayoutOrder ~= 0 then {{ own = g.LayoutOrder, base = k, wrote = 0 }} else false {helper}_own[g] = s \
if s then g:GetPropertyChangedSignal(\"LayoutOrder\"):Connect(function() local t: any = {helper}_own[g] \
if g.LayoutOrder ~= t.wrote then t.own = g.LayoutOrder t.wrote = t.base + t.own g.LayoutOrder = t.wrote end end) end end \
if s then s.base = k s.wrote = k + s.own g.LayoutOrder = s.wrote else g.LayoutOrder = k + n - 1 end end end \
return v end "
    )
}

/// The child component as one line of Alloy, for the table form: it
/// makes a child with `Instance.new`, so no typed factory call names its
/// class. It connects a handler to a signal, as a VideoFrame takes one.
/// With `Make`, the project's factory builds the child, so a source in a
/// value stays live under Vide, Fluid, and Fusion alike.
pub fn child_text(helper: &str) -> String {
    format!(
        "local function {helper}_child(props: any): Instance \
if props.Make ~= nil then local make: any = props.Make local rest: any = {{}} \
for k, v in props do if k ~= \"Class\" and k ~= \"Make\" then rest[k] = v end end return make(props.Class)(rest) end \
local c: any = Instance.new(props.Class) \
local function adopt(x: any) if typeof(x) == \"Instance\" then x.Parent = c elseif type(x) == \"table\" then for _, y in x do adopt(y) end end end \
for k, v in props do if k == \"Class\" then continue end \
if type(k) ~= \"string\" then adopt(v) elseif typeof(c[k]) == \"RBXScriptSignal\" then c[k]:Connect(v) else c[k] = v end end return c end "
    )
}

/// The handler helper as one line of Alloy, for the table form. It takes
/// any function for an event and keeps a nil handler nil: Vide types
/// `Activated` as `(InputObject, number) -> ()`, and a `() -> ()` fails.
pub fn on_text(helper: &str) -> String {
    format!(
        "local function {helper}_on(f: any): any if f == nil then return nil end \
local h: any = f return function(...) h(...) end end "
    )
}

/// The viewport helper as one line of Alloy. It scales the body by the
/// camera's `ViewportSize` over the design size, clamped to the bounds,
/// with a UIScale, and sizes the body to the screen over that scale, so
/// inside it every size reads in design pixels. With `make`, it builds
/// the body and fits it, as the table form takes it. Without, it returns
/// the function that fits one, which React calls as a `ref` with the
/// instance, and with nil when the body goes.
// ponytail: it reads the camera that is current when the body mounts. A
// script that swaps `workspace.CurrentCamera` later leaves the scale on
// the old one; watch `CurrentCamera` if a game does that.
pub fn viewport_text(helper: &str) -> String {
    format!(
        "local function {helper}_viewport(w: number?, h: number?, lo: number?, hi: number?, base: number?, make: any): any \
local con: RBXScriptConnection? = nil \
local function fit(value: any): any \
if con ~= nil then con:Disconnect() con = nil end \
if typeof(value) ~= \"Instance\" then return value end \
local el: any = value \
local scale: any = el:FindFirstChild(\"viewport\") \
if scale == nil then scale = Instance.new(\"UIScale\") scale.Name = \"viewport\" scale.Parent = el end \
local camera = game:GetService(\"Workspace\").CurrentCamera \
local function resize() local s = base or 1 \
if camera ~= nil and (w ~= nil or h ~= nil) then local size = camera.ViewportSize s = math.min(if w ~= nil then size.X / w else math.huge, if h ~= nil then size.Y / h else math.huge) end \
s = math.clamp(s, lo or 0, hi or math.huge) if s <= 0 or s == math.huge then s = 1 end \
scale.Scale = s el.Size = UDim2.fromScale(1 / s, 1 / s) end \
resize() \
if camera ~= nil then local c = camera:GetPropertyChangedSignal(\"ViewportSize\"):Connect(resize) con = c el.Destroying:Connect(function() c:Disconnect() end) end \
return el end \
if make ~= nil then local build: any = make return fit(build()) end \
return fit end "
    )
}

/// The markup factory of a project: what `[alx.factory]` sets.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Factory {
    /// `backend = "table"`: a tag lowers to `create(name)(props)`, as
    /// Vide, Fluid, and Fusion take it. Else the element form, React's.
    pub table: bool,
    /// The function a tag calls: `vide.create`, `New`.
    pub create: Option<String>,
    /// `compute`, which wraps a value that reads Fusion state, and `use`,
    /// the reader inside it.
    pub compute: Option<(String, String)>,
}

impl Factory {
    /// The factory in the `[alx]` table the host sends at init. It comes
    /// from `alloy.toml` or `.config.aly` alike. An old host sends none,
    /// and Silk takes the element form, as Alloy does with no `[alx]`.
    pub fn from_alx(alx: &serde_json::Value) -> Self {
        let text = |key: &str| {
            alx["factory"]
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        };

        Self {
            table: text("backend").as_deref() == Some("table"),
            create: text("create"),
            compute: text("compute").map(|c| (c, text("use").unwrap_or_else(|| "use".into()))),
        }
    }
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
        let src =
            "return <div>\n  <button className=\"primary big\" onClick={go}>Go</button>\n</div>\n";
        let out = vide(src);

        assert!(
            out.contains("{__silk(function() return <TextButton"),
            "{out}"
        );
        assert!(out.contains("Activated={__silk_on(go)}"), "{out}");
        assert!(
            out.contains("</TextButton> end, \"primary big\", nil, nil, nil)}"),
            "{out}"
        );
        assert!(out.starts_with("local __silk_state"), "{out}");

        // React's element is no instance: the helper takes it as a `ref`.
        let out = silk(src);
        assert!(
            out.contains("{{ ref = __silk(nil, \"primary big\", nil, nil, nil, nil) }}"),
            "{out}"
        );
        assert!(out.contains("Activated={go}"), "{out}");
    }

    #[test]
    fn a_link_scrolls_to_its_target() {
        let out = vide("return <nav><a href=\"#faq\">FAQ</a></nav>\n");

        assert!(out.contains("<TextButton"), "{out}");
        assert!(out.contains(" end, nil, \"faq\", nil, nil)}"), "{out}");
        assert!(
            silk("return <nav><a href=\"#faq\">FAQ</a></nav>\n")
                .contains("{{ ref = __silk(nil, nil, \"faq\", nil, nil, nil) }}")
        );
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

    /// S2, game UI 14: a component takes only the props it declares; the
    /// order helper sets the order on what it returns.
    #[test]
    fn a_component_in_a_box_takes_its_order_through_the_helper() {
        let out = silk("return <div><h1>T</h1><Card title=\"one\" /></div>\n");
        assert!(
            out.contains("{__silk_order((function() return <Card title=\"one\" /> end)(), 2)}"),
            "{out}"
        );
        assert!(
            out.contains("local function __silk_order(v: any, k: number, compute: any?): any"),
            "{out}"
        );
    }

    /// Game UI 13: a `{ }` child keeps the order of every child, and the
    /// items it holds keep theirs.
    #[test]
    fn a_hole_child_keeps_the_order() {
        let out = silk(
            "return <div>\n  <h2>Create World</h2>\n  <p>NAME</p>\n  {seed_box}\n  <p>SEED</p>\n</div>\n",
        );
        assert!(out.contains("LayoutOrder={1000}"), "{out}");
        assert!(out.contains("LayoutOrder={2000}"), "{out}");
        assert!(out.contains("{__silk_order(seed_box, 3000)}"), "{out}");
        assert!(out.contains("LayoutOrder={4000}"), "{out}");

        // A hole that starts with markup: the helper wraps the tag wrapper.
        let out = vide("return <div><p>a</p>{<p className=\"x\">b</p>}</div>\n");
        assert!(
            out.contains("{__silk_order(__silk(function() return <TextLabel"),
            "{out}"
        );
        assert!(out.contains("end, \"x\", nil, nil, nil), 2000)}"), "{out}");

        // A clickable box keeps its hole a child, in a fragment.
        let out = silk("return <div onClick={go}><p>a</p>{extra}</div>\n");
        assert!(out.contains("<>{__silk_order(extra, 2000)}</>"), "{out}");
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
            out.starts_with("--[[\n    The shop.\n]]\nlocal __silk_state"),
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

    fn with_enamel(src: &str) -> String {
        let opts = Options {
            enamel: true,
            ..Options::default()
        };

        apply(src, &run(src, "a.alx", &opts).edits)
    }

    /// The table form, as Vide lowers it.
    fn vide_opts() -> Options {
        Options {
            factory: Factory {
                table: true,
                create: Some("vide.create".into()),
                compute: None,
            },
            ..Options::default()
        }
    }

    fn vide(src: &str) -> String {
        let text = apply(src, &run(src, "a.alx", &vide_opts()).edits);
        assert_eq!(
            text.matches('\n').count(),
            src.matches('\n').count(),
            "{text}"
        );

        text
    }

    /// Game UI 9 and 16: a class drops only the defaults it sets. A text
    /// color keeps the size and the alignment, and a gradient keeps the
    /// clear background.
    #[test]
    fn a_class_drops_only_the_default_it_sets() {
        let out = with_enamel("return <h1 className=\"text-white\">T</h1>\n");
        assert!(out.contains("TextSize={32}"), "{out}");
        assert!(out.contains("TextWrapped={true}"), "{out}");
        assert!(
            out.contains("TextXAlignment={Enum.TextXAlignment.Left}"),
            "{out}"
        );
        assert!(!out.contains("TextColor3"), "{out}");

        let out = with_enamel(
            "return <h1 className=\"bg-gradient-to-b from-accent to-orange-500\">T</h1>\n",
        );
        assert!(out.contains("BackgroundTransparency={1}"), "{out}");

        let out = with_enamel("return <button className=\"bg-red-500 stroke\">Go</button>\n");
        assert!(
            !out.contains("BackgroundColor3") && !out.contains("BackgroundTransparency"),
            "{out}"
        );
        assert!(
            !out.contains("<UIStroke") && out.contains("<UICorner"),
            "{out}"
        );
    }

    /// Game UI 10: a size class replaces Silk's size on its axis, and
    /// Silk names its own size on the other axis for Enamel.
    #[test]
    fn a_size_class_replaces_the_size_on_its_axis() {
        let out = with_enamel("return <div className=\"w-full h-full\" />\n");
        assert!(out.contains("Size={UDim2.fromScale(1, 0)}"), "{out}");
        assert!(!out.contains("AutomaticSize"), "{out}");
        assert!(out.contains("ClassName=\"w-full h-full\""), "{out}");

        let out = with_enamel("return <div className=\"w-full\" />\n");
        assert!(!out.contains("AutomaticSize"), "{out}");
        assert!(out.contains("ClassName=\"w-full h-auto\""), "{out}");

        let out = with_enamel("return <div className=\"h-full\" />\n");
        assert!(out.contains("ClassName=\"h-full\""), "{out}");

        let out = with_enamel("return <button className=\"w-auto\">Go</button>\n");
        assert!(out.contains("Size={UDim2.new()}"), "{out}");
        assert!(out.contains("ClassName=\"w-auto h-auto\""), "{out}");

        // A scale and an offset stay in Silk's Size for Enamel to merge.
        let out = with_enamel(
            "return <div className=\"h-10\" style={{ width = \"calc(100% - 8px)\" }} />\n",
        );
        assert!(out.contains("Size={UDim2.new(1, -8, 0, 0)}"), "{out}");
        assert!(out.contains("ClassName=\"h-10\""), "{out}");
    }

    /// Game UI 21: a theme class leaves out what its utilities set.
    #[test]
    fn a_theme_class_leaves_out_what_its_utilities_set() {
        let opts = Options {
            enamel: true,
            theme: crate::enamel::theme(
                "export const classes = { panel = 'bg-glass/75 rounded-2xl stroke stroke-white/15' }",
            ),
            ..Options::default()
        };
        let text = |src: &str| apply(src, &run(src, "a.alx", &opts).edits);

        let out = text("return <button className=\"panel\">Go</button>\n");
        assert!(!out.contains("Background"), "{out}");
        assert!(
            !out.contains("<UICorner") && !out.contains("<UIStroke"),
            "{out}"
        );
        assert!(out.contains("<UIPadding"), "{out}");

        let out = text("return <div className=\"panel\" />\n");
        assert!(!out.contains("BackgroundTransparency"), "{out}");
    }

    /// Game UI 11: a child of a flex box is a flex item, and a classed
    /// inline tag with no text beside it keeps its instance.
    #[test]
    fn a_flex_item_and_a_classed_span_keep_their_instances() {
        let out = with_enamel(
            "return <div className=\"flex justify-between\">\n  <span className=\"text-white\">L</span>\n  <span className=\"text-white\">R</span>\n</div>\n",
        );
        assert!(out.contains("<Frame Name={\"div\"}"), "{out}");
        assert_eq!(
            out.matches("<TextLabel Name={\"span\"}").count(),
            2,
            "{out}"
        );
        assert!(out.contains("LayoutOrder={2}"), "{out}");

        let out =
            silk("return <div style={{ display = \"flex\" }}><span>a</span><span>b</span></div>\n");
        assert_eq!(
            out.matches("<TextLabel Name={\"span\"}").count(),
            2,
            "{out}"
        );

        let src = "return <button className=\"group\"><span className=\"group-hover:text-yellow-400\">Inner</span></button>\n";
        let out = apply(
            src,
            &run(
                src,
                "a.alx",
                &Options {
                    enamel: true,
                    ..vide_opts()
                },
            )
            .edits,
        );
        assert!(out.contains("<TextLabel Name={\"span\"}"), "{out}");
        // A hole in a TextButton is text to the markup compiler; a
        // fragment keeps the wrapper a child.
        assert!(
            out.contains("<>{__silk(function() return <TextLabel"),
            "{out}"
        );
        assert!(out.contains("nil)}</></TextButton>"), "{out}");

        // Text beside it: the class has no instance, and the lint says so.
        let out = silk("return <p>Hi <b className=\"x\">there</b></p>\n");
        assert!(out.contains("\\<b>there\\</b>"), "{out}");
        assert!(
            lints("return <p>Hi <b className=\"x\">there</b></p>\n")
                .contains(&"no_effect".to_string())
        );
    }

    /// Game UI 20: a hole beside a `Text` the author wrote is a child, in
    /// a fragment so the markup compiler does not read it as text.
    #[test]
    fn a_hole_beside_text_is_a_child() {
        let out = silk("return <button Text={label}>{grow}</button>\n");
        assert!(out.contains("Text={label}"), "{out}");
        assert!(out.contains("<>{grow}</>"), "{out}");

        // A lone hole in a button whose classes replace every modifier
        // still moves to Text: Enamel adds the modifiers back as children.
        let out =
            with_enamel("return <button className=\"p-2 rounded-xl border-0\">{x}</button>\n");
        assert!(out.contains("Text={x}"), "{out}");
    }

    /// Game UI 19: `onChange` runs on each key with the text, as React
    /// runs it, and `maxLength` cuts the text.
    #[test]
    fn on_change_runs_on_each_key() {
        assert!(
            silk("return <input maxLength={24} onChange={f} />\n")
                .contains("{{ ref = __silk(nil, nil, nil, f, 24, nil) }}")
        );
        let out = vide("return <input maxLength={24} onChange={f} />\n");
        assert!(
            out.contains("return __silk(function() return <TextBox"),
            "{out}"
        );
        assert!(out.contains(" end, nil, nil, f, 24)"), "{out}");
        assert!(
            !out.contains("FocusLost") && !out.contains("maxLength"),
            "{out}"
        );
        assert!(out.contains("GetPropertyChangedSignal(\"Text\")"), "{out}");

        let out = vide("return <textarea onInput={name} className=\"x\"></textarea>\n");
        assert!(out.contains(" end, \"x\", nil, name, nil)"), "{out}");

        // A handler on more lines stays where it stands, inside the `ref`.
        let out = silk("return <input\n  onChange={function(t)\n    f(t)\n  end}\n/>\n");
        assert!(
            out.contains(
                "{{ ref = __silk(nil, nil, nil, function(t)\n    f(t)\n  end, nil, nil) }}"
            ),
            "{out}"
        );

        assert!(lints("return <div onChange={f}></div>\n").contains(&"no_effect".to_string()));
        assert!(!lints("return <input maxLength=\"4\" />\n").contains(&"no_effect".to_string()));
    }

    /// Game UI 26: in the table form, a child Silk adds and a VideoFrame
    /// come from the child component, and a handler goes through the
    /// handler helper, so Vide's typed factory takes the output.
    #[test]
    fn the_table_form_writes_what_a_typed_factory_takes() {
        let opts = Options {
            factory: Factory {
                table: true,
                ..Factory::default()
            },
            ..Options::default()
        };
        let src = "return <div>\n<style>.a { color: red }</style>\n<button onClick={go}>Go</button>\n<video src=\"x\" onMouseEnter={go} />\n</div>\n";
        let out = apply(src, &run(src, "a.alx", &opts).edits);

        assert!(
            out.contains("<__silk_child Class=\"StyleLink\" StyleSheet="),
            "{out}"
        );
        assert!(
            out.contains("<__silk_child Class=\"UIPadding\" PaddingTop="),
            "{out}"
        );
        assert!(
            out.contains("<__silk_child Class=\"UIListLayout\" SortOrder="),
            "{out}"
        );
        assert!(out.contains("Activated={__silk_on(go)}"), "{out}");
        assert!(
            out.contains("<__silk_child Class=\"VideoFrame\" Name={\"video\"}"),
            "{out}"
        );
        assert!(out.contains("MouseEnter={__silk_on(go)}"), "{out}");
        assert!(
            out.contains("local function __silk_child(props: any): Instance"),
            "{out}"
        );

        // The element form keeps the Roblox tags, which React needs.
        let out = silk(src);
        assert!(
            out.contains("<UIPadding ") && out.contains("Activated={go}"),
            "{out}"
        );
    }

    /// The factory comes from the `[alx]` table the host sends at init.
    #[test]
    fn the_factory_comes_from_the_host() {
        let vide = serde_json::json!({ "factory": { "backend": "table", "create": "vide.create", "compute": null } });
        assert_eq!(
            Factory::from_alx(&vide),
            Factory {
                table: true,
                create: Some("vide.create".into()),
                compute: None
            }
        );

        let fusion = serde_json::json!({ "factory": { "backend": "table", "create": "New", "compute": "computed" } });
        assert_eq!(
            Factory::from_alx(&fusion).compute,
            Some(("computed".into(), "use".into()))
        );

        let react = serde_json::json!({ "factory": { "backend": "element", "create": "React.createElement" } });
        assert!(!Factory::from_alx(&react).table);

        // An old host sends no `[alx]`: the element form, React's.
        assert_eq!(
            Factory::from_alx(&serde_json::Value::Null),
            Factory::default()
        );
    }

    /// Game UI 12: a classed child on the line of its box compiles as it
    /// does on a line of its own.
    #[test]
    fn a_classed_child_on_the_line_of_its_box_keeps_the_layout_outside() {
        let out = vide("return <div><p className=\"a\">x</p></div>\n");

        assert!(
            out.contains("<__silk_child Class=\"UIListLayout\" SortOrder={Enum.SortOrder.LayoutOrder} />{__silk(function() return <TextLabel"),
            "{out}"
        );
    }

    /// Game UI 15: a Roblox modifier inside a Silk tag takes no order
    /// and is no box beside the text.
    #[test]
    fn a_roblox_modifier_is_no_box() {
        let out = silk("return <button>\n  Play\n  <UIScale Scale={1.2} />\n</button>\n");

        assert!(out.contains("<UIScale Scale={1.2} />"), "{out}");
        assert!(
            !out.contains("<TextLabel"),
            "the text stays the button's: {out}"
        );
        assert!(!out.contains("UIListLayout"), "{out}");

        let out = silk("return <div><p>a</p><UIGradient /><Frame /></div>\n");
        assert!(out.contains("<UIGradient />"), "{out}");
        assert!(out.contains("<Frame LayoutOrder={2} />"), "{out}");
    }

    /// Game UI 17: `disabled`, `hidden`, and `readOnly` negate a source
    /// as a function that reads it.
    #[test]
    fn a_negated_attribute_reads_a_source() {
        let out = silk(
            "return <div><button disabled={busy}>Go</button><p hidden={busy}>x</p><input readOnly={busy} /><p hidden>y</p></div>\n",
        );

        assert!(out.contains("Interactable={__silk_not(busy)}"), "{out}");
        assert!(out.contains("AutoButtonColor={__silk_not(busy)}"), "{out}");
        assert!(out.contains("Visible={__silk_not(busy)}"), "{out}");
        assert!(out.contains("TextEditable={__silk_not(busy)}"), "{out}");
        assert!(out.contains("Visible={false}"), "{out}");
        assert!(
            out.contains(
                "local function __silk_not(v: any, compute: any?): any if type(v) == \"function\""
            ),
            "{out}"
        );
    }

    /// A negated source and an escaped hole stay live in every
    /// factory: the helpers map a React binding, and derive a Fusion
    /// state through the `compute` of the factory.
    #[test]
    fn a_negated_source_takes_each_factory() {
        let src = "return <div><button disabled={busy}>Go</button><p>Hi <b>{name}</b></p></div>\n";
        let fusion = Options {
            factory: Factory {
                table: true,
                create: Some("New".into()),
                compute: Some(("computed".into(), "use".into())),
            },
            ..Options::default()
        };
        let out = apply(src, &run(src, "a.alx", &fusion).edits);

        assert!(
            out.contains("Interactable={__silk_not(busy, computed)}"),
            "{out}"
        );
        let list = apply(
            "return <div><p>a</p>{items}</div>\n",
            &run("return <div><p>a</p>{items}</div>\n", "a.alx", &fusion).edits,
        );
        assert!(
            list.contains("{__silk_order(items, 2000, computed)}"),
            "{list}"
        );
        assert!(out.contains("{__silk_rich(name, computed)}"), "{out}");

        let script = format!(
            "{}{}\n{}",
            not_text("__silk"),
            rich_text("__silk"),
            r#"
local binding = { map = function(self, f) return f(true) end }
assert(__silk_not(binding) == false)
assert(__silk_rich({ map = function(self, f) return f("<b>") end }) == "&lt;b&gt;")
local state = { type = "State" }
local derived = __silk_not(state, function(f) return f(function(v) assert(v == state) return false end) end)
assert(derived == true)
assert(__silk_not(function() return false end)() == true)
assert(__silk_not(false) == true)
"#
        );
        let path = std::env::temp_dir().join(format!("silk-not-{}.luau", std::process::id()));
        std::fs::write(&path, script).unwrap();
        let run = std::process::Command::new("luau").arg(&path).output();
        let _ = std::fs::remove_file(&path);
        let Ok(out) = run else {
            eprintln!("no luau on PATH: the helper run is skipped");

            return;
        };

        assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Game UI 18: markup inside an attribute lowers as it does in a
    /// body hole.
    #[test]
    fn markup_inside_an_attribute_lowers() {
        let out = silk("return <Panel title=\"Hi\" children={[<p>one</p>, <p>two</p>]} />\n");

        assert_eq!(out.matches("<TextLabel Name={\"p\"}").count(), 2, "{out}");
        assert!(out.contains(">one</TextLabel>, <TextLabel"), "{out}");

        // An attribute Silk drops takes its markup with it.
        let out = silk("return <div title={<p>x</p>}></div>\n");
        assert!(!out.contains("title") && !out.contains("<p"), "{out}");
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

    const DOCUMENT: &str = "return <html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>Menu &amp; more</title>\n    <meta name=\"display-order\" content=\"10\" />\n    <meta name=\"ignore-inset\" />\n    <meta name=\"reset-on-spawn\" content=\"false\" />\n    <meta name=\"viewport\" content=\"width=1280, height=720, minimum-scale=0.5, maximum-scale=2\" />\n    <style>.card { color: red }</style>\n  </head>\n  <body>\n    <p>Hi</p>\n  </body>\n</html>\n";

    /// `<html>` is a ScreenGui that the head configures: the title names
    /// it, each meta sets a property, and the style links to it.
    #[test]
    fn a_document_makes_a_screen_gui() {
        let out = silk(DOCUMENT);

        assert!(
            out.contains("<ScreenGui Name={\"Menu & more\"} ZIndexBehavior={Enum.ZIndexBehavior.Sibling} DisplayOrder={10} ScreenInsets={Enum.ScreenInsets.None} ResetOnSpawn={false}>"),
            "{out}"
        );
        assert!(out.contains("</ScreenGui>"), "{out}");
        assert!(
            out.contains("<StyleLink StyleSheet={__silk_sheet("),
            "{out}"
        );
        assert!(
            !out.contains("<head") && !out.contains("<meta") && !out.contains("<title"),
            "{out}"
        );
        assert!(!out.contains("lang") && !out.contains("charset"), "{out}");
        // React's body takes the viewport as a `ref`.
        assert!(
            out.contains("<Frame Name={\"body\"}") && out.contains("Size={UDim2.fromScale(1, 1)}"),
            "{out}"
        );
        assert!(
            out.contains("AnchorPoint={Vector2.new(0.5, 0.5)} Position={UDim2.fromScale(0.5, 0.5)} {{ ref = __silk_viewport(1280, 720, 0.5, 2, nil) }}>"),
            "{out}"
        );
        assert!(
            out.contains("local function __silk_viewport(w: number?, h: number?, lo: number?"),
            "{out}"
        );

        let findings: Vec<String> = run(DOCUMENT, "a.alx", &Options::default())
            .findings
            .into_iter()
            .map(|f| f.lint)
            .collect();
        assert!(findings.is_empty(), "{findings:?}");
    }

    /// The document compiles under each factory: the table form builds
    /// the body inside the viewport helper, and the element form hands it
    /// the instance as a `ref`.
    #[test]
    fn a_document_takes_each_factory() {
        let table = |create: &str, compute: Option<(&str, &str)>| Options {
            factory: Factory {
                table: true,
                create: Some(create.into()),
                compute: compute.map(|(c, u)| (c.into(), u.into())),
            },
            ..Options::default()
        };
        let vide = table("vide.create", None);
        let fusion = table("New", Some(("computed", "use")));
        let react = Options {
            factory: Factory {
                table: false,
                create: Some("React.createElement".into()),
                compute: None,
            },
            ..Options::default()
        };

        for opts in [&vide, &fusion] {
            let out = apply(DOCUMENT, &run(DOCUMENT, "a.alx", opts).edits);

            assert!(
                out.contains("{__silk_viewport(1280, 720, 0.5, 2, nil, function() return <Frame Name={\"body\"}"),
                "{out}"
            );
            assert!(out.contains("</Frame> end)}"), "{out}");
            assert!(!out.contains("ref ="), "{out}");
            assert!(
                out.contains("<__silk_child Class=\"StyleLink\" StyleSheet="),
                "{out}"
            );
            assert!(out.contains("<ScreenGui Name={\"Menu & more\"}"), "{out}");
        }

        let out = apply(DOCUMENT, &run(DOCUMENT, "a.alx", &react).edits);
        assert!(
            out.contains("{{ ref = __silk_viewport(1280, 720, 0.5, 2, nil) }}"),
            "{out}"
        );

        assert!(out.contains("<StyleLink StyleSheet="), "{out}");

        // A body with a class takes one `ref` that runs both helpers.
        let classed = DOCUMENT.replace("<body>", "<body className=\"card\">");
        let out = apply(&classed, &run(&classed, "a.alx", &react).edits);
        assert!(
            out.contains("{{ ref = __silk(nil, \"card\", nil, nil, nil, __silk_viewport(1280, 720, 0.5, 2, nil)) }}"),
            "{out}"
        );
        let out = apply(&classed, &run(&classed, "a.alx", &vide).edits);
        assert!(
            out.contains("{__silk_viewport(1280, 720, 0.5, 2, nil, function() return __silk(function() return <Frame Name={\"body\"}"),
            "{out}"
        );

        // A source reaches each property as it is.
        let src = "return <html><head><title>{name}</title><meta name=\"display-order\" content={order} /><meta name=\"ignore-inset\" content={full} /><meta name=\"reset-on-spawn\" /></head><body>{children}</body></html>\n";

        for opts in [&vide, &fusion, &react] {
            let out = apply(src, &run(src, "a.alx", opts).edits);

            assert!(
                out.contains("<ScreenGui Name={name} ZIndexBehavior={Enum.ZIndexBehavior.Sibling} DisplayOrder={order} IgnoreGuiInset={full} ResetOnSpawn={true}>"),
                "{out}"
            );
            assert!(!out.contains("viewport"), "{out}");
        }
    }

    #[test]
    fn the_viewport_reads_the_html_keys() {
        let (v, problems) = Viewport::parse(
            "width=1280px; height=720, min-scale=0.5, maximum-scale=2, user-scalable=no",
        );
        assert_eq!(
            v,
            Some(Viewport {
                width: Some(1280.0),
                height: Some(720.0),
                min: Some(0.5),
                max: Some(2.0),
                initial: None,
            })
        );
        assert!(problems.is_empty(), "{problems:?}");

        // HTML's usual line scales nothing.
        assert_eq!(
            Viewport::parse("width=device-width, initial-scale=1"),
            (None, Vec::new())
        );

        let (_, problems) = Viewport::parse("width=wide, zoom=2");
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    /// The viewport helper runs under `luau` against the mock camera of
    /// `tests/viewport.luau`. A machine without `luau` skips the run.
    #[test]
    fn the_viewport_helper_runs_against_a_mock() {
        let script = format!(
            "{}\n{}",
            viewport_text("__silk"),
            include_str!("../tests/viewport.luau")
        );
        let path = std::env::temp_dir().join(format!("silk-viewport-{}.luau", std::process::id()));
        std::fs::write(&path, script).unwrap();
        let run = std::process::Command::new("luau").arg(&path).output();
        let _ = std::fs::remove_file(&path);
        let Ok(out) = run else {
            eprintln!("no luau on PATH: the helper run is skipped");

            return;
        };

        assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// LANG_BUGS 65: the items of `vide.values` come in the order of a
    /// hash, and each binds `LayoutOrder` to its index. The order helper
    /// adds the place of the hole to that order and follows it. Each
    /// factory writes the one helper; `tests/order.luau` runs it under
    /// `luau`, and a machine without `luau` skips the run.
    #[test]
    fn a_list_item_keeps_the_order_it_binds() {
        let src = "return <div className=\"flex-col\">\n  {vide.values(lines, function(line, index)\n    return <p LayoutOrder={index}>{line}</p>\n  end)}\n</div>\n";
        let fusion = Options {
            factory: Factory {
                table: true,
                create: Some("New".into()),
                compute: Some(("computed".into(), "use".into())),
            },
            ..Options::default()
        };

        for (out, tail) in [
            (vide(src), "end), 1000)}"),
            (
                apply(src, &run(src, "a.alx", &fusion).edits),
                "end), 1000, computed)}",
            ),
            (silk(src), "end), 1000)}"),
        ] {
            assert!(out.contains("{__silk_order(vide.values("), "{out}");
            assert!(out.contains(tail), "{out}");
            assert!(out.contains("local __silk_own"), "{out}");
            assert!(out.contains("LayoutOrder={index}"), "{out}");
        }

        let script = format!(
            "{}\n{}",
            order_text("__silk"),
            include_str!("../tests/order.luau")
        );
        let path = std::env::temp_dir().join(format!("silk-order-{}.luau", std::process::id()));
        std::fs::write(&path, script).unwrap();
        let run = std::process::Command::new("luau").arg(&path).output();
        let _ = std::fs::remove_file(&path);
        let Ok(out) = run else {
            eprintln!("no luau on PATH: the helper run is skipped");

            return;
        };

        assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// The tag helper runs under `luau` against the mock of
    /// `tests/helper.luau`, in both forms. A machine without `luau`
    /// skips the run.
    #[test]
    fn the_tag_helper_runs_against_a_mock() {
        let script = format!(
            "{}\n{}",
            helper_text("__silk"),
            include_str!("../tests/helper.luau")
        );
        let path = std::env::temp_dir().join(format!("silk-helper-{}.luau", std::process::id()));
        std::fs::write(&path, script).unwrap();
        let run = std::process::Command::new("luau").arg(&path).output();
        let _ = std::fs::remove_file(&path);
        let Ok(out) = run else {
            eprintln!("no luau on PATH: the helper run is skipped");

            return;
        };

        assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// The child component hands a child with `Make` to the factory,
    /// without the `Class` and the `Make`.
    #[test]
    fn the_child_component_builds_through_make() {
        let script = format!(
            "{}\nlocal made: any = nil\nlocal function make(class) return function(props) made = {{ class = class, props = props }} return 7 end end\nassert(__silk_child({{ Class = \"UIScale\", Scale = 2, Make = make }}) == 7)\nassert(made.class == \"UIScale\" and made.props.Scale == 2 and made.props.Make == nil and made.props.Class == nil)\nlocal INST = {{}}\ntypeof = function(v) return if getmetatable(v) == INST then \"Instance\" else type(v) end\nInstance = {{ new = function(c) return setmetatable({{ ClassName = c }}, INST) end }}\nlocal g = Instance.new(\"UIGradient\")\nlocal stroke = __silk_child({{ Class = \"UIStroke\", Thickness = 6, g }})\nassert(stroke.Thickness == 6 and g.Parent == stroke)\n",
            child_text("__silk")
        );
        let path = std::env::temp_dir().join(format!("silk-child-{}.luau", std::process::id()));
        std::fs::write(&path, script).unwrap();
        let run = std::process::Command::new("luau").arg(&path).output();
        let _ = std::fs::remove_file(&path);
        let Ok(out) = run else {
            eprintln!("no luau on PATH: the helper run is skipped");

            return;
        };

        assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[test]
    fn a_head_tag_outside_a_document_reports() {
        let out = run(
            "return <div><meta name=\"viewport\" content=\"width=1\" /><title>x</title></div>\n",
            "a.alx",
            &Options::default(),
        );
        let unsupported = out
            .findings
            .iter()
            .filter(|f| f.lint == "unsupported_tag")
            .count();

        assert_eq!(unsupported, 2, "{:?}", out.findings);

        let lints = lints(
            "return <html><head><meta name=\"theme-color\" content=\"#fff\" /><meta name=\"viewport\" content=\"width=10\" /></head></html>\n",
        );
        assert!(lints.contains(&"no_effect".to_string()), "{lints:?}");

        // A value on more lines cannot move to the open tag.
        let src = "return <html><head><meta name=\"display-order\" content={if a\n  then 1 else 2} /></head><body /></html>\n";
        assert!(
            silk(src).contains(
                "<ScreenGui Name={\"html\"} ZIndexBehavior={Enum.ZIndexBehavior.Sibling}>"
            )
        );
        assert!(
            run(src, "a.alx", &Options::default())
                .findings
                .iter()
                .any(|f| f.lint == "dynamic_style")
        );
    }

    /// A child that places itself stands out of the flow, so its box
    /// writes no layout: a UIListLayout would move it.
    #[test]
    fn a_placed_child_turns_the_layout_off() {
        let out = silk(
            "return <div><p>a</p><div style={{ position = \"absolute\", top = 0 }}>b</div></div>\n",
        );
        assert!(!out.contains("UIListLayout"), "{out}");

        let out = silk("return <div><Frame Position={UDim2.new()} /><p>a</p></div>\n");
        assert!(!out.contains("UIListLayout"), "{out}");

        let out = with_enamel(
            "return <div className=\"w-full\">{children}<div className=\"absolute inset-0\" /></div>\n",
        );
        assert!(!out.contains("UIListLayout"), "{out}");

        // A button keeps its text beside a badge that places itself.
        let out = with_enamel(
            "return <button>Play<span className=\"absolute top-0 right-0\">new</span></button>\n",
        );
        assert!(!out.contains("UIListLayout"), "{out}");
        assert!(!out.contains("Name={\"text\"}"), "{out}");
        assert!(out.contains(">Play<"), "{out}");

        // A layout the author asks for stays.
        let out = silk(
            "return <div style={{ display = \"flex\" }}><span style={{ position = \"absolute\" }}>x</span></div>\n",
        );
        assert!(out.contains("UIListLayout"), "{out}");

        // `position: relative` with no offset flows.
        let out = silk("return <div><p style={{ position = \"relative\" }}>a</p><p>b</p></div>\n");
        assert!(out.contains("UIListLayout"), "{out}");
    }

    /// An Enamel overflow class makes a ScrollingFrame whose canvas
    /// grows with its content, as `overflow: auto` in CSS does.
    #[test]
    fn an_overflow_class_scrolls() {
        let out = with_enamel(
            "return <div className=\"w-full h-[400px] overflow-y-auto flex flex-wrap gap-4\"><p>a</p></div>\n",
        );

        assert!(out.contains("<ScrollingFrame Name={\"div\"}"), "{out}");
        assert!(
            out.contains("ScrollingDirection={Enum.ScrollingDirection.Y} CanvasSize={UDim2.new()} AutomaticCanvasSize={Enum.AutomaticSize.Y}"),
            "{out}"
        );
        assert!(out.contains("</ScrollingFrame>"), "{out}");

        // Enamel's own direction class wins over Silk's.
        let out =
            with_enamel("return <div className=\"h-10 overflow-auto scroll-x\"><p>a</p></div>\n");
        assert!(out.contains("<ScrollingFrame"), "{out}");
        assert!(!out.contains("ScrollingDirection="), "{out}");
    }

    /// `appearance: none` drops the look a browser gives a control, so a
    /// bare TextButton needs no Roblox tag.
    #[test]
    fn appearance_none_drops_the_browser_look() {
        let out = silk("return <button style={{ appearance = \"none\" }}>Go</button>\n");

        assert!(out.contains("BackgroundTransparency={1}"), "{out}");
        assert!(!out.contains("BackgroundColor3"), "{out}");
        assert!(
            !out.contains("UIPadding") && !out.contains("UIStroke") && !out.contains("UICorner"),
            "{out}"
        );

        let out =
            with_enamel("return <button className=\"appearance-none size-9 rounded-lg\" />\n");
        assert!(
            !out.contains("UIStroke") && !out.contains("UIPadding"),
            "{out}"
        );

        let out = silk(
            "return <div><style>.x { appearance: none; }</style><input className=\"x\" /></div>\n",
        );
        assert!(!out.contains("UIStroke"), "{out}");

        // A box keeps its look: `appearance` is for controls.
        assert!(
            silk("return <ul style={{ appearance = \"none\" }}><li>a</li></ul>\n")
                .contains("UIPadding")
        );
    }

    /// A Luau value in `style` reaches the one property behind it, so a
    /// source stays live. The table form builds a live child through the
    /// project's factory.
    #[test]
    fn a_luau_value_in_style_reaches_its_property() {
        let src = "return <button style={{ scale = grow, borderColor = color, borderWidth = 2, zIndex = z, backgroundColor = bg, rotate = spin }}>Go</button>\n";
        let out = run(src, "a.alx", &Options::default());
        let text = apply(src, &out.edits);

        assert!(text.contains("<UIScale Scale={grow} />"), "{text}");
        assert!(
            text.contains("<UIStroke Color={color} Thickness={2} ApplyStrokeMode={Enum.ApplyStrokeMode.Border} />"),
            "{text}"
        );
        assert!(
            text.contains("ZIndex={z}")
                && text.contains("BackgroundColor3={bg}")
                && text.contains("Rotation={spin}"),
            "{text}"
        );
        assert!(out.findings.is_empty(), "{:?}", out.findings);

        for create in ["vide.create", "New"] {
            let opts = Options {
                factory: Factory {
                    table: true,
                    create: Some(create.into()),
                    compute: None,
                },
                ..Options::default()
            };
            let text = apply(src, &run(src, "a.alx", &opts).edits);

            assert!(
                text.contains(&format!(
                    "<__silk_child Class=\"UIScale\" Scale={{grow}} Make={{{create}}} />"
                )),
                "{text}"
            );
            // A child with no live value keeps `Instance.new`.
            assert!(
                text.contains("<__silk_child Class=\"UIPadding\" PaddingTop={UDim.new(0, 1)} PaddingRight={UDim.new(0, 6)} PaddingBottom={UDim.new(0, 1)} PaddingLeft={UDim.new(0, 6)} />"),
                "{text}"
            );
        }

        // A value on more lines stays where it stands, so the file keeps
        // its lines. A modifier cannot take it there.
        let out = silk(
            "return <button style={{ color = function()\n  return c()\nend, scale = function()\n  return 1\nend }}>Go</button>\n",
        );
        assert!(
            out.contains("TextColor3={function()\n  return c()\nend}"),
            "{out}"
        );
        assert!(!out.contains("UIScale"), "{out}");
        assert!(
            lints("return <p style={{ scale = function()\n  return 1\nend }}>a</p>\n")
                .contains(&"dynamic_style".to_string())
        );

        // A property with no single Roblox property behind it stays a
        // compile-time value.
        assert!(
            lints("return <div style={{ width = w }}></div>\n")
                .contains(&"dynamic_style".to_string())
        );
    }

    /// An `AutomaticSize` the author writes keeps Silk's automatic axis
    /// out of the classes, so Enamel writes no second one.
    #[test]
    fn a_written_automatic_size_wins() {
        let out = with_enamel(
            "return <button className=\"h-10\" AutomaticSize={Enum.AutomaticSize.X}>Go</button>\n",
        );

        assert!(out.contains("ClassName=\"h-10\""), "{out}");
    }

    /// `border-image` puts a gradient along the border: a UIGradient
    /// inside the UIStroke, whose white color lets the gradient show.
    #[test]
    fn a_border_image_is_a_gradient_in_the_stroke() {
        let src = "return <div style={{ border = \"6px solid #FFD65C\", borderImage = \"linear-gradient(to right, #FFD65C, transparent) 1\" }} />\n";
        let out = silk(src);

        assert!(
            out.contains("<UIStroke Thickness={6} Color={Color3.new(1, 1, 1)} Transparency={0} ApplyStrokeMode={Enum.ApplyStrokeMode.Border}><UIGradient Color={ColorSequence.new({ ColorSequenceKeypoint.new(0, Color3.fromRGB(255, 214, 92)), ColorSequenceKeypoint.new(1, Color3.fromRGB(255, 214, 92)) })} Rotation={0} Transparency={NumberSequence.new({ NumberSequenceKeypoint.new(0, 0), NumberSequenceKeypoint.new(1, 1) })} /></UIStroke>"),
            "{out}"
        );

        let out = vide(src);
        assert!(
            out.contains("<__silk_child Class=\"UIStroke\" Thickness={6}"),
            "{out}"
        );
        assert!(
            out.contains("><__silk_child Class=\"UIGradient\" Color={ColorSequence.new("),
            "{out}"
        );
        assert!(out.contains(" /></__silk_child>"), "{out}");
    }

    /// A live stroke transparency and a live gradient reach their
    /// modifier, so a source stays live. The gradient's direction is
    /// `gradientRotation` or an Enamel direction class, which then leaves
    /// the list so Enamel writes no second UIGradient.
    #[test]
    fn a_live_stroke_transparency_and_gradient_reach_their_modifier() {
        let out = silk(
            "return <button style={{ borderColor = color, borderTransparency = fade, borderWidth = 2 }}>Go</button>\n",
        );
        assert!(
            out.contains("<UIStroke Color={color} Thickness={2} ApplyStrokeMode={Enum.ApplyStrokeMode.Border} Transparency={fade} />"),
            "{out}"
        );

        // A literal is 0 to 1, as the Roblox property takes it.
        let out = silk(
            "return <div style={{ border = \"1px solid #fff\", borderTransparency = 0.8 }} />\n",
        );
        assert!(out.contains("Transparency={0.8}"), "{out}");

        let out = silk(
            "return <div style={{ backgroundGradient = sky, gradientRotation = 90, gradientTransparency = fade }} />\n",
        );
        assert!(
            out.contains("<UIGradient Rotation={90} Color={sky} Transparency={fade} />"),
            "{out}"
        );
        assert!(
            out.contains("BackgroundTransparency={0} BackgroundColor3={Color3.new(1, 1, 1)}"),
            "{out}"
        );

        // An Enamel direction class gives the rotation, and leaves the list.
        let src = "return <div className=\"w-full h-full bg-white bg-gradient-to-b\" style={{ backgroundGradient = sky }} />\n";
        let out = with_enamel(src);
        assert!(
            out.contains("<UIGradient Color={sky} Rotation={90} />"),
            "{out}"
        );
        assert!(
            out.contains("ClassName=\"w-full h-full bg-white \""),
            "{out}"
        );
        assert!(
            !out.contains("BackgroundColor3={Color3.new(1, 1, 1)}"),
            "{out}"
        );

        // The table form builds the live gradient through the factory.
        let out = vide(
            "return <button className=\"bg-white\" style={{ backgroundGradient = track }}>Go</button>\n",
        );
        assert!(
            out.contains("<__silk_child Class=\"UIGradient\" Color={track} Make={vide.create} />"),
            "{out}"
        );

        // A literal gradient is CSS: the Luau name reports.
        assert!(
            lints("return <div style={{ backgroundGradient = \"red\" }} />\n")
                .contains(&"bad_value".to_string())
        );
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
