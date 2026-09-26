//! CSS as Silk reads it: a stylesheet of rules, a declaration list for a
//! `style` attribute, selectors, and the values a declaration holds.
//! Every span is a byte offset into the whole `.alx` file.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Decl {
    /// The property as written, lower case: `background-color`, or a
    /// Roblox name such as `BackgroundColor3`.
    pub name: String,
    pub name_span: (usize, usize),
    pub value: String,
    pub value_span: (usize, usize),
    pub important: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub selector_span: (usize, usize),
    pub decls: Vec<Decl>,
    pub span: (usize, usize),
}

/// A problem in the CSS text, with the lint that reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub span: (usize, usize),
    pub lint: &'static str,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Sheet {
    pub rules: Vec<Rule>,
    /// The custom properties `:root`, `html`, and `*` declare.
    pub vars: BTreeMap<String, String>,
    pub problems: Vec<Problem>,
}

/// Blanks `/* */` comments to spaces, so every span stays where it was.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(s) = rest.find("/*") {
        out.push_str(&rest[..s]);
        let end = rest[s + 2..]
            .find("*/")
            .map_or(rest.len(), |e| s + 2 + e + 2);
        out.extend(
            rest[s..end]
                .chars()
                .map(|c| if c == '\n' { '\n' } else { ' ' }),
        );
        // A multi-byte character becomes one space; pad to its byte length.
        let blanked: usize = rest[s..end].chars().map(|c| c.len_utf8() - 1).sum();
        out.push_str(&" ".repeat(blanked));
        rest = &rest[end..];
    }

    out.push_str(rest);

    out
}

/// Reads a stylesheet. `base` is the offset of `text` in the file.
pub fn parse_sheet(text: &str, base: usize) -> Sheet {
    let clean = strip_comments(text);
    let t = clean.as_str();
    let mut sheet = Sheet::default();
    let mut i = 0;

    while i < t.len() {
        let rest = &t[i..];
        let skip = rest.len() - rest.trim_start().len();
        i += skip;

        if i >= t.len() {
            break;
        }

        // An at-rule: `@media (...) { ... }` or `@import ...;`.
        if t[i..].starts_with('@') {
            let brace = t[i..].find('{');
            let semi = t[i..].find(';');
            let end = match (brace, semi) {
                (Some(b), Some(s)) if s < b => i + s + 1,

                (Some(b), _) => block_end(t, i + b).unwrap_or(t.len()),

                (None, Some(s)) => i + s + 1,

                (None, None) => t.len(),
            };
            let head_end = t[i..]
                .find(|c: char| c.is_whitespace() || c == '{' || c == ';')
                .map_or(end, |n| i + n);
            sheet.problems.push(Problem {
                span: (base + i, base + head_end),
                lint: "unsupported_css",
                message: format!(
                    "`{}` has no Roblox form, so Silk skips the block",
                    &t[i..head_end]
                ),
            });
            i = end;

            continue;
        }

        let Some(open) = t[i..].find('{').map(|n| i + n) else {
            sheet.problems.push(Problem {
                span: (base + i, base + t.len()),
                lint: "css_syntax",
                message: "a selector with no `{ }` block".into(),
            });

            break;
        };
        let close = block_end(t, open).unwrap_or(t.len());
        let selector_text = &t[i..open];
        let trimmed_end = i + selector_text.trim_end().len();
        let selector_span = (base + i, base + trimmed_end);
        let selectors = parse_selectors(&t[i..trimmed_end], base + i);
        let body_end = if close > open && t[..close].ends_with('}') {
            close - 1
        } else {
            close
        };
        let decls = parse_decls_clean(&t[open + 1..body_end], base + open + 1);

        // Custom properties on the root are the file's variables.
        let root = selectors.iter().any(|s| {
            let raw = s.raw.trim();
            raw == ":root" || raw == "html" || raw == "*"
        });

        if root {
            for d in &decls {
                if d.name.starts_with("--") {
                    sheet.vars.insert(d.name.clone(), d.value.clone());
                }
            }
        }

        if close == t.len() && !t.ends_with('}') {
            sheet.problems.push(Problem {
                span: (base + open, base + open + 1),
                lint: "css_syntax",
                message: "a `{` with no `}`".into(),
            });
        }

        sheet.rules.push(Rule {
            selectors,
            selector_span,
            decls,
            span: (base + i, base + close),
        });
        i = close;
    }

    sheet
}

