//! The rewrite. It turns the runes, the `{#if}` and `{#each}` blocks, and
//! a `<script>` file into calls of the svelte-aly package, as edits that
//! keep every line where it was.
//!
//! - `let n = $state(0)` makes `n` a state. Each read and write of `n`
//!   becomes `n.value`, and the declaration becomes `local` or `const`.
//! - `$derived(expr)` becomes `derived(function() return expr end)`.
//! - A name that `$props()` binds reads through `get`, so a parent can
//!   pass a live value.
//! - A markup hole that reads a state becomes a function, which the
//!   factory binds. On a component tag it becomes a derived value.
//! - A component file, a `.alx` file that starts with `<script>`, gets
//!   the package and its factory on the `<script>` line, and each
//!   `function name()` of the script becomes local. The compiler then
//!   makes the script the body of the component.

use std::collections::{HashMap, HashSet};

use alloy_ingot::{Edit, Finding};

use crate::scan::{self, Kind, Place, Scan, Tag, Tok};

/// The settings of the rewrite, from the ingot's options and the
/// project's `[alx.factory]`.
#[derive(Debug, Clone)]
pub struct Options {
    /// The module path of the svelte-aly package.
    pub import: String,
    /// The name the rewrite gives the package in each file.
    pub helper: String,
    /// The name of the markup factory, when it is a plain name. A
    /// component file imports the package's `create` under it.
    pub create: Option<String>,
    /// Whether the project loads Silk, which turns `<style>` into a
    /// StyleSheet.
    pub silk: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            import: "@packages/svelte-aly".into(),
            helper: "__svelte".into(),
            create: Some("create".into()),
            silk: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct Output {
    pub edits: Vec<Edit>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Rune {
    State,
    Derived,
    Prop,
}

// The ranks order the edits at one offset: an opening before what it
// holds, a closing after it.
const HEADER: u8 = 0;
const WRAP_OPEN: u8 = 1;
const REF_OPEN: u8 = 2;
const REF_CLOSE: u8 = 3;
const DERIVED_CLOSE: u8 = 4;
const WRAP_CLOSE: u8 = 5;
const LAST: u8 = 9;

/// The Array methods that change an array in place.
const MUTATORS: &[&str] = &[
    "push",
    "pop",
    "insert",
    "remove",
    "clear",
    "sort",
    "reverse",
    "shift",
    "unshift",
    "splice",
    "fill",
    "swap_remove",
    "retain",
    "truncate",
    "dedup",
    "extend",
];

/// The `table` functions that change a table in place.
const TABLE_MUTATORS: &[&str] = &["insert", "remove", "sort", "clear", "move"];

/// The compound assignments, after which a name is written.
const ASSIGNS: &[&str] = &["=", "+=", "-=", "*=", "/=", "//=", "%=", "^=", "..=", "??="];

/// The tokens after which a name is a field, not a variable.
const FIELD: &[&str] = &[".", ":", "?.", "?:", "->", "::"];

struct Decl {
    keyword: Tok,
    names: Vec<String>,
    rune: Rune,
    /// The span of the type annotation, for a state or a derived value.
    ty: Option<(usize, usize)>,
}

/// What follows `as` in an `{#each}`.
struct EachHead {
    /// Where `as` starts. The expression ends here.
    cut: usize,
    item: String,
    index: Option<String>,
    key: Option<(usize, usize)>,
}

struct Pending {
    start: usize,
    end: usize,
    rank: u8,
    text: String,
}

struct Work<'a> {
    src: &'a str,
    scan: Scan,
    opts: &'a Options,
    edits: Vec<Pending>,
    findings: Vec<Finding>,
    decls: Vec<Decl>,
    names: HashMap<String, Rune>,
    /// The token starts of the names that each rune declaration binds.
    declared: HashSet<usize>,
    /// Each index name of an `{#each}`, with the span where it is a state.
    scoped: Vec<(String, usize, usize)>,
    /// The region of each `{#each}` head, with the offset where `as` starts.
    cuts: HashMap<usize, usize>,
    written: HashSet<String>,
    /// The regions that read a reactive name.
    reads: HashSet<usize>,
    /// Whether the file needs the package.
    used: bool,
    /// Whether the file is a component file in the form the compiler
    /// hands a transform: its imports, then `return function(props)`.
    component: bool,
}

/// Whether a `$` name is one this ingot compiles.
fn is_rune(text: &str) -> bool {
    matches!(
        text,
        "$state" | "$derived" | "$derived.by" | "$effect" | "$effect.pre" | "$props"
    )
}

/// Rewrites one file. `kind` is `aly` or `alx`.
pub fn run(src: &str, kind: &str, opts: &Options) -> Result<Output, String> {
    let scan = scan::scan(src, kind == "alx");
    let has_rune = scan
        .regions
        .iter()
        .flat_map(|r| &r.toks)
        .any(|t| t.kind == Kind::Rune && is_rune(&src[t.start..t.end]));

    if !has_rune && scan.blocks.is_empty() && scan.script.is_none() {
        return Ok(Output::default());
    }

    let mut w = Work {
        src,
        scan,
        opts,
        edits: Vec::new(),
        findings: Vec::new(),
        decls: Vec::new(),
        names: HashMap::new(),
        declared: HashSet::new(),
        scoped: Vec::new(),
        cuts: HashMap::new(),
        written: HashSet::new(),
        reads: HashSet::new(),
        used: false,
        component: false,
    };
    w.component = w.is_component();

    w.declarations();
    let heads = w.each_heads();
    w.runes();
    w.refs();
    w.blocks(&heads);
    w.holes();
    w.declaration_edits();
    w.wrap();

    Ok(Output {
        edits: finish(w.edits)?,
        findings: w.findings,
    })
}

/// Sorts the edits and joins the ones that touch, so that the host gets
/// no two edits at one offset. Two edits that overlap are a bug here.
fn finish(mut edits: Vec<Pending>) -> Result<Vec<Edit>, String> {
    edits.sort_by_key(|e| (e.start, e.end > e.start, e.rank));

    let mut out: Vec<Pending> = Vec::new();

    for e in edits {
        if e.text.contains('\n') {
            return Err(format!(
                "svelte-aly: an edit at byte {} adds a line",
                e.start
            ));
        }

        match out.last_mut() {
            Some(last) if e.start < last.end => {
                return Err(format!(
                    "svelte-aly: two edits overlap at byte {}; please report the file",
                    e.start
                ));
            }

            Some(last) if e.start == last.end => {
                last.text.push_str(&e.text);
                last.end = e.end;
            }

            _ => out.push(e),
        }
    }

    Ok(out
        .into_iter()
        .map(|e| Edit::replace(e.start as u32, e.end as u32, e.text))
        .collect())
}

impl<'a> Work<'a> {
    fn text(&self, t: Tok) -> &'a str {
        &self.src[t.start..t.end]
    }

