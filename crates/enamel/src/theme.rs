//! The project's `enamel.aly`: fonts, colors, and classes of its own,
//! written as Alloy with the Roblox API. Enamel never runs it. Each entry
//! is the text of an expression, copied into the generated attribute
//! the way a macro would; the statements before the `return` are the
//! prelude, copied once into a file that uses one of the entries.
//!
//! ```alloy
//! local purple = Color3.fromRGB(138, 61, 245)
//!
//! return {
//!     colors = { brand = purple, accent = "#ff8a00" },
//!     fonts = { title = Font.new("rbxasset://fonts/families/Montserrat.json", Enum.FontWeight.Bold) },
//!     classes = {
//!         card = "bg-slate-900/80 rounded-xl p-4",
//!         glow = { BackgroundColor3 = purple, ZIndex = 2 },
//!     },
//! }
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The file name, at the project root.
pub const FILE_NAME: &str = "enamel.aly";

/// One entry of a theme table: the expression text and its span in the
/// file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub expr: String,
    pub span: (usize, usize),
}

/// A class of the theme: a class list to expand, or properties to set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassDef {
    Classes(String),
    Props(Vec<(String, String)>),
}

/// A problem in the theme file, with its span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub span: (usize, usize),
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Theme {
    /// The statements before the `return`, as one line.
    pub prelude: String,
    pub colors: BTreeMap<String, Entry>,
    pub fonts: BTreeMap<String, Entry>,
    pub classes: BTreeMap<String, (ClassDef, (usize, usize))>,
    pub problems: Vec<Problem>,
}

impl Theme {
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty() && self.fonts.is_empty() && self.classes.is_empty()
    }
}

/// The theme file, read again when its modification time changes.
#[derive(Debug, Default)]
pub struct Watched {
    path: Option<PathBuf>,
    modified: Option<SystemTime>,
    pub theme: Theme,
}

impl Watched {
    /// The file's path, when a root is known.
    pub fn path(&self) -> Option<PathBuf> {
        self.path.clone()
    }

    pub fn at(root: &Path) -> Self {
        let mut w = Self {
            path: Some(root.join(FILE_NAME)),
            modified: None,
            theme: Theme::default(),
        };
        w.refresh();

        w
    }

    /// Reads the file when it changed; true when the theme is new.
    pub fn refresh(&mut self) -> bool {
        let Some(path) = &self.path else {
            return false;
        };
        let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();

        if modified == self.modified && (modified.is_some() || self.theme.is_empty()) {
            return false;
        }

        self.modified = modified;
        self.theme = match std::fs::read_to_string(path) {
            Ok(text) => parse(&text),

            Err(_) => Theme::default(),
        };

        true
    }
}

