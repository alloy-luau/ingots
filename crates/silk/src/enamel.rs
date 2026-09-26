//! What an Enamel class sets. Silk runs before Enamel and writes the
//! defaults a browser gives each tag. Where a class of the element sets
//! the same property, Silk leaves its default out, so the element has
//! one value and not two.
//!
//! The names follow Enamel's utilities. A key is a Roblox property, a
//! modifier class Enamel adds (`UIPadding`), or one of the keys below.

use std::collections::{HashMap, HashSet};

use crate::markup;

/// A class that lays the children out in a row: `flex`, `flex-row`.
pub const ROW: &str = "layout:row";
/// `flex-col`.
pub const COLUMN: &str = "layout:column";
/// `grid`, `grid-cols-3`.
pub const GRID: &str = "layout:grid";
/// A class that arranges the children in the layout: `gap-2`,
/// `justify-between`, `items-center`.
pub const ARRANGE: &str = "layout:arrange";
/// A class that sets the width of `Size`: `w-full`. Enamel sets it
/// inside the `Size` the tag writes, so Silk's height stays.
pub const SIZE_X: &str = "size:x";
pub const SIZE_Y: &str = "size:y";
/// `absolute` and `fixed`: the element places itself, out of the flow.
pub const PLACED: &str = "position:placed";
/// `w-auto`: the X axis of `AutomaticSize`, which starts at 0.
pub const AUTO_X: &str = "auto:x";
pub const AUTO_Y: &str = "auto:y";

/// A class of the project's `enamel.aly`: the utilities it stands for,
/// or the properties it sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeClass {
    Utilities(Vec<String>),
    Props(Vec<String>),
}

/// The classes of `enamel.aly`, by name.
pub type Theme = HashMap<String, ThemeClass>;

/// Every key the classes of one element set. A class with a state
/// variant, `hover:bg-red-500`, applies on that state alone, so the
/// element keeps the default for its resting look. A class of the theme
/// sets what its utilities or its properties set.
pub fn sets<'a>(classes: impl IntoIterator<Item = &'a str>, theme: &Theme) -> HashSet<String> {
    let mut out = HashSet::new();

    for class in classes {
        add(class, theme, 0, &mut out);
    }

    out
}

fn add(class: &str, theme: &Theme, depth: usize, out: &mut HashSet<String>) {
    if has_variant(class) {
        return;
    }

    let base = class
        .strip_prefix('-')
        .filter(|b| !b.is_empty())
        .unwrap_or(class);

    // Enamel reads a theme class first, and stops at eight levels.
    match theme.get(base) {
        Some(ThemeClass::Utilities(list)) if depth < 8 => {
            for c in list {
                add(c, theme, depth + 1, out);
            }
        }

        Some(ThemeClass::Props(keys)) => out.extend(keys.iter().cloned()),

        _ => out.extend(utility(base).iter().map(|k| (*k).to_string())),
    }
}

/// The classes of the text of an `enamel.aly`: the `classes` table it
/// exports or returns. A string holds utilities, and a table holds
/// properties.
pub fn theme(text: &str) -> Theme {
    let mut out = Theme::new();
    let Some(open) = classes_table(text) else {
        return out;
    };
    let Some(close) = markup::skip_hole(text, open) else {
        return out;
    };

    for (name, (s, e)) in fields(text, open + 1, close - 1) {
        let v = text[s..e].trim();
        let at = s + text[s..e].len() - text[s..e].trim_start().len();
        let quoted =
            v.len() >= 2 && (v.starts_with('"') || v.starts_with('\'')) && v.ends_with(&v[..1]);

        let class = if quoted {
            ThemeClass::Utilities(
                v[1..v.len() - 1]
                    .split_whitespace()
                    .map(String::from)
                    .collect(),
            )
        } else if v.starts_with('{') {
            let keys = fields(text, at + 1, at + v.len() - 1);

            ThemeClass::Props(keys.into_iter().map(|(k, _)| k).collect())
        } else {
            continue;
        };
        out.insert(name, class);
    }

    out
}