    fn is(&self, t: Option<&Tok>, text: &str) -> bool {
        t.is_some_and(|t| self.text(*t) == text && t.kind != Kind::Str)
    }

    fn edit(&mut self, start: usize, end: usize, rank: u8, text: impl Into<String>) {
        self.edits.push(Pending {
            start,
            end,
            rank,
            text: text.into(),
        });
    }

    fn find(&mut self, lint: &str, span: (usize, usize), message: impl Into<String>) {
        self.findings
            .push(Finding::new(lint, (span.0 as u32, span.1 as u32), message));
    }

    /// Whether the file is a component file: a `<script>` as the author
    /// wrote it, which a lint reads, or the module the compiler makes of
    /// one, which a transform reads.
    fn sfc(&self) -> bool {
        self.scan.script.is_some() || self.component
    }

    /// Whether the file starts with imports and then `return
    /// function(props)`: the module the compiler makes of a `<script>`.
    fn is_component(&self) -> bool {
        let Some(toks) = self.scan.regions.first().map(|r| &r.toks) else {
            return false;
        };
        let mut k = 0;

        while self.is(toks.get(k), "import") {
            k += 1;

            while toks.get(k).is_some_and(|t| t.kind != Kind::Str) {
                k += 1;
            }

            k += 1;
        }

        ["return", "function", "(", "props", ")"]
            .iter()
            .enumerate()
            .all(|(i, word)| self.is(toks.get(k + i), word))
    }

