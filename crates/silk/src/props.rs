//! CSS declarations as Roblox properties. A declaration sets a property
//! of the element, a property of a modifier child (`UICorner`,
//! `UIPadding`, `UIStroke`, and the rest), or a property of the layout
//! that orders the element's children.

use std::collections::BTreeMap;

use crate::css::{self, Decl, Len, Problem, Rgba, num};

/// What kind of element a declaration lands on. It decides between
/// `TextColor3` and `ImageColor3`, for one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Container,
    Text,
    Input,
    Image,
    Video,
    Canvas,
    /// A stylesheet rule whose selector names no element kind.
    Any,
}

impl Target {
    fn has_text(self) -> bool {
        matches!(self, Self::Text | Self::Input | Self::Any)
    }

    fn has_image(self) -> bool {
        matches!(self, Self::Image | Self::Any)
    }
}

/// The font families the generic CSS names stand for.
#[derive(Debug, Clone)]
pub struct Fonts {
    pub sans: String,
    pub serif: String,
    pub mono: String,
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            sans: "rbxasset://fonts/families/SourceSansPro.json".into(),
            serif: "rbxasset://fonts/families/Merriweather.json".into(),
            mono: "rbxasset://fonts/families/RobotoMono.json".into(),
        }
    }
}

pub struct Ctx<'a> {
    pub vars: &'a BTreeMap<String, String>,
    pub fonts: &'a Fonts,
    /// Whether the project allows Roblox names.
    pub roblox: bool,
    pub target: Target,
}

/// One axis of `width` or `height`: a length, or `auto`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Axis {
    Len(Len),
    Auto,
}

/// The parts of a `FontFace`; unset parts keep the element's own.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FontParts {
    pub family: Option<String>,
    pub weight: Option<&'static str>,
    pub italic: Option<bool>,
}

impl FontParts {
    pub fn is_set(&self) -> bool {
        self.family.is_some() || self.weight.is_some() || self.italic.is_some()
    }

    /// The `Font.new` call, with `base` for the parts this leaves unset.
    pub fn luau(&self, base: &FontParts, fonts: &Fonts) -> String {
        let family = self
            .family
            .clone()
            .or_else(|| base.family.clone())
            .unwrap_or_else(|| fonts.sans.clone());
        let weight = self.weight.or(base.weight).unwrap_or("Regular");
        let style = match self.italic.or(base.italic).unwrap_or(false) {
            true => "Italic",

            false => "Normal",
        };

        format!("Font.new(\"{family}\", Enum.FontWeight.{weight}, Enum.FontStyle.{style})")
    }
}

/// The layout the children take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// `display: block` and the default: children stack down the element.
    Flow,
    /// `display: flex`.
    Flex,
    Grid,
    /// `position: absolute` children or `display: none`: no layout.
    None,
}

/// What a declaration list sets.
#[derive(Debug, Clone, Default)]
pub struct Out {
    pub props: Vec<(String, String)>,
    /// Modifier children by class, each with its properties.
    pub mods: Vec<(&'static str, Vec<(String, String)>)>,
    /// Properties of the `UIListLayout` or `UIGridLayout` child.
    pub layout: Vec<(String, String)>,
    pub layout_kind: Option<Layout>,
    pub width: Option<Axis>,
    pub height: Option<Axis>,
    pub font: FontParts,
    pub background: Option<Rgba>,
    pub opacity: Option<f64>,
    /// `overflow: auto` or `scroll`: the element becomes a ScrollingFrame.
    pub scroll: Option<&'static str>,
    /// `appearance: none`: a control drops the look a browser gives it.
    pub plain: bool,
    /// RichText tags to wrap the text in: `text-decoration` and
    /// `text-transform`.
    pub rich: Vec<(&'static str, &'static str)>,
    pub problems: Vec<Problem>,
}

impl Out {
    pub fn set(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();

        match self.props.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value,

            None => self.props.push((key.to_string(), value)),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.props
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn modifier(&mut self, class: &'static str, key: &str, value: impl Into<String>) {
        let value = value.into();
        let slot = match self.mods.iter().position(|(c, _)| *c == class) {
            Some(i) => &mut self.mods[i].1,

            None => {
                self.mods.push((class, Vec::new()));
                &mut self.mods.last_mut().expect("just pushed").1
            }
        };

        match slot.iter_mut().find(|(k, _)| k == key) {
            Some(s) => s.1 = value,

            None => slot.push((key.to_string(), value)),
        }
    }

    pub(crate) fn layout(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();

        match self.layout.iter_mut().find(|(k, _)| k == key) {
            Some(s) => s.1 = value,

            None => self.layout.push((key.to_string(), value)),
        }
    }

    fn problem(&mut self, span: (usize, usize), lint: &'static str, message: impl Into<String>) {
        self.problems.push(Problem {
            span,
            lint,
            message: message.into(),
        });
    }

    /// The `Size` and `AutomaticSize` the axes give, over `base` for an
    /// axis this leaves unset.
    pub fn size(&self, base: (Axis, Axis)) -> Option<(String, String)> {
        if self.width.is_none() && self.height.is_none() {
            return None;
        }

        let w = self.width.unwrap_or(base.0);
        let h = self.height.unwrap_or(base.1);

        Some(size_props(w, h))
    }
}

/// `Size` and `AutomaticSize` for two axes.
pub fn size_props(w: Axis, h: Axis) -> (String, String) {
    let len = |a: Axis| match a {
        Axis::Len(l) => l,

        Axis::Auto => Len::px(0.0),
    };
    let (wl, hl) = (len(w), len(h));
    let size = udim2(wl, hl);
    let auto = match (w, h) {
        (Axis::Auto, Axis::Auto) => "XY",

        (Axis::Auto, _) => "X",

        (_, Axis::Auto) => "Y",

        _ => "None",
    };

    (size, format!("Enum.AutomaticSize.{auto}"))
}

pub fn udim2(x: Len, y: Len) -> String {
    match (x.scale, y.scale, x.offset, y.offset) {
        (0.0, 0.0, 0.0, 0.0) => "UDim2.new()".into(),

        (0.0, 0.0, ..) => format!("UDim2.fromOffset({}, {})", num(x.offset), num(y.offset)),

        (_, _, 0.0, 0.0) => format!("UDim2.fromScale({}, {})", num(x.scale), num(y.scale)),

        _ => format!(
            "UDim2.new({}, {}, {}, {})",
            num(x.scale),
            num(x.offset),
            num(y.scale),
            num(y.offset)
        ),
    }
}

fn transparency(a: f64) -> String {
    num(1.0 - a)
}

/// 1 to 4 lengths as top, right, bottom, left.
fn four(value: &str) -> Option<[Len; 4]> {
    let v: Vec<Len> = css::words(value)
        .iter()
        .map(|w| css::length(w))
        .collect::<Option<_>>()?;

    Some(match v.len() {
        1 => [v[0], v[0], v[0], v[0]],

        2 => [v[0], v[1], v[0], v[1]],

        3 => [v[0], v[1], v[2], v[1]],

        4 => [v[0], v[1], v[2], v[3]],

        _ => return None,
    })
}

const WEIGHTS: &[(&str, &str)] = &[
    ("100", "Thin"),
    ("200", "ExtraLight"),
    ("300", "Light"),
    ("400", "Regular"),
    ("normal", "Regular"),
    ("500", "Medium"),
    ("600", "SemiBold"),
    ("700", "Bold"),
    ("bold", "Bold"),
    ("bolder", "ExtraBold"),
    ("lighter", "Light"),
    ("800", "ExtraBold"),
    ("900", "Heavy"),
];

pub fn weight(value: &str) -> Option<&'static str> {
    WEIGHTS
        .iter()
        .find(|(k, _)| *k == value.trim())
        .map(|(_, v)| *v)
}

/// The Roblox font family a CSS family list names: its first entry.
pub fn family(value: &str, fonts: &Fonts) -> String {
    let first = css::commas(value)
        .into_iter()
        .next()
        .unwrap_or("sans-serif");
    let name = first.trim_matches(|c| c == '"' || c == '\'');

    match name.to_ascii_lowercase().as_str() {
        "sans-serif" | "system-ui" | "ui-sans-serif" | "-apple-system" | "arial" => {
            fonts.sans.clone()
        }

        "serif" | "ui-serif" => fonts.serif.clone(),

        "monospace" | "ui-monospace" => fonts.mono.clone(),

        "cursive" => "rbxasset://fonts/families/Cartoon.json".into(),

        "fantasy" => "rbxasset://fonts/families/Fantasy.json".into(),

        _ if name.starts_with("rbxasset") => name.to_string(),

        // The family files of the `Enum.Font` names that differ.
        "gotham" => "rbxasset://fonts/families/GothamSSm.json".into(),

        "source sans" | "sourcesans" | "source sans pro" => fonts.sans.clone(),

        _ => format!("rbxasset://fonts/families/{}.json", name.replace(' ', "")),
    }
}

/// The name RichText's `face` takes for a family file: the `Enum.Font`
/// name where the file carries another.
pub fn rich_face(url: &str) -> String {
    let file = url
        .rsplit('/')
        .next()
        .unwrap_or(url)
        .trim_end_matches(".json");

    match file {
        "SourceSansPro" => "SourceSans",
        "GothamSSm" => "Gotham",
        "Arimo" => "Arial",
        other => other,
    }
    .to_string()
}

/// The TextSize a `font-size` names.
pub fn font_size(value: &str) -> Option<f64> {
    let v = value.trim().to_ascii_lowercase();
    let keyword = match v.as_str() {
        "xx-small" => 9.0,
        "x-small" => 10.0,
        "small" => 13.0,
        "medium" => 16.0,
        "large" => 18.0,
        "x-large" => 24.0,
        "xx-large" => 32.0,
        "xxx-large" => 48.0,
        _ => return css::length(&v).filter(|l| l.scale == 0.0).map(|l| l.offset),
    };

    Some(keyword)
}

/// One CSS property Silk knows: its name, what it becomes, and the
/// keywords its value takes.
pub struct Known {
    pub name: &'static str,
    pub sets: &'static str,
    pub values: &'static [&'static str],
}

