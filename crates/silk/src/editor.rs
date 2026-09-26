//! What the editor asks of Silk: completion, hover, and color swatches
//! for the HTML tags, their attributes, a `style` table, and the CSS of a
//! `<style>` element.

use std::collections::BTreeSet;

use alloy_ingot::{ColorInfo, CompletionItem, Completions, Hover, ItemKind};

use crate::css;
use crate::emit;
use crate::html::{self, Kind};
use crate::markup::{self, Markup, Value};
use crate::props;

/// Where the cursor stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spot {
    /// After `<`, naming a tag: the typed prefix and its span.
    Tag(String, (usize, usize)),
    /// In the open tag of `tag`, where an attribute name goes.
    Attr {
        tag: String,
        typed: String,
        span: (usize, usize),
        existing: Vec<String>,
    },
    /// In the string value of an attribute.
    AttrValue {
        tag: String,
        attr: String,
        typed: String,
        span: (usize, usize),
    },
    /// In a `style={{ }}` table, where a key goes.
    StyleKey { typed: String, span: (usize, usize) },
    /// In a `style={{ }}` table, in the string value of a key.
    StyleValue {
        prop: String,
        typed: String,
        span: (usize, usize),
    },
    /// In a `<style>`, where a selector goes.
    Selector { typed: String, span: (usize, usize) },
    /// In a `<style>` rule, where a property name goes.
    Property { typed: String, span: (usize, usize) },
    /// In a `<style>` rule, in the value of `prop`.
    PropValue {
        prop: String,
        typed: String,
        span: (usize, usize),
    },
}

/// A byte span as the protocol takes it.
fn u((s, e): (usize, usize)) -> (u32, u32) {
    (s as u32, e as u32)
}

fn is_css_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// The word that ends at `offset`, by `word`, and its span.
fn word_before(src: &str, offset: usize, word: fn(u8) -> bool) -> (String, (usize, usize)) {
    let b = src.as_bytes();
    let mut s = offset.min(b.len());

    while s > 0 && word(b[s - 1]) {
        s -= 1;
    }

    let mut e = offset.min(b.len());

    while e < b.len() && word(b[e]) {
        e += 1;
    }

    (src[s..offset.min(b.len())].to_string(), (s, e))
}

/// The `<style>` element whose CSS holds `offset`, as the span of its
/// CSS. A style element being typed has no close yet; the CSS then runs
/// to the end of the file or the next tag.
fn style_at(src: &str, offset: usize) -> Option<(usize, usize)> {
    let before = &src[..offset];
    let mut open = before.rfind("<style")?;

    // A `<style` in a comment or a string opens no CSS.
    while !markup::in_code(src, open) {
        open = before[..open].rfind("<style")?;
    }

    if before[open..].contains("</style") {
        return None;
    }

    let gt = open + src[open..].find('>')? + 1;

    if gt > offset {
        return None;
    }

    let end = src[gt..].find("</style").map_or(src.len(), |n| gt + n);

    (offset <= end).then_some((gt, end))
}

/// Whether a `<` at `lt` can open a tag: what comes before it starts an
/// expression, or ends a tag.
fn opens_tag(src: &str, lt: usize) -> bool {
    let before = src[..lt].trim_end();

    match before.as_bytes().last() {
        None => true,

        Some(b'>' | b'(' | b'{' | b'}' | b',' | b'=' | b'[' | b';' | b':') => true,

        Some(c) if c.is_ascii_alphabetic() => {
            let word_start = before
                .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .map_or(0, |n| n + 1);

            matches!(
                &before[word_start..],
                "return" | "then" | "else" | "do" | "and" | "or" | "not" | "in"
            ) || in_children(src, lt)
        }

        Some(_) => in_children(src, lt),
    }
}

/// Whether `at` sits among the children of an element: the last tag
/// before it is an open tag without a close.
fn in_children(src: &str, at: usize) -> bool {
    let m = Markup::read(src);

    m.elements
        .iter()
        .any(|e| e.open_end <= at && e.close.is_some_and(|(c, _)| at <= c))
}