    /// Finds each `let name = $state(...)`, `$derived`, and `$props()`
    /// declaration.
    fn declarations(&mut self) {
        let mut decls = Vec::new();

        for region in &self.scan.regions {
            let toks = &region.toks;

            for k in 0..toks.len() {
                if !matches!(self.text(toks[k]), "let" | "local" | "const")
                    || toks[k].kind != Kind::Name
                {
                    continue;
                }

                let (names, after) = match toks.get(k + 1) {
                    Some(t) if t.kind == Kind::Name => (vec![*t], k + 2),

                    Some(t) if self.text(*t) == "{" => {
                        let mut names = Vec::new();
                        let mut j = k + 2;

                        while j < toks.len() && self.text(toks[j]) != "}" {
                            if toks[j].kind == Kind::Name
                                && matches!(toks.get(j + 1).map(|t| self.text(*t)), Some("," | "}"))
                            {
                                names.push(toks[j]);
                            }

                            j += 1;
                        }

                        (names, j + 1)
                    }

                    _ => continue,
                };

                // An annotation runs to the `=` at depth zero.
                let mut eq = after;
                let mut ty = None;

                if self.is(toks.get(after), ":") {
                    let mut depth = 0i32;
                    eq = after + 1;

                    while let Some(t) = toks.get(eq) {
                        match self.text(*t) {
                            "(" | "{" | "[" => depth += 1,
                            ")" | "}" | "]" => depth -= 1,
                            "=" if depth == 0 => break,
                            _ => {}
                        }

                        eq += 1;
                    }

                    if eq > after + 1 && eq < toks.len() {
                        ty = Some((toks[after + 1].start, toks[eq - 1].end));
                    }
                }

                if !self.is(toks.get(eq), "=") {
                    continue;
                }

                let Some(value) = toks.get(eq + 1).filter(|t| t.kind == Kind::Rune) else {
                    continue;
                };

                let rune = match self.text(*value) {
                    "$state" => Rune::State,
                    "$derived" | "$derived.by" => Rune::Derived,
                    "$props" => Rune::Prop,
                    _ => continue,
                };

                // A destructure binds props only; `$state` needs one name.
                if (rune == Rune::Prop) != (self.text(toks[k + 1]) == "{") {
                    continue;
                }

                decls.push(Decl {
                    keyword: toks[k],
                    names: names.iter().map(|t| self.text(*t).to_string()).collect(),
                    rune,
                    ty: ty.filter(|_| rune != Rune::Prop),
                });
                self.declared.extend(names.iter().map(|t| t.start));
            }
        }

        for d in &decls {
            for n in &d.names {
                self.names.insert(n.clone(), d.rune);
            }
        }

        self.decls = decls;
    }

    /// Reads the head of each `{#each}`, and the span where its index
    /// name is a state.
    fn each_heads(&mut self) -> HashMap<usize, EachHead> {
        let mut heads = HashMap::new();
        let mut open: Vec<usize> = Vec::new();
        let blocks = self.scan.blocks.clone();

        for (b, block) in blocks.iter().enumerate() {
            match block.tag {
                Tag::Each => {
                    open.push(b);

                    if let Some(head) = block.head.and_then(|r| self.each_head(r)) {
                        heads.insert(b, head);
                    } else {
                        self.find(
                            "bad_each",
                            (block.start, block.end),
                            "write `{#each items as item}`, `{#each items as item, i}`, or add a key: `{#each items as item (item.id)}`",
                        );
                    }
                }

                Tag::EndEach => {
                    let Some(each) = open.pop() else {
                        continue;
                    };

                    if let Some(head) = heads.get(&each) {
                        if let Some(index) = &head.index {
                            self.scoped
                                .push((index.clone(), blocks[each].end, block.start));
                        }

                        if self.names.contains_key(&head.item) {
                            self.find(
                                "shadowed_rune",
                                (blocks[each].start, blocks[each].end),
                                format!(
                                    "`{}` names a rune already; give the item another name",
                                    head.item
                                ),
                            );
                        }
                    }
                }

                _ => {}
            }
        }

        heads
    }

    fn each_head(&mut self, region: usize) -> Option<EachHead> {
        let toks = self.scan.regions[region].toks.clone();
        let mut depth = 0i32;
        let mut cut = None;

        for (k, t) in toks.iter().enumerate() {
            match self.text(*t) {
                "(" | "{" | "[" => depth += 1,
                ")" | "}" | "]" => depth -= 1,
                "as" if depth == 0 && t.kind == Kind::Name => cut = Some(k),
                _ => {}
            }
        }

        let cut = cut.filter(|k| *k > 0)?;
        let item = toks.get(cut + 1).filter(|t| t.kind == Kind::Name)?;
        let mut next = cut + 2;
        let mut index = None;

        if self.is(toks.get(next), ",") {
            index = Some(toks.get(next + 1).filter(|t| t.kind == Kind::Name)?);
            next += 2;
        }

        let mut key = None;

        if self.is(toks.get(next), "(") {
            let close = toks.len().checked_sub(1)?;

            if !self.is(toks.get(close), ")") || close <= next + 1 {
                return None;
            }

            key = Some((toks[next + 1].start, toks[close - 1].end));
        } else if next != toks.len() {
            return None;
        }

        self.cuts.insert(region, toks[cut].start);

        Some(EachHead {
            cut: toks[cut].start,
            item: self.text(*item).to_string(),
            index: index.map(|t| self.text(*t).to_string()),
            key,
        })
    }