const K: fn(&'static str, &'static str, &'static [&'static str]) -> Known =
    |name, sets, values| Known { name, sets, values };

/// Every CSS property Silk maps, for completion and hover.
pub fn catalog() -> Vec<Known> {
    vec![
        K(
            "appearance",
            "`none` drops the look a browser gives a control: its background, border, corner, and padding",
            &["none", "auto"],
        ),
        K(
            "align-items",
            "the cross-axis alignment of the `UIListLayout`",
            &[
                "flex-start",
                "center",
                "flex-end",
                "stretch",
                "start",
                "end",
            ],
        ),
        K(
            "align-self",
            "`UIFlexItem.ItemLineAlignment`",
            &["auto", "flex-start", "center", "flex-end", "stretch"],
        ),
        K("aspect-ratio", "a `UIAspectRatioConstraint`", &[]),
        K(
            "background",
            "`BackgroundColor3` and `BackgroundTransparency`, a `UIGradient` for `linear-gradient()`, or `Image` for `url()`",
            &["transparent", "none"],
        ),
        K(
            "background-color",
            "`BackgroundColor3` and `BackgroundTransparency`",
            &["transparent"],
        ),
        K(
            "background-image",
            "a `UIGradient` for `linear-gradient()`, or `Image` for `url()`",
            &["none"],
        ),
        K(
            "border",
            "a `UIStroke`: `Thickness`, `Color`, `Transparency`",
            &["none"],
        ),
        K("border-color", "`UIStroke.Color`", &[]),
        K(
            "border-transparency",
            "`UIStroke.Transparency`, 0 to 1; takes a source",
            &[],
        ),
        K(
            "background-gradient",
            "a `UIGradient`'s `Color`: a Luau ColorSequence, or a source of one",
            &[],
        ),
        K(
            "gradient-rotation",
            "the `UIGradient`'s `Rotation`, in degrees",
            &[],
        ),
        K(
            "gradient-transparency",
            "the `UIGradient`'s `Transparency`: a Luau NumberSequence, or a source of one",
            &[],
        ),
        K(
            "border-image",
            "a `UIGradient` inside the `UIStroke`, for `linear-gradient()`",
            &["none"],
        ),
        K("border-radius", "a `UICorner`: `CornerRadius`", &[]),
        K(
            "border-style",
            "`UIStroke.Enabled`",
            &["solid", "none", "hidden"],
        ),
        K("border-width", "`UIStroke.Thickness`", &[]),
        K(
            "bottom",
            "`Position` and `AnchorPoint` from the bottom edge",
            &["auto"],
        ),
        K(
            "color",
            "`TextColor3` and `TextTransparency`, or `ImageColor3` on an image",
            &["transparent"],
        ),
        K("column-gap", "the `UIListLayout` padding across a row", &[]),
        K(
            "display",
            "the layout child: `flex` a horizontal `UIListLayout`, `grid` a `UIGridLayout`, `none` `Visible = false`",
            &[
                "block",
                "flex",
                "grid",
                "none",
                "inline",
                "inline-block",
                "inline-flex",
            ],
        ),
        K("flex", "a `UIFlexItem`", &["1", "auto", "none"]),
        K(
            "flex-direction",
            "`UIListLayout.FillDirection`",
            &["row", "column", "row-reverse", "column-reverse"],
        ),
        K("flex-grow", "`UIFlexItem.GrowRatio`", &[]),
        K("flex-shrink", "`UIFlexItem.ShrinkRatio`", &[]),
        K("flex-wrap", "`UIListLayout.Wraps`", &["wrap", "nowrap"]),
        K("font", "`FontFace` and `TextSize`", &[]),
        K(
            "font-family",
            "the family of `FontFace`",
            &[
                "sans-serif",
                "serif",
                "monospace",
                "Gotham",
                "Roboto",
                "SourceSansPro",
                "Montserrat",
                "Ubuntu",
            ],
        ),
        K(
            "font-size",
            "`TextSize`",
            &["small", "medium", "large", "x-large", "xx-large"],
        ),
        K(
            "font-style",
            "the style of `FontFace`",
            &["normal", "italic"],
        ),
        K(
            "font-weight",
            "the weight of `FontFace`",
            &[
                "normal", "bold", "100", "200", "300", "400", "500", "600", "700", "800", "900",
            ],
        ),
        K(
            "gap",
            "`UIListLayout.Padding`, or `UIGridLayout.CellPadding`",
            &[],
        ),
        K(
            "grid-auto-rows",
            "the height of `UIGridLayout.CellSize`",
            &[],
        ),
        K(
            "grid-template-columns",
            "`UIGridLayout.CellSize` and `FillDirectionMaxCells`",
            &["repeat(2, 1fr)", "repeat(3, 1fr)", "repeat(4, 1fr)"],
        ),
        K(
            "grid-template-rows",
            "the height of `UIGridLayout.CellSize`",
            &[],
        ),
        K("height", "`Size` and `AutomaticSize`", &["auto"]),
        K("image-rendering", "`ResampleMode`", &["auto", "pixelated"]),
        K(
            "justify-content",
            "the main-axis alignment of the `UIListLayout`",
            &[
                "flex-start",
                "center",
                "flex-end",
                "space-between",
                "space-around",
                "space-evenly",
            ],
        ),
        K("left", "`Position`", &["auto"]),
        K("line-height", "`LineHeight`", &["normal"]),
        K("max-height", "a `UISizeConstraint`: `MaxSize`", &["none"]),
        K("max-width", "a `UISizeConstraint`: `MaxSize`", &["none"]),
        K("min-height", "a `UISizeConstraint`: `MinSize`", &[]),
        K("min-width", "a `UISizeConstraint`: `MinSize`", &[]),
        K(
            "object-fit",
            "`ScaleType`",
            &["fill", "contain", "cover", "scale-down"],
        ),
        K(
            "opacity",
            "the transparency of the background, the text, and the image",
            &[],
        ),
        K("order", "`LayoutOrder`", &[]),
        K("outline", "a `UIStroke`", &["none"]),
        K(
            "overflow",
            "`ClipsDescendants`; `auto` and `scroll` make a `ScrollingFrame`",
            &["visible", "hidden", "auto", "scroll"],
        ),
        K(
            "overflow-x",
            "`ScrollingDirection` of a `ScrollingFrame`",
            &["visible", "hidden", "auto", "scroll"],
        ),
        K(
            "overflow-y",
            "`ScrollingDirection` of a `ScrollingFrame`",
            &["visible", "hidden", "auto", "scroll"],
        ),
        K("padding", "a `UIPadding`", &[]),
        K("padding-bottom", "`UIPadding.PaddingBottom`", &[]),
        K("padding-left", "`UIPadding.PaddingLeft`", &[]),
        K("padding-right", "`UIPadding.PaddingRight`", &[]),
        K("padding-top", "`UIPadding.PaddingTop`", &[]),
        K("pointer-events", "`Interactable`", &["auto", "none"]),
        K(
            "position",
            "with `top`, `left`, `right`, `bottom`: `Position` and `AnchorPoint`",
            &["static", "relative", "absolute", "fixed"],
        ),
        K(
            "right",
            "`Position` and `AnchorPoint` from the right edge",
            &["auto"],
        ),
        K("rotate", "`Rotation`", &[]),
        K("row-gap", "the `UIListLayout` padding down a column", &[]),
        K("scale", "a `UIScale`", &[]),
        K(
            "text-align",
            "`TextXAlignment`",
            &["left", "center", "right", "start", "end", "justify"],
        ),
        K(
            "text-decoration",
            "RichText `<u>` or `<s>` on an element's own text",
            &["none", "underline", "line-through"],
        ),
        K("text-overflow", "`TextTruncate`", &["clip", "ellipsis"]),
        K(
            "text-transform",
            "RichText `<uc>` or `<sc>` on an element's own text",
            &["none", "uppercase", "small-caps"],
        ),
        K("top", "`Position`", &["auto"]),
        K(
            "transform",
            "`translate()` to `Position` and `AnchorPoint`, `rotate()` to `Rotation`, `scale()` to a `UIScale`",
            &["translate(-50%, -50%)", "rotate(45deg)", "scale(1.1)"],
        ),
        K(
            "vertical-align",
            "`TextYAlignment`",
            &["top", "middle", "bottom"],
        ),
        K("visibility", "`Visible`", &["visible", "hidden"]),
        K(
            "white-space",
            "`TextWrapped`",
            &["normal", "nowrap", "pre", "pre-wrap"],
        ),
        K("width", "`Size` and `AutomaticSize`", &["auto"]),
        K("z-index", "`ZIndex`", &[]),
    ]
}