/// The offset one past the `}` that closes the `{` at `open`.
fn block_end(t: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;

    for (k, c) in t[open..].char_indices() {
        match c {
            '{' => depth += 1,

            '}' => {
                depth -= 1;

                if depth == 0 {
                    return Some(open + k + 1);
                }
            }

            _ => {}
        }
    }

    None
}

/// Reads a declaration list, the text of a `style` attribute.
pub fn parse_decls(text: &str, base: usize) -> Vec<Decl> {
    parse_decls_clean(&strip_comments(text), base)
}

fn parse_decls_clean(t: &str, base: usize) -> Vec<Decl> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    let mut pieces = Vec::new();

    for (k, c) in t.char_indices() {
        match c {
            '(' => depth += 1,

            ')' => depth -= 1,

            ';' if depth <= 0 => {
                pieces.push((start, k));
                start = k + 1;
            }

            _ => {}
        }
    }

    pieces.push((start, t.len()));

    for (s, e) in pieces {
        let piece = &t[s..e];

        let Some(colon) = piece.find(':') else {
            continue;
        };
        let raw_name = &piece[..colon];
        let name = raw_name.trim();

        if name.is_empty() {
            continue;
        }

        let name_start = s + raw_name.find(name).unwrap_or(0);
        let raw_value = &piece[colon + 1..];
        let mut value = raw_value.trim();
        let mut important = false;

        if value.to_ascii_lowercase().ends_with("!important") {
            value = value[..value.len() - "!important".len()].trim_end();
            important = true;
        }

        let value_start = s + colon + 1 + raw_value.find(value).unwrap_or(0);
        // CSS names are case blind; a Roblox property is written as
        // itself, in PascalCase.
        let lower = match name.starts_with("--") || is_roblox_name(name) {
            true => name.to_string(),

            false => name.to_ascii_lowercase(),
        };

        out.push(Decl {
            name: lower,
            name_span: (base + name_start, base + name_start + name.len()),
            value: value.to_string(),
            value_span: (base + value_start, base + value_start + value.len()),
            important,
        });
    }

    out
}

// ------------------------------------------------------------ React style

/// The CSS properties React writes as a bare number, with no `px`.
const UNITLESS: &[&str] = &[
    "animation-iteration-count",
    "aspect-ratio",
    "column-count",
    "columns",
    "flex",
    "flex-grow",
    "flex-shrink",
    "font-weight",
    "grid-area",
    "grid-column",
    "grid-row",
    "line-clamp",
    "line-height",
    "opacity",
    "order",
    "orphans",
    "scale",
    "tab-size",
    "widows",
    "z-index",
    "zoom",
];

/// Whether a property name is a Roblox one: PascalCase, `BackgroundColor3`.
/// A CSS name in capitals, `COLOR` or `Background-Color`, is not.
pub fn is_roblox_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
        && name.chars().any(|c| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `backgroundColor` as `background-color`, and `WebkitTextStroke` as
/// `-webkit-text-stroke`.
pub fn kebab(name: &str) -> String {
    if name.starts_with("--") {
        return name.to_string();
    }

    let mut out = String::new();

    for (k, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            out.push('-');
            out.push(c.to_ascii_lowercase());
        } else {
            if k == 0
                && (name.starts_with("ms")
                    && name[2..].starts_with(|c: char| c.is_ascii_uppercase()))
            {
                out.push('-');
            }

            out.push(c);
        }
    }

    out
}

/// `background-color` as `backgroundColor`.
pub fn camel(name: &str) -> String {
    let mut out = String::new();
    let mut up = false;

    for c in name.trim_start_matches('-').chars() {
        match (c, up) {
            ('-', _) => up = true,

            (c, true) => {
                out.push(c.to_ascii_uppercase());
                up = false;
            }

            (c, false) => out.push(c),
        }
    }

    match name.starts_with('-') {
        true => {
            let mut chars = out.chars();

            chars.next().map_or(out.clone(), |f| {
                f.to_ascii_uppercase().to_string() + chars.as_str()
            })
        }

        false => out,
    }
}