    /// Rewrites each rune call.
    fn runes(&mut self) {
        let h = self.opts.helper.clone();
        let sfc = self.sfc();

        for r in 0..self.scan.regions.len() {
            let toks = self.scan.regions[r].toks.clone();

            for (k, t) in toks.iter().enumerate() {
                if t.kind != Kind::Rune {
                    continue;
                }

                match self.text(*t) {
                    "$state" => self.edit(t.start, t.end, REF_OPEN, format!("{h}.state")),
                    "$derived.by" => self.edit(t.start, t.end, REF_OPEN, format!("{h}.derived")),
                    "$effect" | "$effect.pre" => {
                        self.edit(t.start, t.end, REF_OPEN, format!("{h}.effect"))
                    }

                    "$derived" => {
                        let Some(close) = self
                            .is(toks.get(k + 1), "(")
                            .then(|| matching(self, &toks, k + 1))
                            .flatten()
                        else {
                            continue;
                        };

                        self.edit(
                            t.start,
                            toks[k + 1].end,
                            REF_OPEN,
                            format!("{h}.derived(function() return "),
                        );
                        self.edit(toks[close].start, toks[close].start, DERIVED_CLOSE, " end");
                    }

                    "$props" => {
                        if !sfc {
                            self.find(
                                "props_outside_script",
                                (t.start, t.end),
                                "`$props()` reads the props of a component; use it in the `<script>` of a component file",
                            );
                            continue;
                        }

                        if self.is(toks.get(k + 1), "(") && self.is(toks.get(k + 2), ")") {
                            self.edit(t.start, toks[k + 2].end, REF_OPEN, "(props :: any)");
                        }
                    }

                    _ => continue,
                }

                self.used = true;
            }
        }
    }

    /// The token starts of the names that a `local`, a `for`, or a
    /// parameter list binds.
    fn bindings(&self, toks: &[Tok]) -> HashSet<usize> {
        let mut out = HashSet::new();

        for k in 0..toks.len() {
            if toks[k].kind != Kind::Name {
                continue;
            }

            match self.text(toks[k]) {
                "local" | "const" | "let" | "for" => {
                    let mut j = k + 1;

                    if self.is(toks.get(j), "function") {
                        j += 1;
                    }

                    // A destructure: `local { a, b } = t`.
                    if self.is(toks.get(j), "{") {
                        while let Some(t) = toks.get(j + 1).filter(|t| self.text(**t) != "}") {
                            if t.kind == Kind::Name
                                && matches!(toks.get(j + 2).map(|n| self.text(*n)), Some("," | "}"))
                            {
                                out.insert(t.start);
                            }

                            j += 1;
                        }

                        continue;
                    }

                    while let Some(t) = toks.get(j).filter(|t| t.kind == Kind::Name) {
                        out.insert(t.start);

                        if !self.is(toks.get(j + 1), ",") {
                            break;
                        }

                        j += 2;
                    }
                }

                "function" => {
                    // Past the name and its generics, to the parameters.
                    let Some(open) =
                        (k + 1..toks.len().min(k + 12)).find(|j| self.is(toks.get(*j), "("))
                    else {
                        continue;
                    };
                    let mut j = open + 1;
                    let mut depth = 0i32;

                    while let Some(t) = toks.get(j) {
                        match self.text(*t) {
                            "(" | "{" | "[" => depth += 1,
                            ")" if depth == 0 => break,
                            ")" | "}" | "]" => depth -= 1,
                            _ if depth == 0
                                && t.kind == Kind::Name
                                && (self.is(toks.get(j - 1), "(")
                                    || self.is(toks.get(j - 1), ",")) =>
                            {
                                out.insert(t.start);
                            }
                            _ => {}
                        }

                        j += 1;
                    }
                }

                _ => {}
            }
        }

        out
    }

    /// The rune of a name at `at`: a declared rune, or the index of an
    /// `{#each}` that holds `at`.
    fn rune_of(&self, name: &str, at: usize) -> Option<Rune> {
        self.names.get(name).copied().or_else(|| {
            self.scoped
                .iter()
                .any(|(n, s, e)| n == name && *s <= at && at < *e)
                .then_some(Rune::State)
        })
    }