/// The modifier key of a UIGradient that sits inside the UIStroke, for
/// `border-image`: the emitter writes it as a child of the stroke.
pub const STROKE_GRADIENT: &str = "UIStroke.UIGradient";

/// The text properties CSS inherits from a box to the text inside it.
pub const INHERITED: &[&str] = &[
    "color",
    "font",
    "font-family",
    "font-size",
    "font-style",
    "font-weight",
    "line-height",
    "text-align",
    "text-transform",
    "white-space",
    "text-overflow",
    "vertical-align",
    "text-decoration",
    "text-decoration-line",
    "font-variant",
    "font-variant-caps",
];

/// CSS properties that parse and set nothing on a Roblox instance.
pub const NO_EFFECT: &[&str] = &[
    "animation",
    "backdrop-filter",
    "box-shadow",
    "box-sizing",
    "caret-color",
    "cursor",
    "filter",
    "letter-spacing",
    "margin",
    "margin-bottom",
    "margin-left",
    "margin-right",
    "margin-top",
    "outline-offset",
    "resize",
    "scroll-behavior",
    "text-indent",
    "text-shadow",
    "transition",
    "user-select",
    "will-change",
    "word-spacing",
    "word-break",
    "overflow-wrap",
    "content",
    "float",
    "clear",
    "accent-color",
];