/// Reads a React style table, `{ backgroundColor = "#fff", padding = 8 }`,
/// as declarations. `base` is the offset of `text` in the file. A field
/// whose value is not a literal is left out, and a problem says so.
pub fn parse_table(text: &str, base: usize) -> (Vec<Decl>, Vec<Problem>) {
    let mut decls = Vec::new();
    let mut problems = Vec::new();
    let trimmed = text.trim();
    let lead = text.len() - text.trim_start().len();

    let dynamic = |span: (usize, usize), what: &str| Problem {
        span,
        lint: "dynamic_style",
        message: format!(
            "Silk reads `style` at compile time, and {what} is not a literal; set the property as an attribute instead"
        ),
    };

    let Some(inner) = trimmed.strip_prefix('{').and_then(|r| r.strip_suffix('}')) else {
        problems.push(dynamic(
            (base + lead, base + lead + trimmed.len()),
            "this table",
        ));

        return (decls, problems);
    };
    let inner_base = base + lead + 1;

    for (s, e) in fields(inner) {
        let field = &inner[s..e];
        let Some(eq) = top_eq(field) else {
            continue;
        };
        let raw_key = field[..eq].trim();
        let key_at = s + field.find(raw_key).unwrap_or(0);
        let key = raw_key
            .trim_start_matches('[')
            .trim_end_matches(']')
            .trim()
            .trim_matches(|c| c == '"' || c == '\'');
        // A Roblox property keeps its name; `WebkitTextStroke` is a
        // vendor prefix.
        let vendor = ["Webkit", "Moz", "Ms", "O"].iter().any(|p| {
            key.strip_prefix(p)
                .is_some_and(|r| r.starts_with(|c: char| c.is_ascii_uppercase()))
        });
        let name = match is_roblox_name(key) && !vendor {
            true => key.to_string(),

            false => kebab(key),
        };
        let raw_value = &field[eq + 1..];
        let value = raw_value.trim();
        let value_at = s + eq + 1 + (raw_value.len() - raw_value.trim_start().len());
        let name_span = (inner_base + key_at, inner_base + key_at + raw_key.len());

        let text = if (value.starts_with('"') && value.ends_with('"')
            || value.starts_with('\'') && value.ends_with('\''))
            && value.len() >= 2
        {
            Some((
                value[1..value.len() - 1].to_string(),
                (
                    inner_base + value_at + 1,
                    inner_base + value_at + value.len() - 1,
                ),
            ))
        } else if let Ok(n) = value.parse::<f64>() {
            let unit = match UNITLESS.contains(&name.as_str()) || n == 0.0 {
                true => "",

                false => "px",
            };

            Some((
                format!("{}{unit}", num(n)),
                (inner_base + value_at, inner_base + value_at + value.len()),
            ))
        } else {
            None
        };

        match text {
            Some((v, value_span)) => decls.push(Decl {
                name,
                name_span,
                value: v,
                value_span,
                important: false,
            }),

            None => problems.push(dynamic(
                (inner_base + value_at, inner_base + value_at + value.len()),
                &format!("`{key}`"),
            )),
        }
    }

    (decls, problems)
}

/// The spans of the fields of a table's inside, split on top-level `,`
/// and `;`.
fn fields(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut start = 0;
    let mut escaped = false;

    for (k, c) in text.char_indices() {
        if let Some(q) = quote {
            match (c, escaped) {
                ('\\', false) => escaped = true,

                (c, false) if c == q => quote = None,

                _ => escaped = false,
            }

            continue;
        }

        match c {
            '"' | '\'' => quote = Some(c),

            '(' | '{' | '[' => depth += 1,

            ')' | '}' | ']' => depth -= 1,

            ',' | ';' if depth == 0 => {
                out.push((start, k));
                start = k + 1;
            }

            _ => {}
        }
    }

    out.push((start, text.len()));
    out.retain(|(s, e)| !text[*s..*e].trim().is_empty());

    out
}

/// The `=` of a field, outside brackets and strings.
fn top_eq(field: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;

    for (k, c) in field.char_indices() {
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }

            continue;
        }

        match c {
            '"' | '\'' => quote = Some(c),

            '[' | '(' | '{' => depth += 1,

            ']' | ')' | '}' => depth -= 1,

            '=' if depth == 0 && !field[k + 1..].starts_with('=') => return Some(k),

            _ => {}
        }
    }

    None
}

// -------------------------------------------------------------- selectors

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
    Adjacent,
    Sibling,
}

/// One compound selector: `div.card#main:hover`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Compound {
    pub tag: Option<String>,
    pub universal: bool,
    pub ids: Vec<String>,
    pub classes: Vec<String>,
    /// Pseudo-classes, lower case, without the colon.
    pub pseudo: Vec<String>,
    /// A pseudo-element or a Roblox pseudo-instance, without the colons.
    pub pseudo_element: Option<String>,
    /// An attribute selector or another piece Silk cannot write.
    pub unsupported: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    pub raw: String,
    pub span: (usize, usize),
    /// The compounds from left to right, each with the combinator that
    /// joins it to the one before.
    pub parts: Vec<(Combinator, Compound)>,
}