    /// Rewrites each read and write of a reactive name.
    fn refs(&mut self) {
        let h = self.opts.helper.clone();

        for r in 0..self.scan.regions.len() {
            let toks = self.scan.regions[r].toks.clone();
            let cut = self.cuts.get(&r).copied().unwrap_or(usize::MAX);
            let binds = self.bindings(&toks);
            let mut stack: Vec<&str> = Vec::new();

            for k in 0..toks.len() {
                let t = toks[k];
                let text = self.text(t);

                match (t.kind, text) {
                    (Kind::Punct, "{" | "(" | "[") => stack.push(text),
                    (Kind::Interp, "{") => stack.push("`"),
                    (Kind::Punct, "}" | ")" | "]") | (Kind::Interp, "}") => {
                        stack.pop();
                    }
                    _ => {}
                }

                if t.kind != Kind::Name || t.start >= cut {
                    continue;
                }

                let Some(rune) = self.rune_of(text, t.start) else {
                    continue;
                };

                let prev = k.checked_sub(1).map(|j| toks[j]);
                let next = toks.get(k + 1).copied();

                if prev.is_some_and(|p| p.kind == Kind::Punct && FIELD.contains(&self.text(p))) {
                    continue;
                }

                if self.declared.contains(&t.start) {
                    continue;
                }

                if binds.contains(&t.start) {
                    self.find(
                        "shadowed_rune",
                        (t.start, t.end),
                        format!("`{text}` names a rune already, and the rewrite reads it as the rune; give this one another name"),
                    );

                    continue;
                }

                // A key in a table constructor: `{ count = 1 }`.
                if self.is(next.as_ref(), "=")
                    && stack.last() == Some(&"{")
                    && prev.is_some_and(|p| matches!(self.text(p), "{" | "," | ";"))
                {
                    continue;
                }

                let assigned =
                    next.is_some_and(|n| n.kind == Kind::Punct && ASSIGNS.contains(&self.text(n)));
                self.reads.insert(r);

                match rune {
                    Rune::Prop if assigned => {
                        self.written.insert(text.to_string());
                    }

                    Rune::Prop => {
                        self.edit(t.start, t.start, REF_OPEN, format!("{h}.get("));
                        self.edit(t.end, t.end, REF_CLOSE, ")");
                    }

                    Rune::Derived if assigned => {
                        self.find(
                            "derived_write",
                            (t.start, t.end),
                            format!("`{text}` is a `$derived` value, which code cannot write; write the state it reads"),
                        );
                    }

                    Rune::State | Rune::Derived => {
                        if assigned {
                            self.written.insert(text.to_string());
                        }

                        self.mutation(&toks, k);
                        self.edit(t.end, t.end, REF_CLOSE, ".value");
                    }
                }
            }
        }
    }

    /// Warns when code changes the table of a state in place, which runs
    /// no effect.
    fn mutation(&mut self, toks: &[Tok], k: usize) {
        let t = toks[k];
        let method = self.is(toks.get(k + 1), ":")
            && toks
                .get(k + 2)
                .is_some_and(|m| MUTATORS.contains(&self.text(*m)));
        let table = k >= 4
            && self.is(toks.get(k - 1), "(")
            && TABLE_MUTATORS.contains(&self.text(toks[k - 2]))
            && self.is(toks.get(k - 3), ".")
            && self.is(toks.get(k - 4), "table");

        if method || table {
            let name = self.text(t).to_string();
            self.find(
                "mutation",
                (t.start, t.end),
                format!("this changes the table of `{name}` in place, which runs no effect; assign it again after: `{name} = {name}`"),
            );
        }
    }