/// The open tag `offset` sits in: its `<`, and its name.
fn open_tag(src: &str, offset: usize) -> Option<(usize, String)> {
    let b = src.as_bytes();
    let mut i = offset;
    let mut depth = 0i32;

    while i > 0 {
        i -= 1;

        match b[i] {
            b'}' => depth += 1,

            b'{' if depth > 0 => depth -= 1,

            b'{' => return None,

            // A `>` inside a string value has an odd count of quotes
            // between it and the cursor.
            b'>' if depth == 0 && src[i..offset].matches('"').count().is_multiple_of(2) => {
                return None;
            }

            b'<' if depth == 0 => {
                let name_end = i
                    + 1
                    + src[i + 1..]
                        .bytes()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == b'-' || *c == b'.')
                        .count();
                let name = &src[i + 1..name_end];

                if name.is_empty() || offset <= name_end {
                    return None;
                }

                // A known HTML tag reads as a tag even in a file that does
                // not parse, which is most files while someone types.
                return (opens_tag(src, i) || html::tag(name).is_some())
                    .then(|| (i, name.to_string()));
            }

            _ => {}
        }
    }

    None
}

pub fn spot(src: &str, offset: usize) -> Option<Spot> {
    let offset = offset.min(src.len());

    // Inside the CSS of a `<style>`.
    if let Some((css_start, _)) = style_at(src, offset) {
        return Some(css_spot(src, css_start, offset));
    }

    // A tag name.
    let (typed, span) = word_before(src, offset, |b| b.is_ascii_alphanumeric() || b == b'-');

    if span.0 > 0 && src.as_bytes()[span.0 - 1] == b'<' && opens_tag(src, span.0 - 1) {
        return Some(Spot::Tag(typed, span));
    }

    // A `style={{ }}` table.
    let before = &src[..offset];

    if let Some(at) = before.rfind("style={{")
        && !before[at..].contains("}}")
        && open_tag(src, at).is_some()
    {
        return Some(table_spot(src, offset, &before[at + 8..]));
    }

    let (lt, tag) = open_tag(src, offset)?;
    let inside = &src[lt..offset];

    // A string value: an odd count of quotes after the last `=`.
    let quotes = inside.matches('"').count();

    if quotes % 2 == 1 {
        let q = lt + inside.rfind('"')?;
        let before_q = src[..q].trim_end();
        let attr_end = before_q.strip_suffix('=')?.trim_end();
        let (attr, _) = word_before(attr_end, attr_end.len(), |b| {
            b.is_ascii_alphanumeric() || b == b'-'
        });
        let value_typed = &src[q + 1..offset];
        let token_start = value_typed
            .rfind(char::is_whitespace)
            .map_or(q + 1, |n| q + 1 + n + 1);
        let end = src[offset..]
            .find(|c: char| c == '"' || c.is_whitespace())
            .map_or(src.len(), |n| offset + n);

        return Some(Spot::AttrValue {
            tag,
            attr,
            typed: src[token_start..offset].to_string(),
            span: (token_start, end),
        });
    }

    // An attribute name: after whitespace inside the open tag.
    let (typed, span) = word_before(src, offset, |b| b.is_ascii_alphanumeric() || b == b'-');
    let before = src[..span.0].chars().last();

    if before.is_some_and(char::is_whitespace) {
        let open_end = src[lt..].find('>').map_or(src.len(), |n| lt + n);
        let whole = &src[lt..open_end.max(offset)];
        let existing = whole
            .split(|c: char| c.is_whitespace())
            .skip(1)
            .filter_map(|p| p.split('=').next())
            .map(str::to_string)
            .collect();

        return Some(Spot::Attr {
            tag,
            typed,
            span,
            existing,
        });
    }

    None
}

fn table_spot(src: &str, offset: usize, table: &str) -> Spot {
    // After `key = "`: a value.
    if table.matches('"').count() % 2 == 1 {
        let q = offset - (table.len() - table.rfind('"').unwrap_or(0));
        let key_part = src[..q].trim_end().trim_end_matches('=').trim_end();
        let (key, _) = word_before(key_part, key_part.len(), |b| {
            b.is_ascii_alphanumeric() || b == b'_'
        });
        let (typed, span) = word_before(src, offset, is_css_word);

        return Spot::StyleValue {
            prop: css::kebab(&key),
            typed,
            span,
        };
    }

    let (typed, span) = word_before(src, offset, |b| b.is_ascii_alphanumeric() || b == b'_');

    Spot::StyleKey { typed, span }
}