impl Selector {
    /// CSS specificity: ids, classes and pseudo-classes, tags.
    pub fn specificity(&self) -> (u32, u32, u32) {
        self.parts.iter().fold((0, 0, 0), |(a, b, c), (_, p)| {
            (
                a + p.ids.len() as u32,
                b + (p.classes.len() + p.pseudo.len()) as u32,
                c + u32::from(p.tag.is_some()) + u32::from(p.pseudo_element.is_some()),
            )
        })
    }

    pub fn last(&self) -> Option<&Compound> {
        self.parts.last().map(|(_, c)| c)
    }
}

fn parse_selectors(text: &str, base: usize) -> Vec<Selector> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;

    let push = |s: usize, e: usize, out: &mut Vec<Selector>| {
        let piece = &text[s..e];
        let t = piece.trim();

        if !t.is_empty() {
            let off = s + piece.find(t).unwrap_or(0);
            out.push(parse_selector(t, base + off));
        }
    };

    for (k, c) in text.char_indices() {
        match c {
            '(' | '[' => depth += 1,

            ')' | ']' => depth -= 1,

            ',' if depth == 0 => {
                push(start, k, &mut out);
                start = k + 1;
            }

            _ => {}
        }
    }

    push(start, text.len(), &mut out);

    out
}

fn ident_end(t: &str, i: usize) -> usize {
    t[i..]
        .find(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .map_or(t.len(), |n| i + n)
}

pub fn parse_selector(t: &str, base: usize) -> Selector {
    let mut parts: Vec<(Combinator, Compound)> = Vec::new();
    let mut current = Compound::default();
    let mut pending = Combinator::Descendant;
    let mut started = false;
    let b = t.as_bytes();
    let mut i = 0;

    let flush = |current: &mut Compound, parts: &mut Vec<(Combinator, Compound)>, comb| {
        parts.push((comb, std::mem::take(current)));
    };

    while i < b.len() {
        match b[i] {
            c if c.is_ascii_whitespace() || c == b'>' || c == b'+' || c == b'~' => {
                let mut comb = Combinator::Descendant;

                while i < b.len()
                    && matches!(b[i], b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'+' | b'~')
                {
                    comb = match b[i] {
                        b'>' if b.get(i + 1) == Some(&b'>') => {
                            i += 1;
                            Combinator::Descendant
                        }

                        b'>' => Combinator::Child,

                        b'+' => Combinator::Adjacent,

                        b'~' => Combinator::Sibling,

                        _ => comb,
                    };
                    i += 1;
                }

                if started {
                    flush(&mut current, &mut parts, pending);
                    pending = comb;
                    started = false;
                }
            }

            b'.' => {
                let e = ident_end(t, i + 1);
                current.classes.push(t[i + 1..e].to_string());
                started = true;
                i = e;
            }

            b'#' => {
                let e = ident_end(t, i + 1);
                current.ids.push(t[i + 1..e].to_string());
                started = true;
                i = e;
            }

            b'*' => {
                current.universal = true;
                started = true;
                i += 1;
            }

            b'[' => {
                let e = t[i..].find(']').map_or(t.len(), |n| i + n + 1);
                current.unsupported = Some(t[i..e].to_string());
                started = true;
                i = e;
            }

            b':' if b.get(i + 1) == Some(&b':') => {
                let e = ident_end(t, i + 2);
                current.pseudo_element = Some(t[i + 2..e].to_string());
                started = true;
                i = e;
            }

            b':' => {
                let e = ident_end(t, i + 1);
                let mut end = e;

                // `:not(...)` and the other functional pseudo-classes.
                if b.get(e) == Some(&b'(') {
                    end = t[e..].find(')').map_or(t.len(), |n| e + n + 1);
                    current.unsupported = Some(t[i..end].to_string());
                } else {
                    current.pseudo.push(t[i + 1..e].to_ascii_lowercase());
                }

                started = true;
                i = end;
            }

            _ => {
                let e = ident_end(t, i).max(i + 1);
                current.tag = Some(t[i..e].to_string());
                started = true;
                i = e;
            }
        }
    }

    if started {
        flush(&mut current, &mut parts, pending);
    }

    if let Some(first) = parts.first_mut() {
        first.0 = Combinator::Descendant;
    }

    Selector {
        raw: t.to_string(),
        span: (base, base + t.len()),
        parts,
    }
}

// ------------------------------------------------------------------ values

/// An 8 bit color with an alpha from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

impl Rgba {
    pub fn luau(self) -> String {
        format!("Color3.fromRGB({}, {}, {})", self.r, self.g, self.b)
    }

    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

/// The CSS named colors, lower case.
pub const NAMED: &[(&str, u32)] = &[
    ("aliceblue", 0xF0F8FF),
    ("antiquewhite", 0xFAEBD7),
    ("aqua", 0x00FFFF),
    ("aquamarine", 0x7FFFD4),
    ("azure", 0xF0FFFF),
    ("beige", 0xF5F5DC),
    ("bisque", 0xFFE4C4),
    ("black", 0x000000),
    ("blanchedalmond", 0xFFEBCD),
    ("blue", 0x0000FF),
    ("blueviolet", 0x8A2BE2),
    ("brown", 0xA52A2A),
    ("burlywood", 0xDEB887),
    ("cadetblue", 0x5F9EA0),
    ("chartreuse", 0x7FFF00),
    ("chocolate", 0xD2691E),
    ("coral", 0xFF7F50),
    ("cornflowerblue", 0x6495ED),
    ("cornsilk", 0xFFF8DC),
    ("crimson", 0xDC143C),
    ("cyan", 0x00FFFF),
    ("darkblue", 0x00008B),
    ("darkcyan", 0x008B8B),
    ("darkgoldenrod", 0xB8860B),
    ("darkgray", 0xA9A9A9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xA9A9A9),
    ("darkkhaki", 0xBDB76B),
    ("darkmagenta", 0x8B008B),
    ("darkolivegreen", 0x556B2F),
    ("darkorange", 0xFF8C00),
    ("darkorchid", 0x9932CC),
    ("darkred", 0x8B0000),
    ("darksalmon", 0xE9967A),
    ("darkseagreen", 0x8FBC8F),
    ("darkslateblue", 0x483D8B),
    ("darkslategray", 0x2F4F4F),
    ("darkslategrey", 0x2F4F4F),
    ("darkturquoise", 0x00CED1),
    ("darkviolet", 0x9400D3),
    ("deeppink", 0xFF1493),
    ("deepskyblue", 0x00BFFF),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1E90FF),
    ("firebrick", 0xB22222),
    ("floralwhite", 0xFFFAF0),
    ("forestgreen", 0x228B22),
    ("fuchsia", 0xFF00FF),
    ("gainsboro", 0xDCDCDC),
    ("ghostwhite", 0xF8F8FF),
    ("gold", 0xFFD700),
    ("goldenrod", 0xDAA520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xADFF2F),
    ("grey", 0x808080),
    ("honeydew", 0xF0FFF0),
    ("hotpink", 0xFF69B4),
    ("indianred", 0xCD5C5C),
    ("indigo", 0x4B0082),
    ("ivory", 0xFFFFF0),
    ("khaki", 0xF0E68C),
    ("lavender", 0xE6E6FA),
    ("lavenderblush", 0xFFF0F5),
    ("lawngreen", 0x7CFC00),
    ("lemonchiffon", 0xFFFACD),
    ("lightblue", 0xADD8E6),
    ("lightcoral", 0xF08080),
    ("lightcyan", 0xE0FFFF),
    ("lightgoldenrodyellow", 0xFAFAD2),
    ("lightgray", 0xD3D3D3),
    ("lightgreen", 0x90EE90),
    ("lightgrey", 0xD3D3D3),
    ("lightpink", 0xFFB6C1),
    ("lightsalmon", 0xFFA07A),
    ("lightseagreen", 0x20B2AA),
    ("lightskyblue", 0x87CEFA),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xB0C4DE),
    ("lightyellow", 0xFFFFE0),
    ("lime", 0x00FF00),
    ("limegreen", 0x32CD32),
    ("linen", 0xFAF0E6),
    ("magenta", 0xFF00FF),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66CDAA),
    ("mediumblue", 0x0000CD),
    ("mediumorchid", 0xBA55D3),
    ("mediumpurple", 0x9370DB),
    ("mediumseagreen", 0x3CB371),
    ("mediumslateblue", 0x7B68EE),
    ("mediumspringgreen", 0x00FA9A),
    ("mediumturquoise", 0x48D1CC),
    ("mediumvioletred", 0xC71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xF5FFFA),
    ("mistyrose", 0xFFE4E1),
    ("moccasin", 0xFFE4B5),
    ("navajowhite", 0xFFDEAD),
    ("navy", 0x000080),
    ("oldlace", 0xFDF5E6),
    ("olive", 0x808000),
    ("olivedrab", 0x6B8E23),
    ("orange", 0xFFA500),
    ("orangered", 0xFF4500),
    ("orchid", 0xDA70D6),
    ("palegoldenrod", 0xEEE8AA),
    ("palegreen", 0x98FB98),
    ("paleturquoise", 0xAFEEEE),
    ("palevioletred", 0xDB7093),
    ("papayawhip", 0xFFEFD5),
    ("peachpuff", 0xFFDAB9),
    ("peru", 0xCD853F),
    ("pink", 0xFFC0CB),
    ("plum", 0xDDA0DD),
    ("powderblue", 0xB0E0E6),
    ("purple", 0x800080),
    ("rebeccapurple", 0x663399),
    ("red", 0xFF0000),
    ("rosybrown", 0xBC8F8F),
    ("royalblue", 0x4169E1),
    ("saddlebrown", 0x8B4513),
    ("salmon", 0xFA8072),
    ("sandybrown", 0xF4A460),
    ("seagreen", 0x2E8B57),
    ("seashell", 0xFFF5EE),
    ("sienna", 0xA0522D),
    ("silver", 0xC0C0C0),
    ("skyblue", 0x87CEEB),
    ("slateblue", 0x6A5ACD),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xFFFAFA),
    ("springgreen", 0x00FF7F),
    ("steelblue", 0x4682B4),
    ("tan", 0xD2B48C),
    ("teal", 0x008080),
    ("thistle", 0xD8BFD8),
    ("tomato", 0xFF6347),
    ("turquoise", 0x40E0D0),
    ("violet", 0xEE82EE),
    ("wheat", 0xF5DEB3),
    ("white", 0xFFFFFF),
    ("whitesmoke", 0xF5F5F5),
    ("yellow", 0xFFFF00),
    ("yellowgreen", 0x9ACD32),
];