    /// Rewrites the block tags.
    fn blocks(&mut self, heads: &HashMap<usize, EachHead>) {
        let h = self.opts.helper.clone();
        let blocks = self.scan.blocks.clone();
        // Each open block: whether it is an each, and whether it is rewritten.
        let mut open: Vec<(bool, bool)> = Vec::new();
        let stray = |w: &mut Self, b: &scan::Block| {
            w.find(
                "unclosed_block",
                (b.start, b.end),
                "this block tag closes or continues no open block",
            );
        };

        for (b, block) in blocks.iter().enumerate() {
            let head_span = block
                .head
                .map(|r| (self.scan.regions[r].start, self.scan.regions[r].end));

            match &block.tag {
                Tag::If => {
                    let (s, e) = head_span.unwrap_or((block.end, block.end));
                    self.edit(
                        block.start,
                        s,
                        HEADER,
                        format!("{{{h}.if_block(function() return "),
                    );
                    self.edit(e, block.end, HEADER, " end, function() return <>");
                    open.push((false, true));
                }

                Tag::ElseIf => {
                    if open.last() != Some(&(false, true)) {
                        stray(self, block);
                        continue;
                    }

                    let (s, e) = head_span.unwrap_or((block.end, block.end));
                    self.edit(block.start, s, HEADER, "</> end, function() return ");
                    self.edit(e, block.end, HEADER, " end, function() return <>");
                }

                Tag::Else => match open.last() {
                    Some((false, true)) => {
                        self.edit(
                            block.start,
                            block.end,
                            HEADER,
                            "</> end, true, function() return <>",
                        );
                    }

                    Some((true, true)) => {
                        self.edit(
                            block.start,
                            block.end,
                            HEADER,
                            "</> end, function() return <>",
                        );
                    }

                    Some(_) => {}

                    None => stray(self, block),
                },

                Tag::EndIf | Tag::EndEach => {
                    let each = block.tag == Tag::EndEach;

                    match open.pop() {
                        Some((kind, live)) if kind == each => {
                            if live {
                                self.edit(block.start, block.end, HEADER, "</> end)}");
                            }
                        }

                        _ => stray(self, block),
                    }
                }

                Tag::Each => {
                    let Some(head) = heads.get(&b) else {
                        open.push((true, false));
                        continue;
                    };

                    let (s, _) = head_span.unwrap_or((block.end, block.end));
                    // Luau types the parameters of a lambda before it
                    // solves the generic, so the item takes its type from
                    // the list by name: `index<typeof(list), number>`.
                    let list = self.rewritten(s, head.cut);
                    let item = format!("{}: index<typeof({list}), number>", head.item);
                    let key = match head.key {
                        Some((ks, ke)) => {
                            format!("function({item}) return {} end", &self.src[ks..ke])
                        }

                        None => "nil".into(),
                    };
                    let params = match &head.index {
                        Some(i) => format!("{item}, {i}: {h}.Derived<number>"),

                        None => item.clone(),
                    };

                    self.edit(
                        block.start,
                        s,
                        HEADER,
                        format!("{{{h}.each_block(function() return "),
                    );
                    self.edit(
                        head.cut,
                        block.end,
                        HEADER,
                        format!(" end, {key}, function({params}) return <>"),
                    );
                    open.push((true, true));
                }

                Tag::Other(word) => {
                    let word = word.clone();
                    self.find(
                        "unsupported_block",
                        (block.start, block.end),
                        format!("svelte-aly has no `{{{word}}}` block; it compiles `{{#if}}` and `{{#each}}`"),
                    );
                }
            }

            self.used = true;
        }

        if !open.is_empty() {
            let at = self.src.trim_end().len();
            self.find(
                "unclosed_block",
                (at, at),
                "a `{#if}` or `{#each}` block has no `{/if}` or `{/each}`",
            );
        }
    }

    /// The source text of a span with the edits made so far inside it,
    /// on one line, for a copy in generated text.
    fn rewritten(&self, start: usize, end: usize) -> String {
        let mut inside: Vec<&Pending> = self
            .edits
            .iter()
            .filter(|e| e.start >= start && e.end <= end)
            .collect();
        inside.sort_by_key(|e| (e.start, e.end > e.start, e.rank));

        let mut out = String::new();
        let mut at = start;

        for e in inside {
            out.push_str(&self.src[at..e.start.max(at)]);
            out.push_str(&e.text);
            at = at.max(e.end);
        }

        out.push_str(&self.src[at..end]);
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// Makes each hole that reads a reactive name a function, so the
    /// factory binds it.
    fn holes(&mut self) {
        let bound = self.bound_names();

        for r in 0..self.scan.regions.len() {
            let region = &self.scan.regions[r];
            let first = region.toks.first().map(|t| self.text(*t));
            let (s, e) = (region.start, region.end);

            let component = match region.place {
                Place::Attr(tag) => {
                    let name = self.scan.elements[tag].name.as_str();
                    name.contains('.') || bound.contains(name)
                }

                Place::Child => false,

                _ => continue,
            };

            if !self.reads.contains(&r) || first.is_none_or(|f| f == "function") {
                continue;
            }

            let h = self.opts.helper.clone();

            if component {
                self.edit(s, s, WRAP_OPEN, format!("{h}.derived(function() return "));
                self.edit(e, e, WRAP_CLOSE, " end)");
            } else {
                self.edit(s, s, WRAP_OPEN, "function() return ");
                self.edit(e, e, WRAP_CLOSE, " end");
            }
        }
    }

    /// The names that the code binds: imports and declarations. A tag
    /// with one of these names is a component, as luaux reads it.
    fn bound_names(&self) -> HashSet<&'a str> {
        let mut out = HashSet::new();

        for region in self.scan.regions.iter().filter(|r| r.place == Place::Code) {
            let toks = &region.toks;
            let binds = self.bindings(toks);
            let mut importing = false;

            for (k, t) in toks.iter().enumerate() {
                match self.text(*t) {
                    "import" => importing = true,
                    "from" => importing = false,
                    name if t.kind == Kind::Name
                        && (binds.contains(&t.start)
                            || (importing && !matches!(name, "type" | "as"))
                            || (k > 0 && self.text(toks[k - 1]) == "function")) =>
                    {
                        out.insert(name);
                    }
                    _ => {}
                }
            }
        }

        out
    }