fn css_spot(src: &str, css_start: usize, offset: usize) -> Spot {
    let text = &src[css_start..offset];
    let depth = text.matches('{').count() as i64 - text.matches('}').count() as i64;
    let (typed, span) = word_before(src, offset, is_css_word);

    if depth <= 0 {
        // `.card` and `#main` keep their sigil in the typed text.
        let s = span.0.saturating_sub(1);
        let (typed, span) = match src.as_bytes().get(s) {
            Some(b'.' | b'#' | b':') if span.0 > css_start => {
                (src[s..offset].to_string(), (s, span.1))
            }

            _ => (typed, span),
        };

        return Spot::Selector { typed, span };
    }

    let decl_start = text.rfind([';', '{']).map_or(0, |n| n + 1);
    let decl = &text[decl_start..];

    match decl.find(':') {
        Some(colon) => Spot::PropValue {
            prop: decl[..colon].trim().to_ascii_lowercase(),
            typed,
            span,
        },

        None => Spot::Property { typed, span },
    }
}

/// The CSS keywords a property takes, colors for a color property.
fn values_of(prop: &str) -> Vec<(String, &'static str)> {
    let catalog = props::catalog();
    let mut out: Vec<(String, &'static str)> = catalog
        .iter()
        .find(|k| k.name == prop)
        .map(|k| {
            k.values
                .iter()
                .map(|v| (v.to_string(), "keyword"))
                .collect()
        })
        .unwrap_or_default();

    if prop.contains("color") || prop == "background" || prop == "border" || prop == "outline" {
        out.extend(css::NAMED.iter().map(|(n, _)| (n.to_string(), "color")));
    }

    out
}

pub fn complete(src: &str, offset: usize, opts: &emit::Options) -> Completions {
    let Some(spot) = spot(src, offset) else {
        return Completions::default();
    };
    let sheets = emit::sheets(src);
    let starts = |label: &str, typed: &str| {
        label
            .to_ascii_lowercase()
            .starts_with(&typed.to_ascii_lowercase())
    };

    match spot {
        Spot::Tag(typed, span) => {
            let items = html::TAGS
                .iter()
                .filter(|t| starts(t.name, &typed))
                .map(|t| {
                    let class = html::class_of(t, None).unwrap_or(match t.kind {
                        Kind::Unsupported => "no Roblox form",

                        Kind::Style => "StyleSheet",

                        _ => "no instance",
                    });

                    CompletionItem::new(t.name)
                        .kind(ItemKind::Class)
                        .detail(format!("HTML \u{2192} {class}"))
                        .documentation(t.doc)
                        .over(u(span))
                })
                .collect();

            Completions::new(items)
                .merge(true)
                .hide_roblox(!opts.roblox)
        }

        Spot::Attr {
            tag,
            typed,
            span,
            existing,
        } => {
            let Some(t) = html::tag(&tag) else {
                return Completions::default();
            };
            let items: Vec<CompletionItem> = html::attributes(t, None)
                .into_iter()
                .filter(|(n, _)| starts(n, &typed) && !existing.iter().any(|x| x == n))
                .map(|(n, becomes)| {
                    let snippet = match n {
                        "style" => "style={{ $1 }}".to_string(),

                        "hidden" | "disabled" | "readOnly" | "autoPlay" | "loop" | "muted"
                        | "open" | "reversed" => n.to_string(),

                        _ if html::event(n).is_some() => format!("{n}={{function()\n\t$0\nend}}"),

                        _ if html::is_change(n) => format!("{n}={{function(text)\n\t$0\nend}}"),

                        "tabIndex" | "value" | "max" | "min" | "start" | "rows" | "cols"
                        | "width" | "height" | "key" => format!("{n}={{$1}}"),

                        _ => format!("{n}=\"$1\""),
                    };

                    CompletionItem::new(n)
                        .kind(match html::event(n).is_some() || html::is_change(n) {
                            true => ItemKind::Event,

                            false => ItemKind::Property,
                        })
                        .detail(format!(
                            "HTML attribute \u{2192} {}",
                            becomes.split('.').next().unwrap_or(becomes)
                        ))
                        .documentation(format!("`{n}`: {becomes}."))
                        .snippet(snippet)
                        .over(u(span))
                })
                .collect();
            let class = html::class_of(t, None).unwrap_or("Frame");

            Completions::new(items)
                .merge(opts.roblox)
                .hide_roblox(!opts.roblox)
                .as_class(class)
        }

        Spot::AttrValue {
            tag,
            attr,
            typed,
            span,
        } => {
            let words: Vec<(String, String)> = match (tag.as_str(), attr.as_str()) {
                ("input", "type") => html::TEXT_INPUTS
                    .iter()
                    .chain(["button", "submit", "reset"].iter())
                    .map(|t| (t.to_string(), "a TextBox or a TextButton".to_string()))
                    .collect(),

                (_, "className" | "class") => {
                    let mut names = BTreeSet::new();

                    for (_, sheet) in &sheets {
                        for rule in &sheet.rules {
                            for s in &rule.selectors {
                                for (_, c) in &s.parts {
                                    names.extend(c.classes.iter().cloned());
                                }
                            }
                        }
                    }

                    names
                        .into_iter()
                        .map(|n| (n, "a class a `<style>` in this file styles".to_string()))
                        .collect()
                }

                (_, "href") => ids(src)
                    .into_iter()
                    .map(|id| {
                        (
                            format!("#{id}"),
                            "scrolls to the element with this `id`".to_string(),
                        )
                    })
                    .collect(),

                _ => Vec::new(),
            };
            let span = match attr.as_str() {
                "href" => (span.0, span.1),

                _ => span,
            };
            let items = words
                .into_iter()
                .filter(|(w, _)| starts(w, &typed) || attr == "href")
                .map(|(w, d)| {
                    CompletionItem::new(w)
                        .kind(ItemKind::Value)
                        .detail(d)
                        .over(u(span))
                })
                .collect();

            Completions::new(items)
        }

        Spot::StyleKey { typed, span } => {
            let items = props::catalog()
                .iter()
                .map(|k| (css::camel(k.name), k))
                .filter(|(c, _)| starts(c, &typed))
                .map(|(c, k)| {
                    CompletionItem::new(c.clone())
                        .kind(ItemKind::Property)
                        .detail(format!("CSS {}", k.name))
                        .documentation(format!("`{}` sets {}.", k.name, k.sets))
                        .snippet(format!("{c} = \"$1\""))
                        .over(u(span))
                })
                .collect();

            Completions::new(items)
        }

        Spot::StyleValue { prop, typed, span } | Spot::PropValue { prop, typed, span } => {
            let mut items: Vec<CompletionItem> = values_of(&prop)
                .into_iter()
                .filter(|(v, _)| starts(v, &typed))
                .map(|(v, kind)| {
                    let item = CompletionItem::new(v.clone()).detail(kind).over(u(span));

                    match kind {
                        "color" => {
                            let hex = css::color(&v).map(|c| c.hex()).unwrap_or_default();
                            item.kind(ItemKind::Color).documentation(hex)
                        }

                        _ => item.kind(ItemKind::Value),
                    }
                })
                .collect();

            for (_, sheet) in &sheets {
                for (name, value) in &sheet.vars {
                    if starts("var", &typed) || typed.is_empty() {
                        items.push(
                            CompletionItem::new(format!("var({name})"))
                                .kind(ItemKind::Variable)
                                .detail(value.clone())
                                .over(u(span)),
                        );
                    }
                }
            }

            Completions::new(items)
        }

        Spot::Property { typed, span } => {
            let items = props::catalog()
                .iter()
                .filter(|k| starts(k.name, &typed))
                .map(|k| {
                    CompletionItem::new(k.name)
                        .kind(ItemKind::Property)
                        .detail("CSS")
                        .documentation(format!("`{}` sets {}.", k.name, k.sets))
                        .snippet(format!("{}: $1;", k.name))
                        .over(u(span))
                })
                .collect();

            Completions::new(items)
        }

        Spot::Selector { typed, span } => {
            let mut items: Vec<CompletionItem> = Vec::new();

            if typed.starts_with('.') {
                let names: BTreeSet<String> = values_named(src, &["className", "class"])
                    .iter()
                    .flat_map(|v| v.split_whitespace().map(str::to_string))
                    .collect();

                items.extend(names.into_iter().map(|n| {
                    CompletionItem::new(format!(".{n}"))
                        .kind(ItemKind::Class)
                        .detail("a class in this file")
                        .over(u(span))
                }));
            } else if typed.starts_with('#') {
                items.extend(ids(src).into_iter().map(|id| {
                    CompletionItem::new(format!("#{id}"))
                        .kind(ItemKind::Reference)
                        .detail("an id in this file")
                        .over(u(span))
                }));
            } else if typed.starts_with(':') {
                for (p, d) in [
                    (":hover", "`:Hover`"),
                    (":active", "`:Press`"),
                    (":disabled", "`:NonInteractable`"),
                    ("::placeholder", "`PlaceholderColor3` of a TextBox"),
                ] {
                    items.push(
                        CompletionItem::new(p)
                            .kind(ItemKind::Keyword)
                            .detail(d)
                            .over(u(span)),
                    );
                }
            } else {
                items.extend(
                    html::TAGS
                        .iter()
                        .filter(|t| html::class_of(t, None).is_some() && starts(t.name, &typed))
                        .map(|t| {
                            CompletionItem::new(t.name)
                                .kind(ItemKind::Class)
                                .detail("HTML element")
                                .documentation(t.doc)
                                .over(u(span))
                        }),
                );
            }

            Completions::new(items)
        }
    }
}

/// The string values of an attribute across a file, read from the text,
/// so a file being typed still answers.
fn values_named(src: &str, names: &[&str]) -> Vec<String> {
    let mut out = Vec::new();

    for name in names {
        let pat = format!("{name}=\"");
        let mut from = 0;

        while let Some(at) = src[from..].find(&pat) {
            let start = from + at + pat.len();
            let before = src[..from + at].chars().last();
            from = start;

            if before.is_some_and(|c| c.is_alphanumeric()) {
                continue;
            }

            if let Some(end) = src[start..].find('"') {
                out.push(src[start..start + end].to_string());
            }
        }
    }

    out
}

/// The `id` values of a file.
fn ids(src: &str) -> Vec<String> {
    let mut out = values_named(src, &["id"]);
    out.sort();
    out.dedup();

    out
}

pub fn hover(src: &str, offset: usize) -> Option<Hover> {
    // A CSS property or a selector in a `<style>`.
    if let Some((css_start, css_end)) = style_at(src, offset) {
        let sheet = css::parse_sheet(&src[css_start..css_end], css_start);

        for rule in &sheet.rules {
            for d in &rule.decls {
                if d.name_span.0 <= offset && offset <= d.name_span.1 {
                    return property_hover(&d.name).map(|h| h.over(u(d.name_span)));
                }

                if d.value_span.0 <= offset && offset <= d.value_span.1 {
                    return property_hover(&d.name).map(|h| h.over(u(d.value_span)));
                }
            }

            for s in &rule.selectors {
                if s.span.0 <= offset && offset <= s.span.1 {
                    let (a, b, c) = s.specificity();
                    let roblox = match emit::roblox_selector(s, true) {
                        Ok(r) => format!("The StyleRule selector: `{r}`."),

                        Err(why) => format!("No StyleRule: {why}."),
                    };

                    return Some(
                        Hover::new(format!(
                            "```css\n{}\n```\n{roblox}\n\nSpecificity ({a}, {b}, {c}): the higher one wins, then the later rule.",
                            s.raw
                        ))
                        .over(u(s.span)),
                    );
                }
            }
        }

        return None;
    }

    let m = Markup::read(src);
    let Some(i) = m.at(offset) else {
        return text_hover(src, offset);
    };
    let e = &m.elements[i];
    let tag = html::tag(&e.name)?;
    let on_name = (e.name_span.0 <= offset && offset <= e.name_span.1)
        || e.close.is_some_and(|(s, t)| s <= offset && offset <= t);

    if on_name {
        let class = html::class_of(tag, e.text(src, "type")).map_or_else(
            || match tag.kind {
                Kind::Style => "a StyleLink to a StyleSheet".to_string(),

                Kind::Group => "a fragment".to_string(),

                _ => "no instance".to_string(),
            },
            |c| format!("Roblox `{c}`"),
        );

        return Some(
            Hover::new(format!(
                "```alx\n<{}>\n```\nHTML element \u{2192} {class}.\n\n{}",
                tag.name, tag.doc
            ))
            .over(u(e.name_span)),
        );
    }

    for a in &e.attrs {
        if a.name_span.0 <= offset && offset <= a.name_span.1 {
            let list = html::attributes(tag, e.text(src, "type"));
            let react = html::react_name(&a.name);
            let lower = a.name.to_ascii_lowercase();
            let becomes = list
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(react.unwrap_or(&a.name)))
                .map(|(_, b)| *b)
                .or_else(|| {
                    (lower.starts_with("data-") || lower.starts_with("aria-"))
                        .then_some("nothing: Silk drops it, since Roblox has no place for it")
                });
            let react = react
                .map(|r| format!("\n\nReact writes `{r}`."))
                .unwrap_or_default();

            return becomes
                .map(|b| Hover::new(format!("`{}`: {b}.{react}", a.name)).over(u(a.name_span)));
        }

        // A key of a `style={{ }}` table.
        if let (Value::Expr(s, t), "style") = (a.value, a.name.as_str())
            && s <= offset
            && offset <= t
        {
            let (decls, _) = css::parse_table(&src[s..t], s);

            for d in decls {
                if d.name_span.0 <= offset && offset <= d.name_span.1 {
                    return property_hover(&d.name).map(|h| h.over(u(d.name_span)));
                }
            }
        }
    }

    None
}