/// The `{` of the `classes` table: `export const classes = {`, or
/// `classes = {` inside the table the file returns.
fn classes_table(text: &str) -> Option<usize> {
    let b = text.as_bytes();

    text.match_indices("classes").find_map(|(at, word)| {
        let bounded = at.checked_sub(1).is_none_or(|p| !is_word(b[p]))
            && !b.get(at + word.len()).is_some_and(|c| is_word(*c));
        let rest = text[at + word.len()..].trim_start().strip_prefix('=')?;
        let open = text.len() - rest.trim_start().len();

        (bounded && b.get(open) == Some(&b'{') && markup::in_code(text, at)).then_some(open)
    })
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The `key = value` fields of a table between `from` and `to`. A key is
/// a name or `["quoted"]`; the value runs to the next `,` or `;` outside
/// any bracket, string, or comment.
fn fields(text: &str, from: usize, to: usize) -> Vec<(String, (usize, usize))> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = from;

    while i < to {
        match b[i] {
            c if c.is_ascii_whitespace() || c == b',' || c == b';' => i += 1,

            b'-' if b.get(i + 1) == Some(&b'-') => i = markup::skip_comment(text, i),

            _ => {
                let key_start = i;
                let key = match b[i] {
                    b'[' => {
                        let close = text[i..to].find(']').map_or(to, |n| i + n);
                        let key = text[i + 1..close]
                            .trim()
                            .trim_matches(|c| c == '"' || c == '\'');
                        i = close + 1;

                        key.to_string()
                    }

                    _ => {
                        while i < to && is_word(b[i]) {
                            i += 1;
                        }

                        text[key_start..i].to_string()
                    }
                };
                let after = text[i..to].trim_start();
                let value_start = to - after.len();
                let end = value_end(text, value_start.max(key_start + 1), to);

                if let Some(value) = after.strip_prefix('=').filter(|_| !key.is_empty()) {
                    out.push((key, (to - value.len(), end)));
                }

                i = end;
            }
        }
    }

    out
}

/// The end of a field's value: the next `,` or `;` outside brackets,
/// strings, and comments, or `to`.
fn value_end(text: &str, mut i: usize, to: usize) -> usize {
    let b = text.as_bytes();
    let mut depth = 0usize;

    while i < to {
        i = match b[i] {
            b',' | b';' if depth == 0 => return i,

            b'"' | b'\'' => markup::skip_quoted(text, i),

            b'-' if b.get(i + 1) == Some(&b'-') => markup::skip_comment(text, i),

            b'[' if markup::long_open(text, i).is_some() => markup::skip_long(text, i),

            b'{' | b'(' | b'[' => {
                depth += 1;
                i + 1
            }

            b'}' | b')' | b']' => {
                depth = depth.saturating_sub(1);
                i + 1
            }

            _ => i + 1,
        };
    }

    to
}

/// Whether a class carries a variant: a colon before any `[`, as in
/// `hover:bg-red-500` and not `image-[rbxassetid://1]`.
fn has_variant(class: &str) -> bool {
    class
        .find(':')
        .is_some_and(|colon| class.find('[').is_none_or(|b| colon < b))
}