/// Parses the theme file's text.
pub fn parse(text: &str) -> Theme {
    let mut theme = Theme::default();
    let bytes = text.as_bytes();
    let Some(ret) = top_level_return(text) else {
        theme.problems.push(Problem {
            span: (0, 0),
            message: "enamel.aly must end in `return { ... }` with `colors`, `fonts`, and `classes` tables".into(),
        });

        return theme;
    };
    theme.prelude = flatten(&text[..ret]);
    let after = ret + "return".len();
    let Some(open) = text[after..].find('{').map(|i| after + i) else {
        theme.problems.push(Problem {
            span: (ret, after),
            message: "`return` must be followed by a table".into(),
        });

        return theme;
    };
    let close = match group_end(bytes, open) {
        Some(c) => c,

        None => {
            theme.problems.push(Problem {
                span: (open, open + 1),
                message: "the returned table never closes".into(),
            });

            return theme;
        }
    };

    for (key, key_span, value, value_span) in fields(text, open + 1, close) {
        let Some(vopen) = value.strip_prefix('{').map(|_| value_span.0) else {
            theme.problems.push(Problem {
                span: key_span,
                message: format!("`{key}` must be a table"),
            });

            continue;
        };
        let vclose = value_span.1 - 1;

        match key.as_str() {
            "colors" | "fonts" => {
                let map = if key == "colors" {
                    &mut theme.colors
                } else {
                    &mut theme.fonts
                };

                for (name, name_span, expr, span) in fields(text, vopen + 1, vclose) {
                    if !valid_name(&name) {
                        theme.problems.push(Problem {
                            span: name_span,
                            message: format!(
                                "`{name}` is not a class name: letters, digits, `-`, and `_`"
                            ),
                        });

                        continue;
                    }

                    map.insert(name, Entry { expr, span });
                }
            }

            "classes" => {
                for (name, name_span, expr, span) in fields(text, vopen + 1, vclose) {
                    if !valid_name(&name) {
                        theme.problems.push(Problem {
                            span: name_span,
                            message: format!(
                                "`{name}` is not a class name: letters, digits, `-`, and `_`"
                            ),
                        });

                        continue;
                    }

                    let def = if let Some(inner) = string_literal(&expr) {
                        ClassDef::Classes(inner)
                    } else if expr.starts_with('{') {
                        let props = fields(text, span.0 + 1, span.1 - 1)
                            .into_iter()
                            .map(|(k, _, v, _)| (k, v))
                            .collect();

                        ClassDef::Props(props)
                    } else {
                        theme.problems.push(Problem {
                            span,
                            message: format!(
                                "`{name}` must be a string of classes or a table of properties"
                            ),
                        });

                        continue;
                    };
                    theme.classes.insert(name, (def, name_span));
                }
            }

            _ => theme.problems.push(Problem {
                span: key_span,
                message: format!(
                    "`{key}` is not a theme table; the tables are `colors`, `fonts`, and `classes`"
                ),
            }),
        }
    }

    theme
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The inside of a plain string literal, `"..."` or `'...'`.
fn string_literal(expr: &str) -> Option<String> {
    let q = expr.chars().next()?;

    if (q == '"' || q == '\'') && expr.ends_with(q) && expr.len() >= 2 {
        return Some(expr[1..expr.len() - 1].to_string());
    }

    None
}

/// The byte of the top-level `return` keyword.
fn top_level_return(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'-' if text[i..].starts_with("--") => {
                i = skip_comment(text, i);

                continue;
            }
            b'"' | b'\'' | b'`' => {
                i = skip_string(bytes, i);

                continue;
            }
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            b'r' if depth == 0
                && text[i..].starts_with("return")
                && (i == 0 || !is_word(bytes[i - 1]))
                && !bytes.get(i + 6).is_some_and(|b| is_word(*b)) =>
            {
                return Some(i);
            }
            _ => {}
        }

        i += 1;
    }

    None
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// One past the end of a `--` comment, line or block.
fn skip_comment(text: &str, at: usize) -> usize {
    let rest = &text[at + 2..];

    if let Some(body) = rest.strip_prefix("[[") {
        return body.find("]]").map_or(text.len(), |e| at + 4 + e + 2);
    }

    rest.find('\n').map_or(text.len(), |e| at + 2 + e + 1)
}

/// One past the end of the string that starts at `at`.
fn skip_string(bytes: &[u8], at: usize) -> usize {
    let q = bytes[at];
    let mut i = at + 1;

    while i < bytes.len() && bytes[i] != q {
        if bytes[i] == b'\\' {
            i += 1;
        }

        i += 1;
    }

    (i + 1).min(bytes.len())
}

/// The `}` that closes the `{` at `open`.
fn group_end(bytes: &[u8], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = open;

    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' | b'`' => {
                i = skip_string(bytes, i);

                continue;
            }
            b'-' if bytes.get(i + 1) == Some(&b'-') => {
                let text = std::str::from_utf8(bytes).unwrap_or("");
                i = skip_comment(text, i);

                continue;
            }
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => {
                depth -= 1;

                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }

        i += 1;
    }

    None
}

/// One `key = value` field: the key, its span, the value text, and the
/// value's span.
pub type Field = (String, (usize, usize), String, (usize, usize));

/// The `key = value` fields between two bytes, at depth zero.
fn fields(text: &str, from: usize, to: usize) -> Vec<Field> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = from;

    loop {
        // Skip whitespace, commas, and comments.
        loop {
            while i < to && (bytes[i].is_ascii_whitespace() || bytes[i] == b',' || bytes[i] == b';')
            {
                i += 1;
            }

            if i < to && text[i..].starts_with("--") {
                i = skip_comment(text, i);

                continue;
            }

            break;
        }

        if i >= to {
            break;
        }

        // The key: a name, or `["quoted"]`.
        let key_start = i;
        let key = if bytes[i] == b'[' {
            let end = skip_string(bytes, i + 1);
            let k = string_literal(&text[i + 1..end]).unwrap_or_default();
            i = end;

            while i < to && bytes[i] != b']' {
                i += 1;
            }

            i += 1;

            k
        } else {
            while i < to && is_word(bytes[i]) {
                i += 1;
            }

            text[key_start..i].to_string()
        };
        let key_span = (key_start, i);

        while i < to && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        if i >= to || bytes[i] != b'=' || key.is_empty() {
            // Not a `key = value` field: skip to the next comma at depth
            // zero.
            i = value_end(text, i.max(key_start + 1), to);

            continue;
        }

        i += 1;

        while i < to && bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        let value_start = i;
        let end = value_end(text, i, to);
        let value = text[value_start..end].trim_end().to_string();
        let value_span = (value_start, value_start + value.len());
        out.push((key, key_span, value, value_span));
        i = end;
    }

    out
}

/// One past the end of a value that starts at `from`: the next comma at
/// depth zero, or `to`.
fn value_end(text: &str, from: usize, to: usize) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut i = from;

    while i < to {
        match bytes[i] {
            b'"' | b'\'' | b'`' => {
                i = skip_string(bytes, i);

                continue;
            }
            b'-' if text[i..].starts_with("--") => {
                i = skip_comment(text, i);

                continue;
            }
            b'{' | b'(' | b'[' => depth += 1,
            b'}' | b')' | b']' => depth -= 1,
            b',' | b';' if depth == 0 => return i,
            _ => {}
        }

        i += 1;
    }

    to
}