    /// Writes each rune declaration as the package wants it: a state that
    /// code writes is `local`, any other is `const`, and an annotation
    /// names the state type.
    fn declaration_edits(&mut self) {
        let h = self.opts.helper.clone();
        let decls = std::mem::take(&mut self.decls);

        for d in &decls {
            let written = d.names.iter().any(|n| self.written.contains(n));
            let keyword = match (d.rune, written) {
                (Rune::State | Rune::Prop, true) => "local",

                _ => "const",
            };

            if self.text(d.keyword) != keyword {
                self.edit(d.keyword.start, d.keyword.end, LAST, keyword);
            }

            if let Some((s, e)) = d.ty {
                let ty = if d.rune == Rune::State {
                    "State"
                } else {
                    "Derived"
                };
                self.edit(s, s, REF_OPEN, format!("{h}.{ty}<"));
                self.edit(e, e, REF_CLOSE, ">");
            }
        }
    }

    /// Imports the package at the top of the module. A component file
    /// gets the markup factory too, when the script does not bind one.
    fn wrap(&mut self) {
        let h = self.opts.helper.clone();
        let import = format!("import * as {h} from '{}'", self.opts.import);

        let first = self
            .scan
            .regions
            .first()
            .and_then(|r| r.toks.first())
            .map(|t| t.start);
        let at = match self.scan.script {
            Some((open, _)) => Some(open.1),

            None => first,
        };

        let Some(at) = at.filter(|_| self.used || self.sfc()) else {
            return;
        };

        if !self.sfc() {
            self.edit(at, at, HEADER, format!("{import} "));
            return;
        }

        let bound = self.bound_names();
        let create = match &self.opts.create {
            Some(c) if !bound.contains(c.as_str()) => match c.as_str() {
                "create" => format!(" import {{ create }} from '{}'", self.opts.import),

                _ => format!(" import {{ create as {c} }} from '{}'", self.opts.import),
            },

            _ => String::new(),
        };
        let pad = if self.scan.script.is_some() { " " } else { "" };
        let tail = if self.scan.script.is_some() { "" } else { " " };
        self.edit(at, at, HEADER, format!("{pad}{import}{create}{tail}"));
        self.local_functions();

        if !self.opts.silk {
            for (s, e) in self.scan.styles.clone() {
                self.find("style_needs_silk", (s, e), "`<style>` needs the Silk ingot, which turns it into a StyleSheet; the rewrite drops it");

                // Blank the element line by line, so the line count holds.
                let mut at = s;

                for line in self.src[s..e].split('\n') {
                    if !line.is_empty() {
                        self.edit(at, at + line.len(), HEADER, "");
                    }

                    at += line.len() + 1;
                }
            }
        }
    }

    /// Makes each `function name()` of a script local, as a function of a
    /// Svelte script is local to the component. In Luau the statement
    /// would set a global.
    fn local_functions(&mut self) {
        let toks = self.scan.regions[0].toks.clone();

        for k in 0..toks.len() {
            let declares = self.text(toks[k]) == "function"
                && toks[k].kind == Kind::Name
                && toks.get(k + 1).is_some_and(|t| t.kind == Kind::Name)
                && self.is(toks.get(k + 2), "(")
                && !k.checked_sub(1).is_some_and(|p| {
                    matches!(
                        self.text(toks[p]),
                        "local" | "export" | "public" | "private"
                    )
                });

            if declares {
                self.edit(toks[k].start, toks[k].start, HEADER, "local ");
            }
        }
    }
}