/// The keys one utility sets, by its name without variants and sign.
fn utility(base: &str) -> &'static [&'static str] {
    let exact: &'static [&'static str] = match base {
        "hidden" | "visible" | "block" | "inline" | "inline-block" => &["Visible"],
        "flex" | "inline-flex" | "flex-row" => &[ROW],
        "flex-col" => &[COLUMN],
        "grid" => &[GRID],
        "flex-wrap" | "flex-nowrap" => &[ARRANGE],
        "grow" | "grow-0" | "shrink" | "fill" | "flex-1" | "flex-auto" | "flex-none" => {
            &["UIFlexItem"]
        }
        "inset-0" => &["Position", SIZE_X, SIZE_Y],
        "inset-x-0" => &["Position", SIZE_X],
        "inset-y-0" => &["Position", SIZE_Y],
        "overflow-hidden" | "overflow-clip" | "overflow-visible" | "clip" | "no-clip" => {
            &["ClipsDescendants"]
        }
        "overflow-auto" | "overflow-scroll" | "overflow-y-auto" | "overflow-y-scroll"
        | "no-scroll" => &["ScrollingEnabled"],
        "scroll-x" | "scroll-y" | "scroll-xy" => &["ScrollingDirection"],
        "truncate" | "text-ellipsis" | "text-clip" => &["TextTruncate"],
        "text-wrap" | "text-nowrap" | "whitespace-normal" | "whitespace-nowrap" => &["TextWrapped"],
        "text-scaled" => &["TextScaled"],
        "text-left" | "text-center" | "text-right" | "text-justify" => &["TextXAlignment"],
        "align-top" | "align-middle" | "align-bottom" => &["TextYAlignment"],
        "rich" | "rich-text" => &["RichText"],
        "italic" | "not-italic" => &["FontFace"],
        "bg-transparent" => &["BackgroundTransparency"],
        "text-transparent" => &["TextTransparency"],
        "border" | "ring" | "stroke" => &["UIStroke"],
        "rounded" => &["UICorner"],
        "auto-color" | "no-auto-color" => &["AutoButtonColor"],
        "order-first" | "order-last" | "order-none" => &["LayoutOrder"],
        "w-auto" | "w-fit" | "w-max" | "w-min" => &[AUTO_X],
        "h-auto" | "h-fit" | "h-max" | "h-min" => &[AUTO_Y],
        "size-auto" | "size-fit" => &[AUTO_X, AUTO_Y],
        "center" => &["Position", "AnchorPoint"],
        "absolute" | "fixed" => &[PLACED],
        _ => &[],
    };

    if !exact.is_empty() {
        return exact;
    }

    let Some((head, rest)) = base.split_once('-') else {
        return &[];
    };

    match head {
        "bg" if rest.starts_with("gradient-to-") => &["UIGradient"],
        "bg" if rest.starts_with("opacity-") || rest.starts_with("transparency-") => {
            &["BackgroundTransparency"]
        }
        // A color: the box shows it, so Silk's clear background goes too.
        "bg" if rest != "none" => &["BackgroundColor3", "BackgroundTransparency"],
        "text" => text(rest),
        "placeholder" => &["PlaceholderColor3"],
        "font" => &["FontFace"],
        "leading" => &["LineHeight"],
        "border" | "ring" | "outline" | "stroke" if !rest.starts_with("offset") => &["UIStroke"],
        "rounded" => &["UICorner"],
        "p" | "px" | "py" | "pt" | "pr" | "pb" | "pl" => &["UIPadding"],
        "gap" | "justify" | "items" | "sort" | "auto" => &[ARRANGE],
        "grid" => &[GRID],
        "self" => &["UIFlexItem"],
        "w" => &[SIZE_X],
        "h" => &[SIZE_Y],
        "size" => &[SIZE_X, SIZE_Y],
        "max" if rest.starts_with("graphemes") => &["MaxVisibleGraphemes"],
        "min" | "max" => &["UISizeConstraint"],
        "aspect" => &["UIAspectRatioConstraint"],
        "scale" => &["UIScale"],
        "from" | "via" | "to" => &["UIGradient"],
        "top" | "bottom" | "left" | "right" => &["Position", "AnchorPoint"],
        "anchor" | "translate" => &["AnchorPoint"],
        "z" => &["ZIndex"],
        "order" | "layout" => &["LayoutOrder"],
        "opacity" | "transparency" => &[
            "BackgroundTransparency",
            "TextTransparency",
            "ImageTransparency",
        ],
        "rotate" => &["Rotation"],
        "scrollbar" => &["ScrollBarThickness"],
        "canvas" if rest.starts_with("auto") => &["AutomaticCanvasSize"],
        "canvas" => &["CanvasSize"],
        _ => &[],
    }
}