/// Applies a declaration list. Later declarations win, as in CSS.
pub fn apply(decls: &[Decl], ctx: &Ctx, out: &mut Out) {
    let mut flex = FlexParts::default();
    let mut grid = GridParts::default();
    let mut pos = PosParts::default();

    for d in decls {
        let value = css::substitute(&d.value, ctx.vars);
        let v = value.trim();
        let lower = v.to_ascii_lowercase();
        let span = d.value_span;
        let bad = |out: &mut Out| {
            out.problem(
                span,
                "bad_value",
                format!("`{v}` is not a value `{}` takes", d.name),
            );
        };

        if d.name.starts_with("--") {
            continue;
        }

        // A Roblox property written as itself.
        if d.name.chars().next().is_some_and(char::is_uppercase) {
            if !ctx.roblox {
                out.problem(
                    d.name_span,
                    "roblox_instance",
                    format!(
                        "`{}` is a Roblox property, and this project turns Roblox names off",
                        d.name
                    ),
                );

                continue;
            }

            out.set(&d.name, roblox_value(&d.name, v));

            continue;
        }

        // A variable the file never sets leaves nothing to read.
        if v.is_empty() && d.value.contains("var(") {
            out.problem(
                span,
                "bad_value",
                format!(
                    "`{}` names a variable no `:root` rule of this file sets",
                    d.value.trim()
                ),
            );

            continue;
        }

        // Text properties on a box are inherited: the text inside takes
        // them, and the box itself has no text to set.
        if ctx.target == Target::Container && INHERITED.contains(&d.name.as_str()) {
            continue;
        }

        match d.name.as_str() {
            // A list's markers: read at compile time, see the emitter.
            "list-style" | "list-style-type" => {}

            "background-color" => match css::color(v) {
                Some(c) => out.background = Some(c),

                None => bad(out),
            },

            "background" | "background-image" => {
                if lower == "none" {
                    out.background = Some(Rgba {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 0.0,
                    });
                } else if let Some(c) = css::color(v) {
                    out.background = Some(c);
                } else if lower.starts_with("linear-gradient(") {
                    match gradient(v) {
                        Some((seq, rotation, transparency)) => {
                            out.background.get_or_insert(Rgba {
                                r: 255,
                                g: 255,
                                b: 255,
                                a: 1.0,
                            });
                            out.modifier("UIGradient", "Color", seq);
                            out.modifier("UIGradient", "Rotation", num(rotation));

                            if let Some(t) = transparency {
                                out.modifier("UIGradient", "Transparency", t);
                            }
                        }

                        None => bad(out),
                    }
                } else if let Some(url) = url(v) {
                    if ctx.target.has_image() {
                        out.set("Image", format!("\"{url}\""));
                    } else {
                        out.problem(
                            span,
                            "no_effect",
                            "an image background needs an `<img>`; a Frame has no `Image`",
                        );
                    }
                } else {
                    bad(out);
                }
            }

            "color" => match css::color(v) {
                Some(c) if ctx.target == Target::Image => {
                    out.set("ImageColor3", c.luau());
                    out.set("ImageTransparency", transparency(c.a));
                }

                Some(c) if ctx.target.has_text() => {
                    out.set("TextColor3", c.luau());
                    out.set("TextTransparency", transparency(c.a));
                }

                Some(_) => out.problem(
                    span,
                    "no_effect",
                    "`color` colors text, and this element has none",
                ),

                None => bad(out),
            },

            "opacity" => match v
                .parse::<f64>()
                .ok()
                .or_else(|| css::length(v).map(|l| l.scale))
            {
                Some(o) => out.opacity = Some(o.clamp(0.0, 1.0)),

                None => bad(out),
            },

            "width" | "height" => {
                let axis = match lower.as_str() {
                    "auto" | "fit-content" | "max-content" | "min-content" => Some(Axis::Auto),

                    _ => css::length(v).map(Axis::Len),
                };

                match (axis, d.name.as_str()) {
                    (Some(a), "width") => out.width = Some(a),

                    (Some(a), _) => out.height = Some(a),

                    (None, _) => bad(out),
                }
            }

            "min-width" | "min-height" | "max-width" | "max-height" => {
                if lower == "none" {
                    continue;
                }

                match css::length(v) {
                    Some(l) if l.scale == 0.0 => {
                        let (key, x) = match d.name.as_str() {
                            "min-width" => ("MinSize", true),
                            "min-height" => ("MinSize", false),
                            "max-width" => ("MaxSize", true),
                            _ => ("MaxSize", false),
                        };
                        constraint(out, key, x, l.offset);
                    }

                    Some(_) => out.problem(
                        span,
                        "no_effect",
                        "a `UISizeConstraint` takes pixels, not a percent",
                    ),

                    None => bad(out),
                }
            }

            "aspect-ratio" => {
                let ratio = match v.split_once('/') {
                    Some((a, b)) => a
                        .trim()
                        .parse::<f64>()
                        .ok()
                        .zip(b.trim().parse::<f64>().ok())
                        .map(|(a, b)| a / b),

                    None => v.parse::<f64>().ok(),
                };

                match ratio {
                    Some(r) if r > 0.0 => {
                        out.modifier("UIAspectRatioConstraint", "AspectRatio", num(r))
                    }

                    _ => bad(out),
                }
            }

            "padding" => match four(v) {
                Some([t, r, b, l]) => {
                    out.modifier("UIPadding", "PaddingTop", t.udim());
                    out.modifier("UIPadding", "PaddingRight", r.udim());
                    out.modifier("UIPadding", "PaddingBottom", b.udim());
                    out.modifier("UIPadding", "PaddingLeft", l.udim());
                }

                None => bad(out),
            },

            "padding-top" | "padding-right" | "padding-bottom" | "padding-left" => {
                match css::length(v) {
                    Some(l) => {
                        let key = match d.name.as_str() {
                            "padding-top" => "PaddingTop",
                            "padding-right" => "PaddingRight",
                            "padding-bottom" => "PaddingBottom",
                            _ => "PaddingLeft",
                        };
                        out.modifier("UIPadding", key, l.udim());
                    }

                    None => bad(out),
                }
            }

            "border" | "outline" | "border-top" | "border-right" | "border-bottom"
            | "border-left" => {
                if d.name.starts_with("border-") && d.name != "border" {
                    out.problem(
                        d.name_span,
                        "no_effect",
                        "a `UIStroke` draws every side; use `border`",
                    );

                    continue;
                }

                stroke(out, v, span);
            }

            "border-width" => match css::length(v) {
                Some(l) => out.modifier("UIStroke", "Thickness", num(l.offset)),

                None => bad(out),
            },

            "border-color" => match css::color(v) {
                Some(c) => {
                    out.modifier("UIStroke", "Color", c.luau());
                    out.modifier("UIStroke", "Transparency", transparency(c.a));
                }

                None => bad(out),
            },

            // A gradient along the border: a UIGradient inside the UIStroke.
            // The stroke turns white, so the gradient's colors show as they
            // are, as `border-image` replaces the border color.
            "border-image" | "border-image-source" => {
                let source = v.rfind(')').map_or(v, |end| &v[..=end]);

                match gradient(source) {
                    Some((seq, rotation, transparency)) => {
                        out.modifier("UIStroke", "Color", "Color3.new(1, 1, 1)");
                        out.modifier("UIStroke", "ApplyStrokeMode", "Enum.ApplyStrokeMode.Border");
                        out.modifier(STROKE_GRADIENT, "Color", seq);
                        out.modifier(STROKE_GRADIENT, "Rotation", num(rotation));

                        if let Some(t) = transparency {
                            out.modifier(STROKE_GRADIENT, "Transparency", t);
                        }
                    }

                    None if lower == "none" => {}

                    None => bad(out),
                }
            }

            // Roblox's own axis of a stroke, 0 to 1, which a source can
            // drive as it is: `borderTransparency = fade`.
            "border-transparency" => match v.parse::<f64>() {
                Ok(t) => {
                    out.modifier("UIStroke", "Transparency", num(t.clamp(0.0, 1.0)));
                    out.modifier("UIStroke", "ApplyStrokeMode", "Enum.ApplyStrokeMode.Border");
                }

                Err(_) => bad(out),
            },

            "gradient-rotation" => match css::degrees(v).or_else(|| v.parse().ok()) {
                Some(deg) => out.modifier("UIGradient", "Rotation", num(deg)),

                None => bad(out),
            },

            // A live gradient takes Luau alone; a literal one is CSS.
            "background-gradient" | "gradient-transparency" => out.problem(
                span,
                "bad_value",
                format!(
                    "`{}` takes a Luau value, a {}; write a literal gradient as `background: linear-gradient(...)`",
                    d.name,
                    match d.name.as_str() {
                        "background-gradient" => "ColorSequence",

                        _ => "NumberSequence",
                    }
                ),
            ),

            "border-style" => match lower.as_str() {
                "none" | "hidden" => out.modifier("UIStroke", "Enabled", "false"),

                "solid" => out.modifier("UIStroke", "Enabled", "true"),

                _ => out.problem(span, "no_effect", "a `UIStroke` draws a solid line"),
            },

            "border-radius" => {
                match css::words(v).first().and_then(|w| css::length(w)) {
                    Some(l) => {
                        if css::words(v).len() > 1 {
                            out.problem(span, "no_effect", "a `UICorner` rounds every corner the same; Silk takes the first value");
                        }

                        out.modifier("UICorner", "CornerRadius", l.udim());
                    }

                    None => bad(out),
                }
            }

            "display" => match lower.as_str() {
                "none" => out.set("Visible", "false"),

                "flex" | "inline-flex" => {
                    out.layout_kind = Some(Layout::Flex);
                    flex.display = true;
                }

                "grid" | "inline-grid" => out.layout_kind = Some(Layout::Grid),

                "block" | "inline" | "inline-block" | "contents" | "flow-root" => {}

                _ => bad(out),
            },

            "flex-direction" => {
                flex.direction = Some(lower.clone());
                flex.display = true;
            }

            "flex-wrap" => out.layout("Wraps", (lower == "wrap").to_string()),

            "justify-content" => flex.justify = Some(lower.clone()),

            "align-items" => flex.align = Some(lower.clone()),

            "gap" | "row-gap" | "column-gap" => {
                match css::words(v).first().and_then(|w| css::length(w)) {
                    Some(l) => {
                        grid.gap = Some(l);

                        match d.name.as_str() {
                            "row-gap" => flex.row_gap = Some(l),

                            "column-gap" => flex.column_gap = Some(l),

                            _ => flex.gap = Some(l),
                        }
                    }

                    None => bad(out),
                }
            }

            "grid-template-columns" => match columns(v) {
                Some((n, cell)) => {
                    grid.columns = Some(n);
                    grid.cell_width = Some(cell);
                }

                None => bad(out),
            },

            "grid-auto-rows" | "grid-template-rows" => {
                match css::words(v).first().and_then(|w| css::length(w)) {
                    Some(l) => grid.row = Some(l),

                    None => bad(out),
                }
            }

            "flex" => {
                let words = css::words(&lower);

                match words.first().copied() {
                    Some("none") => out.modifier("UIFlexItem", "FlexMode", "Enum.UIFlexMode.None"),

                    Some("auto") => out.modifier("UIFlexItem", "FlexMode", "Enum.UIFlexMode.Fill"),

                    Some(n) => match n.parse::<f64>() {
                        Ok(g) => {
                            out.modifier("UIFlexItem", "FlexMode", "Enum.UIFlexMode.Custom");
                            out.modifier("UIFlexItem", "GrowRatio", num(g));

                            if let Some(s) = words.get(1).and_then(|s| s.parse::<f64>().ok()) {
                                out.modifier("UIFlexItem", "ShrinkRatio", num(s));
                            }
                        }

                        Err(_) => bad(out),
                    },

                    None => bad(out),
                }
            }

            "flex-grow" | "flex-shrink" => match v.parse::<f64>() {
                Ok(n) => {
                    out.modifier("UIFlexItem", "FlexMode", "Enum.UIFlexMode.Custom");
                    let key = match d.name.as_str() {
                        "flex-grow" => "GrowRatio",
                        _ => "ShrinkRatio",
                    };
                    out.modifier("UIFlexItem", key, num(n));
                }

                Err(_) => bad(out),
            },

            "align-self" => match line_alignment(&lower) {
                Some(a) => out.modifier(
                    "UIFlexItem",
                    "ItemLineAlignment",
                    format!("Enum.ItemLineAlignment.{a}"),
                ),

                None => bad(out),
            },

            "order" => match v.parse::<i64>() {
                Ok(n) => out.set("LayoutOrder", n.to_string()),

                Err(_) => bad(out),
            },

            "position" => pos.mode = Some(lower.clone()),

            "top" | "left" | "right" | "bottom" => {
                if lower == "auto" {
                    continue;
                }

                match css::length(v) {
                    Some(l) => match d.name.as_str() {
                        "top" => pos.top = Some(l),
                        "left" => pos.left = Some(l),
                        "right" => pos.right = Some(l),
                        _ => pos.bottom = Some(l),
                    },

                    None => bad(out),
                }
            }

            "transform" => {
                for f in css::words(&lower) {
                    let (name, args) = match f.split_once('(') {
                        Some((n, a)) => (n, a.trim_end_matches(')')),

                        None => (f, ""),
                    };
                    let parts: Vec<&str> = args.split(',').map(str::trim).collect();

                    match name {
                        "translate" | "translatex" | "translatey" => {
                            let (x, y) = match name {
                                "translatex" => (parts.first().copied(), None),
                                "translatey" => (None, parts.first().copied()),
                                _ => (parts.first().copied(), parts.get(1).copied()),
                            };

                            if let Some(x) = x.and_then(css::length) {
                                pos.translate.0 = Some(x);
                            }

                            if let Some(y) = y.and_then(css::length) {
                                pos.translate.1 = Some(y);
                            }
                        }

                        "rotate" => match parts.first().and_then(|a| css::degrees(a)) {
                            Some(deg) => out.set("Rotation", num(deg)),

                            None => bad(out),
                        },

                        "scale" => match parts.first().and_then(|a| a.parse::<f64>().ok()) {
                            Some(s) => out.modifier("UIScale", "Scale", num(s)),

                            None => bad(out),
                        },

                        "none" => {}

                        _ => {
                            out.problem(span, "no_effect", format!("`{name}()` has no Roblox form"))
                        }
                    }
                }
            }

            "rotate" => match css::degrees(v) {
                Some(deg) => out.set("Rotation", num(deg)),

                None => bad(out),
            },

            "scale" => match v.parse::<f64>() {
                Ok(s) => out.modifier("UIScale", "Scale", num(s)),

                Err(_) => bad(out),
            },

            "z-index" => match v.parse::<i64>() {
                Ok(n) => out.set("ZIndex", n.to_string()),

                Err(_) if lower == "auto" => {}

                Err(_) => bad(out),
            },

            "visibility" => match lower.as_str() {
                "hidden" | "collapse" => out.set("Visible", "false"),

                "visible" => out.set("Visible", "true"),

                _ => bad(out),
            },

            "overflow" | "overflow-x" | "overflow-y" => match lower.as_str() {
                "hidden" | "clip" => out.set("ClipsDescendants", "true"),

                "visible" => out.set("ClipsDescendants", "false"),

                "auto" | "scroll" => {
                    out.scroll = Some(match (d.name.as_str(), out.scroll) {
                        ("overflow-x", Some("Y")) | ("overflow-y", Some("X")) => "XY",

                        ("overflow-x", _) => "X",

                        ("overflow-y", _) => "Y",

                        _ => "XY",
                    });
                    out.set("ClipsDescendants", "true");
                }

                _ => bad(out),
            },

            "appearance" | "-webkit-appearance" => match lower.as_str() {
                "none" => out.plain = true,

                "auto" => out.plain = false,

                _ => bad(out),
            },

            "pointer-events" => match lower.as_str() {
                "none" => out.set("Interactable", "false"),

                "auto" => out.set("Interactable", "true"),

                _ => bad(out),
            },

            "font-size" => match font_size(v) {
                Some(px) if ctx.target.has_text() => out.set("TextSize", num(px)),

                Some(_) => out.problem(
                    span,
                    "no_effect",
                    "`font-size` sizes text, and this element has none",
                ),

                None => bad(out),
            },

            "font-weight" => match weight(&lower) {
                Some(w) => out.font.weight = Some(w),

                None => bad(out),
            },

            "font-style" => match lower.as_str() {
                "italic" | "oblique" => out.font.italic = Some(true),

                "normal" => out.font.italic = Some(false),

                _ => bad(out),
            },

            "font-family" => out.font.family = Some(family(v, ctx.fonts)),

            "font" => font_shorthand(out, v, ctx, span),

            "line-height" => match lower.as_str() {
                "normal" => out.set("LineHeight", "1"),

                _ => match v.parse::<f64>() {
                    Ok(n) => out.set("LineHeight", num(n)),

                    Err(_) => match css::length(v) {
                        Some(l) if l.scale > 0.0 => out.set("LineHeight", num(l.scale)),

                        Some(l) => {
                            let size = out
                                .get("TextSize")
                                .and_then(|s| s.parse::<f64>().ok())
                                .unwrap_or(16.0);
                            out.set("LineHeight", num(l.offset / size));
                        }

                        None => bad(out),
                    },
                },
            },

            "text-align" => {
                let a = match lower.as_str() {
                    "left" | "start" | "justify" => "Left",
                    "center" => "Center",
                    "right" | "end" => "Right",
                    _ => {
                        bad(out);
                        continue;
                    }
                };
                out.set("TextXAlignment", format!("Enum.TextXAlignment.{a}"));
            }

            "vertical-align" => {
                let a = match lower.as_str() {
                    "top" | "text-top" => "Top",
                    "middle" => "Center",
                    "bottom" | "text-bottom" | "baseline" => "Bottom",
                    _ => {
                        bad(out);
                        continue;
                    }
                };
                out.set("TextYAlignment", format!("Enum.TextYAlignment.{a}"));
            }

            "white-space" => match lower.as_str() {
                "nowrap" | "pre" => out.set("TextWrapped", "false"),

                "normal" | "pre-wrap" | "pre-line" | "break-spaces" => {
                    out.set("TextWrapped", "true")
                }

                _ => bad(out),
            },

            "text-overflow" => match lower.as_str() {
                "ellipsis" => out.set("TextTruncate", "Enum.TextTruncate.AtEnd"),

                "clip" => out.set("TextTruncate", "Enum.TextTruncate.None"),

                _ => bad(out),
            },

            "text-decoration" | "text-decoration-line" => {
                for w in css::words(&lower) {
                    match w {
                        "underline" => out.rich.push(("<u>", "</u>")),

                        "line-through" => out.rich.push(("<s>", "</s>")),

                        _ => {}
                    }
                }
            }

            "text-transform" => match lower.as_str() {
                "uppercase" => out.rich.push(("<uc>", "</uc>")),

                "small-caps" => out.rich.push(("<sc>", "</sc>")),

                "none" => {}

                _ => out.problem(
                    span,
                    "no_effect",
                    "RichText has `uppercase` and `small-caps` only",
                ),
            },

            "font-variant" | "font-variant-caps" if lower == "small-caps" => {
                out.rich.push(("<sc>", "</sc>"));
            }

            "object-fit" | "background-size" | "image-rendering" if !ctx.target.has_image() => {
                out.problem(
                    d.name_span,
                    "no_effect",
                    format!("`{}` fits an image, and this element has none", d.name),
                );
            }

            "object-fit" | "background-size" => {
                let s = match lower.as_str() {
                    "fill" | "100% 100%" => "Stretch",
                    "contain" | "scale-down" => "Fit",
                    "cover" => "Crop",
                    _ => {
                        bad(out);
                        continue;
                    }
                };
                out.set("ScaleType", format!("Enum.ScaleType.{s}"));
            }

            "image-rendering" => match lower.as_str() {
                "pixelated" | "crisp-edges" => {
                    out.set("ResampleMode", "Enum.ResamplerMode.Pixelated")
                }

                "auto" | "smooth" => out.set("ResampleMode", "Enum.ResamplerMode.Default"),

                _ => bad(out),
            },

            name if NO_EFFECT.contains(&name) => {
                out.problem(
                    d.name_span,
                    "no_effect",
                    format!("`{name}` has no Roblox property behind it"),
                );
            }

            name => {
                out.problem(
                    d.name_span,
                    "unknown_property",
                    format!("Silk does not know `{name}`"),
                );
            }
        }
    }

    finish_flex(out, &flex);
    finish_grid(out, &grid);
    finish_position(out, &pos);
}