/// The index of the `)` that closes the `(` at `open`.
fn matching(w: &Work<'_>, toks: &[Tok], open: usize) -> Option<usize> {
    let mut depth = 0i32;

    for (k, t) in toks.iter().enumerate().skip(open) {
        match w.text(*t) {
            "(" if t.kind == Kind::Punct => depth += 1,
            ")" if t.kind == Kind::Punct => {
                depth -= 1;

                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The source with the edits of the rewrite applied.
    fn apply(src: &str, kind: &str) -> String {
        let out = run(src, kind, &Options::default()).expect("the rewrite runs");
        let mut text = src.to_string();

        for e in out.edits.iter().rev() {
            text.replace_range(e.0 as usize..e.1 as usize, &e.2);
        }

        assert_eq!(text.lines().count(), src.lines().count(), "{text}");
        text
    }

    fn lints(src: &str, kind: &str) -> Vec<String> {
        let out = run(src, kind, &Options::default()).expect("the rewrite runs");
        out.findings.iter().map(|f| f.lint.clone()).collect()
    }

    #[test]
    fn a_state_reads_and_writes_through_value() {
        let out = apply(
            "let n = $state(0)\nlet d = $derived(n * 2)\nn += 1\nprint(n, d, t.n, { n = 1 })\n",
            "aly",
        );

        assert_eq!(
            out,
            "import * as __svelte from '@packages/svelte-aly' local n = __svelte.state(0)\nconst d = __svelte.derived(function() return n.value * 2 end)\nn.value += 1\nprint(n.value, d.value, t.n, { n = 1 })\n"
        );
    }

    #[test]
    fn an_annotation_names_the_state_type() {
        let out = apply("const items: { string } = $state({})\n", "aly");

        assert!(
            out.contains("const items: __svelte.State<{ string }> = __svelte.state({})"),
            "{out}"
        );
    }

    #[test]
    fn a_component_file_gets_the_package_and_its_blocks() {
        let src = "<script>\n  import Row from './Row'\n  let { title } = $props()\n  let open = $state(false)\n  function toggle() open = not open end\n</script>\n{#if open}\n  <TextLabel Text={title} />\n{:else}\n  <Row label={title} />\n{/if}\n{#each items as item, i (item.id)}\n  <TextLabel>{i}. {item.name}</TextLabel>\n{/each}\n";
        let out = apply(src, "alx");

        assert!(out.starts_with("<script> import * as __svelte from '@packages/svelte-aly' import { create } from '@packages/svelte-aly'\n"), "{out}");
        assert!(out.contains("const { title } = (props :: any)"), "{out}");
        assert!(
            out.contains("local function toggle() open.value = not open.value end"),
            "{out}"
        );
        assert!(
            out.contains(
                "{__svelte.if_block(function() return  open.value end, function() return <>"
            ),
            "{out}"
        );
        assert!(
            out.contains("<TextLabel Text={function() return __svelte.get(title) end} />"),
            "{out}"
        );
        assert!(out.contains("</> end, true, function() return <>"), "{out}");
        assert!(
            out.contains(
                "<Row label={__svelte.derived(function() return __svelte.get(title) end)} />"
            ),
            "{out}"
        );
        assert!(out.contains("{__svelte.each_block(function() return  items  end, function(item: index<typeof(items), number>) return item.id end, function(item: index<typeof(items), number>, i: __svelte.Derived<number>) return <>"), "{out}");
        assert!(
            out.contains("<TextLabel>{function() return i.value end}. {item.name}</TextLabel>"),
            "{out}"
        );
        assert!(out.contains("</> end)}"), "{out}");
    }

    #[test]
    fn a_component_in_the_compiler_form_gets_the_package() {
        let src = "\n  import Row from './Row'\n  return function(props) let n = $state(0)\n  function bump() n += 1 end\nreturn (<>\n<Row n={n} /></>) end\n";
        let out = apply(src, "alx");

        assert!(out.contains("import * as __svelte from '@packages/svelte-aly' import { create } from '@packages/svelte-aly' import Row from './Row'"), "{out}");
        assert!(
            out.contains("return function(props) local n = __svelte.state(0)"),
            "{out}"
        );
        assert!(
            out.contains("local function bump() n.value += 1 end"),
            "{out}"
        );
        assert!(
            out.contains("<Row n={__svelte.derived(function() return n.value end)} />"),
            "{out}"
        );
    }

    #[test]
    fn a_file_with_no_rune_is_left_alone() {
        assert!(
            run("local n = 1\nprint(`{n}`)\n", "aly", &Options::default())
                .expect("runs")
                .edits
                .is_empty()
        );
    }

    #[test]
    fn the_lints_name_what_the_rewrite_cannot_do() {
        assert_eq!(
            lints("let n = $state(0)\nlocal function f(n) end\n", "aly"),
            ["shadowed_rune"]
        );
        assert_eq!(
            lints("let d = $derived(1)\nd = 2\n", "aly"),
            ["derived_write"]
        );
        assert_eq!(
            lints(
                "let xs = $state([])\nxs:push(1)\ntable.insert(xs, 2)\n",
                "aly"
            ),
            ["mutation", "mutation"]
        );
        assert_eq!(lints("let p = $props()\n", "aly"), ["props_outside_script"]);
        assert_eq!(lints("<Frame>{#if a}</Frame>", "alx"), ["unclosed_block"]);
        assert_eq!(
            lints("<Frame>{#await p}{/await}</Frame>", "alx"),
            ["unsupported_block", "unsupported_block"]
        );
        assert_eq!(
            lints("<Frame>{#each items}{/each}</Frame>", "alx"),
            ["bad_each"]
        );
        assert_eq!(
            lints("<script>\n</script>\n<style>\nFrame { }\n</style>\n", "alx"),
            ["style_needs_silk"]
        );
    }
}