fn channel(text: &str, max: f64) -> Option<f64> {
    let t = text.trim();

    match t.strip_suffix('%') {
        Some(p) => p.trim().parse::<f64>().ok().map(|v| v / 100.0 * max),

        None => t.parse::<f64>().ok(),
    }
}

fn alpha(text: &str) -> Option<f64> {
    channel(text, 1.0).map(|a| a.clamp(0.0, 1.0))
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;

    (to(r), to(g), to(b))
}

/// A CSS color: a hex, `rgb()`, `rgba()`, `hsl()`, `hsla()`, a name, or
/// `transparent`.
pub fn color(text: &str) -> Option<Rgba> {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();

    if lower == "transparent" {
        return Some(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        });
    }

    if let Some(hex) = t.strip_prefix('#') {
        let digit = |i: usize, n: usize| u8::from_str_radix(&hex[i..i + n], 16).ok();
        let twice = |v: u8| v * 17;

        return match hex.len() {
            3 | 4 if hex.is_ascii() => Some(Rgba {
                r: twice(digit(0, 1)?),
                g: twice(digit(1, 1)?),
                b: twice(digit(2, 1)?),
                a: match hex.len() {
                    4 => f64::from(twice(digit(3, 1)?)) / 255.0,

                    _ => 1.0,
                },
            }),

            6 | 8 if hex.is_ascii() => Some(Rgba {
                r: digit(0, 2)?,
                g: digit(2, 2)?,
                b: digit(4, 2)?,
                a: match hex.len() {
                    8 => f64::from(digit(6, 2)?) / 255.0,

                    _ => 1.0,
                },
            }),

            _ => None,
        };
    }

    if let Some(open) = lower.find('(')
        && lower.ends_with(')')
    {
        let func = &lower[..open];
        let inner = &lower[open + 1..lower.len() - 1];
        let (main, slash_alpha) = match inner.split_once('/') {
            Some((m, a)) => (m, Some(a)),

            None => (inner, None),
        };
        let args: Vec<&str> = main
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
            .collect();
        let a = match (slash_alpha, args.get(3).copied()) {
            (Some(a), _) | (None, Some(a)) => alpha(a)?,

            (None, None) => 1.0,
        };

        return match func {
            "rgb" | "rgba" if args.len() >= 3 => {
                let c =
                    |i: usize| channel(args[i], 255.0).map(|v| v.round().clamp(0.0, 255.0) as u8);

                Some(Rgba {
                    r: c(0)?,
                    g: c(1)?,
                    b: c(2)?,
                    a,
                })
            }

            "hsl" | "hsla" if args.len() >= 3 => {
                let h = args[0].trim_end_matches("deg").parse::<f64>().ok()?;
                let s = channel(args[1], 1.0)?;
                let l = channel(args[2], 1.0)?;
                let (r, g, b) = hsl_to_rgb(h, s, l);

                Some(Rgba { r, g, b, a })
            }

            _ => None,
        };
    }

    NAMED.iter().find(|(n, _)| *n == lower).map(|(_, v)| Rgba {
        r: (v >> 16) as u8,
        g: (v >> 8) as u8,
        b: *v as u8,
        a: 1.0,
    })
}