/// A hover read from the text alone, for a file that does not parse: a
/// tag name after `<` or `</`, or an attribute name in an open tag.
fn text_hover(src: &str, offset: usize) -> Option<Hover> {
    let (_, span) = word_before(src, offset, |b| b.is_ascii_alphanumeric() || b == b'-');
    let word = &src[span.0..span.1];
    let before = &src[..span.0];

    if before.ends_with('<') || before.ends_with("</") {
        let tag = html::tag(word)?;

        return Some(Hover::new(format!("```alx\n<{}>\n```\n{}", tag.name, tag.doc)).over(u(span)));
    }

    let (_, name) = open_tag(src, span.0)?;
    let tag = html::tag(&name)?;
    let becomes = html::attributes(tag, None)
        .into_iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(word))
        .map(|(_, b)| b)?;

    Some(Hover::new(format!("`{word}`: {becomes}.")).over(u(span)))
}

fn property_hover(name: &str) -> Option<Hover> {
    if props::NO_EFFECT.contains(&name) {
        return Some(Hover::new(format!(
            "```css\n{name}\n```\nNo Roblox property stands behind `{name}`, so it sets nothing."
        )));
    }

    let catalog = props::catalog();
    let k = catalog.iter().find(|k| k.name == name)?;
    let values = match k.values.is_empty() {
        true => String::new(),

        false => format!(
            "\n\nValues: {}.",
            k.values
                .iter()
                .map(|v| format!("`{v}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    };

    Some(Hover::new(format!(
        "```css\n{name}\n```\nSets {}.{values}\n\nReact writes it `{}`.",
        k.sets,
        css::camel(name)
    )))
}

/// The colors a file names in CSS: in `<style>` values and `style`
/// tables.
pub fn colors(src: &str) -> Vec<ColorInfo> {
    let m = Markup::read(src);
    let mut out = Vec::new();
    let mut values: Vec<(String, (usize, usize))> = Vec::new();

    for e in &m.elements {
        if e.name == "style"
            && let Some((s, t)) = e.css(src)
        {
            for rule in css::parse_sheet(&src[s..t], s).rules {
                values.extend(rule.decls.into_iter().map(|d| (d.value, d.value_span)));
            }
        }

        if let Some(a) = e.attr("style")
            && let markup::Value::Expr(s, t) = a.value
        {
            values.extend(
                css::parse_table(&src[s..t], s)
                    .0
                    .into_iter()
                    .map(|d| (d.value, d.value_span)),
            );
        }
    }

    for (value, (vs, _)) in values {
        // The value text sits at `vs`; each color word inside it colors.
        let mut at = 0;

        for word in css::words(&value) {
            let Some(rel) = value[at..].find(word) else {
                continue;
            };
            let start = at + rel;
            at = start + word.len();
            let candidates: Vec<&str> =
                match word.contains('(') && !word.starts_with("rgb") && !word.starts_with("hsl") {
                    true => Vec::new(),

                    false => vec![word],
                };

            for w in candidates {
                if let Some(c) = css::color(w).filter(|_| !w.eq_ignore_ascii_case("transparent")) {
                    out.push(ColorInfo::rgb(
                        ((vs + start) as u32, (vs + start + w.len()) as u32),
                        (c.r, c.g, c.b),
                        c.a,
                    ));
                }
            }
        }
    }

    out
}

/// The texts that name a picked color: a hex, with its alpha when it is
/// not opaque.
pub fn present(color: alloy_ingot::Color) -> Vec<String> {
    let (r, g, b) = color.rgb8();

    match color.alpha < 1.0 {
        true => vec![
            format!("rgba({r}, {g}, {b}, {})", css::num(color.alpha)),
            format!(
                "#{r:02X}{g:02X}{b:02X}{:02X}",
                (color.alpha * 255.0).round() as u8
            ),
        ],

        false => vec![
            format!("#{r:02X}{g:02X}{b:02X}"),
            format!("rgb({r}, {g}, {b})"),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(src: &str) -> Vec<String> {
        let at = src.find('|').unwrap();
        let text = src.replacen('|', "", 1);

        complete(&text, at, &emit::Options::default())
            .items
            .into_iter()
            .map(|i| i.label)
            .collect()
    }

    #[test]
    fn completes_tags_attributes_and_style_tables() {
        assert!(labels("return <di|").contains(&"div".to_string()));
        assert!(labels("return <div cla|").contains(&"className".to_string()));
        assert!(labels("return <button on|").contains(&"onClick".to_string()));
        assert!(
            labels("return <div style={{ backgroundC| }}>")
                .contains(&"backgroundColor".to_string())
        );
        assert!(labels("return <div style={{ display = \"fl|\" }}>").contains(&"flex".to_string()));
        assert!(labels("return <input type=\"pass|\" />").contains(&"password".to_string()));
    }

    #[test]
    fn completes_css_in_a_style_element() {
        let base = "return <div className=\"card\" id=\"top\">\n<style>\n";
        assert!(labels(&format!("{base}.ca|")).contains(&".card".to_string()));
        assert!(labels(&format!("{base}#t|")).contains(&"#top".to_string()));
        assert!(
            labels(&format!("{base}.card {{ backgr|")).contains(&"background-color".to_string())
        );
        assert!(labels(&format!("{base}.card {{ color: re|")).contains(&"red".to_string()));
        assert!(labels(&format!("{base}.card {{ display: |")).contains(&"flex".to_string()));
    }

    #[test]
    fn a_tag_completion_asks_for_the_markup_list() {
        let c = complete(
            "return <d",
            9,
            &emit::Options {
                roblox: false,
                ..emit::Options::default()
            },
        );

        assert!(c.merge);
        assert!(c.hide_roblox);
    }

    #[test]
    fn hovers_tags_attributes_and_properties() {
        let src = "return <img src=\"x\" style={{ objectFit = \"cover\" }} />\n<style>\n.a { border-radius: 4px }\n</style>";
        let h = |at: &str| hover(src, src.find(at).unwrap() + 1).map(|h| h.contents);

        assert!(h("img").unwrap().contains("ImageLabel"));
        assert!(h("src").unwrap().contains("`Image`"));
        assert!(h("objectFit").unwrap().contains("ScaleType"));
        assert!(h("border-radius").unwrap().contains("UICorner"));
        assert!(h("4px").unwrap().contains("UICorner"));

        let src = "return <div class=\"a\" data-id=\"1\">x</div>\n";
        let h = |at: &str| hover(src, src.find(at).unwrap() + 1).map(|h| h.contents);

        assert!(h("class").unwrap().contains("CollectionService"));
        assert!(h("data-id").unwrap().contains("drops it"));
    }

    #[test]
    fn a_style_tag_in_a_comment_or_string_opens_no_css() {
        let src = "-- a <style> here\nlocal s = \"<style>\"\nreturn <div cla";

        assert!(style_at(src, src.len()).is_none());
        assert!(matches!(spot(src, src.len()), Some(Spot::Attr { .. })));
    }

    #[test]
    fn a_file_that_does_not_parse_still_answers() {
        let src = "return <div>\n  <sec\n  <button onC\n  <p className=\"a\">x</p>\n";
        let at = src.find("onC").unwrap() + 3;
        let items = complete(src, at, &emit::Options::default()).items;

        assert!(items.iter().any(|i| i.label == "onClick"), "{items:?}");
        assert!(hover(src, src.find("className").unwrap() + 2).is_some());
    }

    #[test]
    fn finds_colors_in_css() {
        let src = "return <div style={{ color = \"#ff0000\" }}>\n<style>\n.a { border: 1px solid rgb(0, 255, 0) }\n</style>\n</div>";
        let colors = colors(src);

        assert_eq!(colors.len(), 2);
        assert_eq!(
            &src[colors[0].span.0 as usize..colors[0].span.1 as usize],
            "#ff0000"
        );
    }
}