/// The prelude as one line: comments out, lines joined by a space.
fn flatten(text: &str) -> String {
    let mut out = String::new();
    let mut i = 0;
    let bytes = text.as_bytes();

    while i < bytes.len() {
        if text[i..].starts_with("--") {
            i = skip_comment(text, i);
            out.push(' ');

            continue;
        }

        if matches!(bytes[i], b'"' | b'\'' | b'`') {
            let end = skip_string(bytes, i);
            out.push_str(&text[i..end]);
            i = end;

            continue;
        }

        let c = text[i..].chars().next().unwrap_or(' ');
        out.push(if c == '\n' { ' ' } else { c });
        i += c.len_utf8();
    }

    let words: Vec<&str> = out.split_whitespace().collect();

    words.join(" ")
}

/// The color a theme expression names, when it is one the reader can
/// see: `Color3.fromRGB(r, g, b)`, `Color3.new(r, g, b)`,
/// `Color3.fromHex("#rrggbb")`, or a `"#rrggbb"` string.
pub fn color_of(expr: &str) -> Option<(u8, u8, u8)> {
    let expr = expr.trim();

    if let Some(inner) = string_literal(expr) {
        return crate::classes::hex(&inner);
    }

    if let Some(args) = expr
        .strip_prefix("Color3.fromHex(")
        .and_then(|r| r.strip_suffix(')'))
    {
        return string_literal(args.trim()).and_then(|h| crate::classes::hex(&h));
    }

    let (head, scale) = match (
        expr.strip_prefix("Color3.fromRGB("),
        expr.strip_prefix("Color3.new("),
    ) {
        (Some(a), _) => (a, 1.0),
        (None, Some(a)) => (a, 255.0),
        (None, None) => return None,
    };
    let args = head.strip_suffix(')')?;
    let parts: Vec<f64> = args
        .split(',')
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;

    if parts.len() != 3 {
        return None;
    }

    let c = |v: f64| (v * scale).round().clamp(0.0, 255.0) as u8;

    Some((c(parts[0]), c(parts[1]), c(parts[2])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_parses_its_tables_and_prelude() {
        let src = "-- the brand\nlocal purple = Color3.fromRGB(138, 61, 245)\n\nreturn {\n    colors = { brand = purple, accent = \"#ff8a00\" },\n    fonts = { title = Font.new(\"rbxasset://fonts/families/Montserrat.json\", Enum.FontWeight.Bold) },\n    classes = {\n        card = \"bg-slate-900/80 rounded-xl p-4\",\n        glow = { BackgroundColor3 = purple, ZIndex = 2 },\n    },\n}\n";
        let t = parse(src);
        assert_eq!(t.prelude, "local purple = Color3.fromRGB(138, 61, 245)");
        assert_eq!(t.colors["brand"].expr, "purple");
        assert_eq!(color_of(&t.colors["accent"].expr), Some((255, 138, 0)));
        assert!(t.fonts["title"].expr.starts_with("Font.new("));
        assert_eq!(
            t.classes["card"].0,
            ClassDef::Classes("bg-slate-900/80 rounded-xl p-4".into())
        );
        assert_eq!(
            t.classes["glow"].0,
            ClassDef::Props(vec![
                ("BackgroundColor3".into(), "purple".into()),
                ("ZIndex".into(), "2".into())
            ])
        );
        assert!(t.problems.is_empty());
    }

    #[test]
    fn a_stray_table_is_a_problem() {
        let t = parse("return { spacing = { x = 1 } }\n");
        assert_eq!(t.problems.len(), 1);
        assert!(t.problems[0].message.contains("`spacing`"));
        assert_eq!(color_of("Color3.new(1, 0.5, 0)"), Some((255, 128, 0)));
    }
}