/// The value of a Roblox property written in CSS: a color, a number, a
/// boolean, or Luau as written.
fn roblox_value(name: &str, v: &str) -> String {
    if let Some(c) = css::color(v) {
        return c.luau();
    }

    if v.parse::<f64>().is_ok() || v == "true" || v == "false" {
        return v.to_string();
    }

    // A length: a UDim where the property takes one, else its pixels.
    let udim = matches!(
        name,
        "CornerRadius"
            | "PaddingTop"
            | "PaddingRight"
            | "PaddingBottom"
            | "PaddingLeft"
            | "Padding"
    );
    let pair = matches!(
        name,
        "Size" | "Position" | "CellSize" | "CellPadding" | "CanvasSize"
    );

    if let Some(l) = css::length(v) {
        return match (udim, pair) {
            (true, _) => l.udim(),

            (_, true) => udim2(l, l),

            _ if l.scale == 0.0 => num(l.offset),

            _ => l.udim(),
        };
    }

    if pair
        && let [x, y] = css::words(v).as_slice()
        && let (Some(x), Some(y)) = (css::length(x), css::length(y))
    {
        return udim2(x, y);
    }

    if v.starts_with('"') || v.starts_with('\'') || v.contains('(') || v.starts_with("Enum.") {
        return v.to_string();
    }

    format!("\"{v}\"")
}