/// The keys of a `text-` utility: a size, a transparency, an outline,
/// or a color. The alignments and the wraps are exact words above.
fn text(rest: &str) -> &'static [&'static str] {
    const SIZES: &[&str] = &[
        "xs", "sm", "base", "lg", "xl", "2xl", "3xl", "4xl", "5xl", "6xl", "7xl", "8xl", "9xl",
    ];
    let length = rest
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .is_some_and(|v| {
            let n = v.strip_suffix("px").or(v.strip_suffix("rem")).unwrap_or(v);

            n.parse::<f64>().is_ok()
        });

    if SIZES.contains(&rest) || rest.starts_with("size-") || length {
        &["TextSize"]
    } else if rest.starts_with("opacity-") || rest.starts_with("transparency-") {
        &["TextTransparency"]
    } else if rest.starts_with("stroke") {
        &["TextStrokeColor3", "TextStrokeTransparency"]
    } else if rest.contains('/') {
        &["TextColor3", "TextTransparency"]
    } else {
        &["TextColor3"]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(classes: &str) -> Vec<String> {
        let mut out: Vec<String> = sets(classes.split_whitespace(), &Theme::new())
            .into_iter()
            .collect();
        out.sort();

        out
    }

    #[test]
    fn a_class_sets_only_its_own_property() {
        assert_eq!(keys("text-white"), ["TextColor3"]);
        assert_eq!(keys("text-xl text-[20px]"), ["TextSize"]);
        assert_eq!(keys("text-center"), ["TextXAlignment"]);
        assert_eq!(keys("text-white/50"), ["TextColor3", "TextTransparency"]);
        assert_eq!(
            keys("bg-gradient-to-b from-accent to-orange-500"),
            ["UIGradient"]
        );
        assert_eq!(
            keys("bg-red-500"),
            ["BackgroundColor3", "BackgroundTransparency"]
        );
        assert_eq!(keys("hover:bg-red-500 -z-10"), ["ZIndex"]);
        assert_eq!(keys("flex-1"), ["UIFlexItem"]);
        assert_eq!(keys("w-full h-auto"), [AUTO_Y, SIZE_X]);
        assert_eq!(keys("stroke stroke-white/15 ring-offset-2"), ["UIStroke"]);
        assert!(keys("card primary").is_empty());
    }

    /// Game UI 21: a class of the theme sets what its utilities set.
    #[test]
    fn a_theme_class_sets_what_its_utilities_set() {
        let text = "-- The theme.\nlocal gold = Color3.fromRGB(255, 214, 92)\n\nexport const classes = {\n  -- A panel, with a comma, in a comment.\n  panel = 'bg-glass/75 rounded-2xl stroke stroke-white/15',\n  glow = { BackgroundColor3 = Color3.fromRGB(1, 2, 3), ZIndex = 2 },\n  [\"deep-panel\"] = \"panel p-4\";\n}\n";
        let theme = theme(text);

        assert_eq!(
            theme.get("glow"),
            Some(&ThemeClass::Props(vec![
                "BackgroundColor3".into(),
                "ZIndex".into()
            ]))
        );

        let mut keys: Vec<String> = sets(["deep-panel"], &theme).into_iter().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "BackgroundColor3",
                "BackgroundTransparency",
                "UICorner",
                "UIPadding",
                "UIStroke"
            ]
        );

        let returned = theme_of_return();
        assert!(returned.contains_key("card"), "{returned:?}");
    }

    fn theme_of_return() -> Theme {
        theme("local classes = { card = 'p-4' }\nreturn { colors = {}, classes = classes }\n")
    }
}