/// A length as a Roblox `UDim`: a scale and an offset in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Len {
    pub scale: f64,
    pub offset: f64,
}

impl Len {
    pub fn px(v: f64) -> Self {
        Self {
            scale: 0.0,
            offset: v,
        }
    }

    pub fn udim(self) -> String {
        format!("UDim.new({}, {})", num(self.scale), num(self.offset))
    }
}

/// A number as Luau writes it: no trailing `.0`, at most three decimals.
pub fn num(v: f64) -> String {
    let r = (v * 1000.0).round() / 1000.0;

    if r == r.trunc() && r.abs() < 1e15 {
        format!("{}", r as i64)
    } else {
        format!("{r}")
    }
}

/// A CSS length: px, %, em, rem, pt, vw, vh, a bare 0, or
/// `calc(50% - 10px)`. `em` and `rem` count 16 pixels.
pub fn length(text: &str) -> Option<Len> {
    let t = text.trim().to_ascii_lowercase();

    if let Some(inner) = t.strip_prefix("calc(").and_then(|r| r.strip_suffix(')')) {
        let mut total = Len {
            scale: 0.0,
            offset: 0.0,
        };
        let mut sign = 1.0;

        for token in inner.split_whitespace() {
            match token {
                "+" => sign = 1.0,

                "-" => sign = -1.0,

                _ => {
                    let l = length(token)?;
                    total.scale += sign * l.scale;
                    total.offset += sign * l.offset;
                    sign = 1.0;
                }
            }
        }

        return Some(total);
    }

    let unit_at = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(t.len());
    let v: f64 = t[..unit_at].parse().ok()?;

    match &t[unit_at..] {
        "px" => Some(Len::px(v)),

        "" if v == 0.0 => Some(Len::px(0.0)),

        "%" | "vw" | "vh" => Some(Len {
            scale: v / 100.0,
            offset: 0.0,
        }),

        "em" | "rem" => Some(Len::px(v * 16.0)),

        "pt" => Some(Len::px(v * 4.0 / 3.0)),

        _ => None,
    }
}