fn url(v: &str) -> Option<String> {
    let inner = v.trim().strip_prefix("url(")?.strip_suffix(')')?;

    Some(
        inner
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string(),
    )
}

/// `linear-gradient(90deg, red, blue 80%)` as a `ColorSequence`, the
/// `UIGradient` rotation, and a `NumberSequence` when a stop is not opaque.
fn gradient(v: &str) -> Option<(String, f64, Option<String>)> {
    let inner = v
        .trim()
        .strip_prefix("linear-gradient(")?
        .strip_suffix(')')?;
    let mut args = css::commas(inner);
    // CSS points 180deg down; a UIGradient at 0 runs left to right, and
    // its rotation turns clockwise.
    let mut rotation = 90.0;

    if let Some(first) = args.first() {
        let f = first.to_ascii_lowercase();
        let side = match f.as_str() {
            "to right" => Some(90.0),
            "to left" => Some(270.0),
            "to bottom" => Some(180.0),
            "to top" => Some(0.0),
            _ => css::degrees(&f),
        };

        if let Some(deg) = side {
            rotation = deg - 90.0;
            args.remove(0);
        } else {
            rotation = 90.0;
        }
    }

    if args.len() < 2 {
        return None;
    }

    let n = args.len();
    let mut stops = Vec::new();

    for (k, a) in args.iter().enumerate() {
        let w = css::words(a);
        let c = css::color(w.first()?)?;
        let at = w
            .get(1)
            .and_then(|p| css::length(p))
            .map_or(k as f64 / (n - 1) as f64, |l| l.scale);
        stops.push((at.clamp(0.0, 1.0), c));
    }

    // A clear stop takes the color of the nearest stop that shows, as a
    // browser mixes the colors premultiplied: `gold, transparent` fades
    // the gold and does not darken it through black.
    for k in 0..stops.len() {
        if stops[k].1.a == 0.0 {
            let near = (1..stops.len())
                .flat_map(|d| [k.checked_sub(d), Some(k + d)])
                .flatten()
                .find(|&j| stops.get(j).is_some_and(|s| s.1.a > 0.0));

            if let Some(j) = near {
                let a = stops[k].1.a;
                stops[k].1 = Rgba { a, ..stops[j].1 };
            }
        }
    }

    let colors = stops
        .iter()
        .map(|(t, c)| format!("ColorSequenceKeypoint.new({}, {})", num(*t), c.luau()))
        .collect::<Vec<_>>()
        .join(", ");
    let transparency = stops.iter().any(|(_, c)| c.a < 1.0).then(|| {
        let points = stops
            .iter()
            .map(|(t, c)| {
                format!(
                    "NumberSequenceKeypoint.new({}, {})",
                    num(*t),
                    num(1.0 - c.a)
                )
            })
            .collect::<Vec<_>>()
            .join(", ");

        format!("NumberSequence.new({{ {points} }})")
    });

    Some((
        format!("ColorSequence.new({{ {colors} }})"),
        rotation,
        transparency,
    ))
}

fn stroke(out: &mut Out, v: &str, span: (usize, usize)) {
    if v.trim().eq_ignore_ascii_case("none") || v.trim() == "0" {
        out.modifier("UIStroke", "Enabled", "false");

        return;
    }

    for w in css::words(v) {
        if let Some(l) = css::length(w) {
            out.modifier("UIStroke", "Thickness", num(l.offset));
        } else if let Some(c) = css::color(w) {
            out.modifier("UIStroke", "Color", c.luau());
            out.modifier("UIStroke", "Transparency", transparency(c.a));
        } else if matches!(w, "none" | "hidden") {
            out.modifier("UIStroke", "Enabled", "false");
        } else if !matches!(
            w,
            "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge" | "inset" | "outset"
        ) {
            out.problem(
                span,
                "bad_value",
                format!("`{w}` is not a width, a style, or a color"),
            );
        }
    }

    out.modifier("UIStroke", "ApplyStrokeMode", "Enum.ApplyStrokeMode.Border");
}

fn constraint(out: &mut Out, key: &str, x: bool, px: f64) {
    let current = out
        .mods
        .iter()
        .find(|(c, _)| *c == "UISizeConstraint")
        .and_then(|(_, p)| p.iter().find(|(k, _)| k == key))
        .map(|(_, v)| v.clone());
    let (mut cx, mut cy) = match key {
        "MinSize" => ("0".to_string(), "0".to_string()),
        _ => ("math.huge".to_string(), "math.huge".to_string()),
    };

    if let Some(cur) = current
        && let Some(inner) = cur
            .strip_prefix("Vector2.new(")
            .and_then(|r| r.strip_suffix(')'))
        && let Some((a, b)) = inner.split_once(", ")
    {
        cx = a.to_string();
        cy = b.to_string();
    }

    match x {
        true => cx = num(px),

        false => cy = num(px),
    }

    out.modifier("UISizeConstraint", key, format!("Vector2.new({cx}, {cy})"));
}

fn columns(v: &str) -> Option<(u32, Len)> {
    let t = v.trim().to_ascii_lowercase();

    if let Some(inner) = t.strip_prefix("repeat(").and_then(|r| r.strip_suffix(')')) {
        let (n, track) = inner.split_once(',')?;
        let n: u32 = n.trim().parse().ok()?;
        let track = track.trim();
        let cell = match track.ends_with("fr") {
            true => Len {
                scale: 1.0 / f64::from(n),
                offset: 0.0,
            },

            false => css::length(track)?,
        };

        return Some((n.max(1), cell));
    }

    let tracks = css::words(&t);
    let n = tracks.len() as u32;
    let first = tracks.first()?;
    let cell = match first.ends_with("fr") {
        true => Len {
            scale: 1.0 / f64::from(n.max(1)),
            offset: 0.0,
        },

        false => css::length(first)?,
    };

    Some((n.max(1), cell))
}

fn line_alignment(v: &str) -> Option<&'static str> {
    Some(match v {
        "auto" | "normal" => "Automatic",
        "flex-start" | "start" | "self-start" => "Start",
        "center" => "Center",
        "flex-end" | "end" | "self-end" => "End",
        "stretch" => "Stretch",
        _ => return None,
    })
}

fn font_shorthand(out: &mut Out, v: &str, ctx: &Ctx, span: (usize, usize)) {
    let words = css::words(v);
    let mut i = 0;

    while i < words.len() {
        let w = words[i].to_ascii_lowercase();

        if w == "italic" || w == "oblique" {
            out.font.italic = Some(true);
        } else if let Some(wt) = weight(&w).filter(|_| w != "normal") {
            out.font.weight = Some(wt);
        } else if let Some(size) = font_size(w.split('/').next().unwrap_or(&w)) {
            if ctx.target.has_text() {
                out.set("TextSize", num(size));
            }

            let family = words[i + 1..].join(" ");

            if !family.is_empty() {
                out.font.family = Some(self::family(&family, ctx.fonts));
            }

            return;
        } else if w != "normal" {
            out.problem(
                span,
                "bad_value",
                format!("`{w}` is not part of a `font` Silk reads"),
            );
        }

        i += 1;
    }
}

#[derive(Default)]
struct FlexParts {
    display: bool,
    direction: Option<String>,
    justify: Option<String>,
    align: Option<String>,
    gap: Option<Len>,
    row_gap: Option<Len>,
    column_gap: Option<Len>,
}

fn finish_flex(out: &mut Out, f: &FlexParts) {
    let touched = f.display || f.justify.is_some() || f.align.is_some() || f.gap.is_some();

    if !touched && f.row_gap.is_none() && f.column_gap.is_none() {
        return;
    }

    if out.layout_kind == Some(Layout::Grid) {
        return;
    }

    let row = match f.direction.as_deref() {
        Some("column" | "column-reverse") => false,

        Some(_) => true,

        // `justify-content` alone is about a flex row; a gap alone
        // spaces the flow, which runs down.
        None => f.display || f.justify.is_some() || f.align.is_some(),
    };

    if f.display {
        out.layout(
            "FillDirection",
            match row {
                true => "Enum.FillDirection.Horizontal",

                false => "Enum.FillDirection.Vertical",
            },
        );
    }

    let (main, cross) = match row {
        true => (
            (
                "HorizontalAlignment",
                "HorizontalFlex",
                ["Left", "Center", "Right"],
            ),
            ("VerticalAlignment", ["Top", "Center", "Bottom"]),
        ),

        false => (
            (
                "VerticalAlignment",
                "VerticalFlex",
                ["Top", "Center", "Bottom"],
            ),
            ("HorizontalAlignment", ["Left", "Center", "Right"]),
        ),
    };
    let enum_of = |key: &str| match key {
        "HorizontalAlignment" => "HorizontalAlignment",
        _ => "VerticalAlignment",
    };

    if let Some(j) = &f.justify {
        match j.as_str() {
            "flex-start" | "start" | "left" | "normal" => {
                out.layout(main.0, format!("Enum.{}.{}", enum_of(main.0), main.2[0]))
            }

            "center" => out.layout(main.0, format!("Enum.{}.{}", enum_of(main.0), main.2[1])),

            "flex-end" | "end" | "right" => {
                out.layout(main.0, format!("Enum.{}.{}", enum_of(main.0), main.2[2]))
            }

            "space-between" => out.layout(main.1, "Enum.UIFlexAlignment.SpaceBetween"),

            "space-around" => out.layout(main.1, "Enum.UIFlexAlignment.SpaceAround"),

            "space-evenly" => out.layout(main.1, "Enum.UIFlexAlignment.SpaceEvenly"),

            "stretch" => out.layout(main.1, "Enum.UIFlexAlignment.Fill"),

            _ => {}
        }
    }

    if let Some(a) = &f.align {
        match a.as_str() {
            "flex-start" | "start" | "baseline" | "normal" => {
                out.layout(cross.0, format!("Enum.{}.{}", enum_of(cross.0), cross.1[0]))
            }

            "center" => out.layout(cross.0, format!("Enum.{}.{}", enum_of(cross.0), cross.1[1])),

            "flex-end" | "end" => {
                out.layout(cross.0, format!("Enum.{}.{}", enum_of(cross.0), cross.1[2]))
            }

            "stretch" => out.layout("ItemLineAlignment", "Enum.ItemLineAlignment.Stretch"),

            _ => {}
        }
    }

    let gap = match row {
        true => f.column_gap.or(f.gap),

        false => f.row_gap.or(f.gap),
    };

    if let Some(g) = gap {
        out.layout("Padding", g.udim());
    }
}

#[derive(Default)]
struct GridParts {
    columns: Option<u32>,
    cell_width: Option<Len>,
    row: Option<Len>,
    gap: Option<Len>,
}

fn finish_grid(out: &mut Out, g: &GridParts) {
    if out.layout_kind != Some(Layout::Grid) {
        return;
    }

    let gap = g.gap.unwrap_or(Len::px(0.0));
    let mut width = g.cell_width.unwrap_or(Len::px(100.0));

    // A fraction of the row leaves room for the gaps between the cells.
    if let (Some(n), true) = (g.columns, width.scale > 0.0) {
        width.offset -= gap.offset * f64::from(n.saturating_sub(1)) / f64::from(n);
    }

    let height = g.row.unwrap_or(Len::px(100.0));
    out.layout("CellSize", udim2(width, height));
    out.layout("CellPadding", udim2(gap, gap));

    if let Some(n) = g.columns {
        out.layout("FillDirectionMaxCells", n.to_string());
    }
}

#[derive(Default)]
struct PosParts {
    mode: Option<String>,
    top: Option<Len>,
    left: Option<Len>,
    right: Option<Len>,
    bottom: Option<Len>,
    translate: (Option<Len>, Option<Len>),
}