/// A time in seconds: `0.3s` or `300ms`.
pub fn seconds(text: &str) -> Option<f64> {
    let t = text.trim();

    match t.strip_suffix("ms") {
        Some(ms) => ms.parse::<f64>().ok().map(|v| v / 1000.0),

        None => t.strip_suffix('s')?.parse().ok(),
    }
}

/// An angle in degrees: `45deg`, `0.5turn`, `1rad`.
pub fn degrees(text: &str) -> Option<f64> {
    let t = text.trim();

    if let Some(v) = t.strip_suffix("deg") {
        return v.parse().ok();
    }

    if let Some(v) = t.strip_suffix("turn") {
        return v.parse::<f64>().ok().map(|v| v * 360.0);
    }

    if let Some(v) = t.strip_suffix("rad") {
        return v.parse::<f64>().ok().map(f64::to_degrees);
    }

    (t == "0").then_some(0.0)
}

/// Splits a value on top-level whitespace, keeping `rgb(1 2 3)` whole.
pub fn words(text: &str) -> Vec<&str> {
    split_top(text, |c| c.is_whitespace())
}

/// Splits a value on top-level commas.
pub fn commas(text: &str) -> Vec<&str> {
    split_top(text, |c| c == ',')
}

fn split_top(text: &str, sep: impl Fn(char) -> bool) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;

    for (k, c) in text.char_indices() {
        match c {
            '(' => depth += 1,

            ')' => depth -= 1,

            c if depth == 0 && sep(c) => {
                let piece = text[start..k].trim();

                if !piece.is_empty() {
                    out.push(piece);
                }

                start = k + c.len_utf8();
            }

            _ => {}
        }
    }

    let piece = text[start..].trim();

    if !piece.is_empty() {
        out.push(piece);
    }

    out
}

/// Replaces each `var(--name, fallback)` with the file's value for it.
pub fn substitute(value: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = value.to_string();

    for _ in 0..8 {
        let Some(at) = out.find("var(") else {
            break;
        };
        let Some(close) = balanced_close(&out, at + 3) else {
            break;
        };
        let inner = &out[at + 4..close];
        let (name, fallback) = match inner.split_once(',') {
            Some((n, f)) => (n.trim(), Some(f.trim())),

            None => (inner.trim(), None),
        };
        let replacement = vars
            .get(name)
            .cloned()
            .or_else(|| fallback.map(str::to_string))
            .unwrap_or_default();
        out.replace_range(at..=close, &replacement);
    }

    out
}