fn finish_position(out: &mut Out, p: &PosParts) {
    let placed = p.top.is_some() || p.left.is_some() || p.right.is_some() || p.bottom.is_some();
    let translated = p.translate.0.is_some() || p.translate.1.is_some();

    if !placed && !translated {
        return;
    }

    if matches!(p.mode.as_deref(), Some("static")) {
        return;
    }

    let mut anchor = (0.0, 0.0);
    let mut x = p.left.unwrap_or(Len::px(0.0));
    let mut y = p.top.unwrap_or(Len::px(0.0));

    if let (None, Some(r)) = (p.left, p.right) {
        x = Len {
            scale: 1.0 - r.scale,
            offset: -r.offset,
        };
        anchor.0 = 1.0;
    }

    if let (None, Some(b)) = (p.top, p.bottom) {
        y = Len {
            scale: 1.0 - b.scale,
            offset: -b.offset,
        };
        anchor.1 = 1.0;
    }

    // `translate(-50%, -50%)` moves the element by its own size, which is
    // what an AnchorPoint does.
    if let Some(t) = p.translate.0 {
        anchor.0 -= t.scale;
        x.offset += t.offset;
    }

    if let Some(t) = p.translate.1 {
        anchor.1 -= t.scale;
        y.offset += t.offset;
    }

    out.set("Position", udim2(x, y));

    if anchor != (0.0, 0.0) {
        out.set(
            "AnchorPoint",
            format!("Vector2.new({}, {})", num(anchor.0), num(anchor.1)),
        );
    }

    if matches!(p.mode.as_deref(), Some("absolute" | "fixed")) {
        out.layout_kind.get_or_insert(Layout::Flow);
    }
}

/// The transparency properties an element's colors and opacity give.
///
/// A rule does not know the transparency of the element it styles, so its
/// `opacity` leaves the background alone unless the rule sets one.
pub fn finish_colors(out: &mut Out, target: Target, rule: bool) {
    let opacity = out.opacity.unwrap_or(1.0);

    if let Some(bg) = out.background {
        out.set("BackgroundColor3", bg.luau());
        out.set("BackgroundTransparency", transparency(bg.a * opacity));
    } else if out.opacity.is_some() && target != Target::Canvas && !rule {
        let base = out
            .get("BackgroundTransparency")
            .and_then(|t| t.parse::<f64>().ok())
            .unwrap_or(0.0);
        out.set(
            "BackgroundTransparency",
            transparency((1.0 - base) * opacity),
        );
    }

    if out.opacity.is_none() {
        return;
    }

    if target == Target::Canvas {
        out.set("GroupTransparency", transparency(opacity));

        return;
    }

    for key in ["TextTransparency", "ImageTransparency"] {
        let applies = match key {
            "TextTransparency" => target.has_text(),

            _ => target.has_image(),
        };

        if applies {
            let base = out
                .get(key)
                .and_then(|t| t.parse::<f64>().ok())
                .unwrap_or(0.0);
            out.set(key, transparency((1.0 - base) * opacity));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::css::parse_decls;

    fn run(style: &str, target: Target) -> Out {
        let vars = BTreeMap::new();
        let fonts = Fonts::default();
        let ctx = Ctx {
            vars: &vars,
            fonts: &fonts,
            roblox: true,
            target,
        };
        let mut out = Out::default();
        apply(&parse_decls(style, 0), &ctx, &mut out);
        finish_colors(&mut out, target, false);

        out
    }

    #[test]
    fn colors_padding_corners_and_strokes() {
        let out = run(
            "background-color: rgba(255, 0, 0, 0.5); color: #fff; padding: 4px 8px; border-radius: 6px; border: 2px solid #333",
            Target::Text,
        );

        assert_eq!(
            out.get("BackgroundColor3"),
            Some("Color3.fromRGB(255, 0, 0)")
        );
        assert_eq!(out.get("BackgroundTransparency"), Some("0.5"));
        assert_eq!(out.get("TextColor3"), Some("Color3.fromRGB(255, 255, 255)"));
        let padding = &out.mods.iter().find(|(c, _)| *c == "UIPadding").unwrap().1;
        assert!(padding.contains(&("PaddingLeft".into(), "UDim.new(0, 8)".into())));
        let stroke = &out.mods.iter().find(|(c, _)| *c == "UIStroke").unwrap().1;
        assert!(stroke.contains(&("Thickness".into(), "2".into())));
        assert!(stroke.contains(&("Color".into(), "Color3.fromRGB(51, 51, 51)".into())));
        assert!(out.problems.is_empty(), "{:?}", out.problems);
    }

    #[test]
    fn flex_becomes_the_list_layout() {
        let out = run(
            "display: flex; justify-content: space-between; align-items: center; gap: 12px",
            Target::Container,
        );

        assert_eq!(out.layout_kind, Some(Layout::Flex));
        assert!(out.layout.contains(&(
            "FillDirection".into(),
            "Enum.FillDirection.Horizontal".into()
        )));
        assert!(out.layout.contains(&(
            "HorizontalFlex".into(),
            "Enum.UIFlexAlignment.SpaceBetween".into()
        )));
        assert!(out.layout.contains(&(
            "VerticalAlignment".into(),
            "Enum.VerticalAlignment.Center".into()
        )));
        assert!(
            out.layout
                .contains(&("Padding".into(), "UDim.new(0, 12)".into()))
        );
    }

    #[test]
    fn a_centered_box_takes_an_anchor_point() {
        let out = run(
            "position: absolute; left: 50%; top: 50%; transform: translate(-50%, -50%); width: 200px; height: 50%",
            Target::Container,
        );

        assert_eq!(out.get("Position"), Some("UDim2.fromScale(0.5, 0.5)"));
        assert_eq!(out.get("AnchorPoint"), Some("Vector2.new(0.5, 0.5)"));
        assert_eq!(
            out.size((Axis::Auto, Axis::Auto)),
            Some((
                "UDim2.new(0, 200, 0.5, 0)".into(),
                "Enum.AutomaticSize.None".into()
            ))
        );
    }

    #[test]
    fn a_grid_and_a_gradient() {
        let out = run(
            "display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; background: linear-gradient(to right, red, blue)",
            Target::Container,
        );

        assert!(
            out.layout
                .contains(&("FillDirectionMaxCells".into(), "3".into()))
        );
        assert!(
            out.layout
                .iter()
                .any(|(k, v)| k == "CellSize" && v.starts_with("UDim2.new(0.333, -6.667"))
        );
        let g = &out.mods.iter().find(|(c, _)| *c == "UIGradient").unwrap().1;
        assert!(g.contains(&("Rotation".into(), "0".into())));
    }

    #[test]
    fn unknown_and_ineffective_properties_report() {
        let out = run("margin: 4px; colr: red; font-weight: bold", Target::Text);
        let lints: Vec<&str> = out.problems.iter().map(|p| p.lint).collect();

        assert_eq!(lints, vec!["no_effect", "unknown_property"]);
        assert_eq!(out.font.weight, Some("Bold"));
    }

    #[test]
    fn a_roblox_property_passes_or_reports() {
        assert_eq!(
            run("BackgroundColor3: #000", Target::Container).get("BackgroundColor3"),
            Some("Color3.fromRGB(0, 0, 0)")
        );

        let vars = BTreeMap::new();
        let fonts = Fonts::default();
        let ctx = Ctx {
            vars: &vars,
            fonts: &fonts,
            roblox: false,
            target: Target::Any,
        };
        let mut out = Out::default();
        apply(&parse_decls("ZIndex: 3", 0), &ctx, &mut out);
        assert_eq!(out.problems[0].lint, "roblox_instance");
    }
}