fn balanced_close(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;

    for (k, c) in text[open..].char_indices() {
        match c {
            '(' => depth += 1,

            ')' => {
                depth -= 1;

                if depth == 0 {
                    return Some(open + k);
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

    #[test]
    fn reads_rules_selectors_and_variables() {
        let css = ":root { --accent: #f00; }\n.card > p:hover, #main { color: var(--accent); padding: 4px 8px !important }\n@media (max-width: 600px) { a { color: red } }\n";
        let sheet = parse_sheet(css, 100);

        assert_eq!(sheet.vars.get("--accent").map(String::as_str), Some("#f00"));
        assert_eq!(sheet.rules.len(), 2);
        let rule = &sheet.rules[1];
        assert_eq!(rule.selectors.len(), 2);
        let s = &rule.selectors[0];
        assert_eq!(s.parts.len(), 2);
        assert_eq!(s.parts[1].0, Combinator::Child);
        assert_eq!(s.parts[1].1.pseudo, vec!["hover"]);
        assert_eq!(s.specificity(), (0, 2, 1));
        assert!(rule.decls[1].important);
        assert_eq!(
            &css[rule.decls[0].name_span.0 - 100..rule.decls[0].name_span.1 - 100],
            "color"
        );
        assert_eq!(sheet.problems.len(), 1, "the @media block");
    }

    #[test]
    fn reads_colors_in_every_form() {
        let c = |t| color(t).map(|c| (c.r, c.g, c.b, (c.a * 100.0).round() as u32));

        assert_eq!(c("#f00"), Some((255, 0, 0, 100)));
        assert_eq!(c("#00ff0080"), Some((0, 255, 0, 50)));
        assert_eq!(c("rgb(1, 2, 3)"), Some((1, 2, 3, 100)));
        assert_eq!(c("rgb(1 2 3 / 50%)"), Some((1, 2, 3, 50)));
        assert_eq!(c("hsl(120, 100%, 50%)"), Some((0, 255, 0, 100)));
        assert_eq!(c("RebeccaPurple"), Some((102, 51, 153, 100)));
        assert_eq!(c("transparent").map(|c| c.3), Some(0));
        assert_eq!(c("nope"), None);
    }

    #[test]
    fn reads_lengths_and_calc() {
        assert_eq!(length("12px"), Some(Len::px(12.0)));
        assert_eq!(
            length("50%"),
            Some(Len {
                scale: 0.5,
                offset: 0.0
            })
        );
        assert_eq!(length("1.5rem"), Some(Len::px(24.0)));
        assert_eq!(
            length("calc(100% - 20px)"),
            Some(Len {
                scale: 1.0,
                offset: -20.0
            })
        );
        assert_eq!(length("auto"), None);
        assert_eq!(num(0.5), "0.5");
        assert_eq!(num(2.0), "2");
    }

    #[test]
    fn reads_a_react_style_table() {
        let src = "{ backgroundColor = \"#fff\", padding = 8, opacity = 0.5, [\"--gap\"] = \"4px\", width = w }";
        let (decls, problems) = parse_table(src, 10);
        let pairs: Vec<(&str, &str)> = decls
            .iter()
            .map(|d| (d.name.as_str(), d.value.as_str()))
            .collect();

        assert_eq!(
            pairs,
            vec![
                ("background-color", "#fff"),
                ("padding", "8px"),
                ("opacity", "0.5"),
                ("--gap", "4px")
            ]
        );
        assert_eq!(
            &src[decls[0].name_span.0 - 10..decls[0].name_span.1 - 10],
            "backgroundColor"
        );
        assert_eq!(
            &src[decls[0].value_span.0 - 10..decls[0].value_span.1 - 10],
            "#fff"
        );
        assert_eq!(problems.len(), 1);
        assert_eq!(camel("background-color"), "backgroundColor");
        assert_eq!(kebab("WebkitTextStroke"), "-webkit-text-stroke");
    }

    #[test]
    fn substitutes_variables_with_fallbacks() {
        let mut vars = BTreeMap::new();
        vars.insert("--gap".to_string(), "8px".to_string());

        assert_eq!(substitute("var(--gap) var(--none, 2px)", &vars), "8px 2px");
    }
}
