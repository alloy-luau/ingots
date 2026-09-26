//! The utilities: one class token in, the properties and children it
//! sets out. The names are Tailwind's; the values are Roblox's. A class
//! carries variants, `hover:bg-red-500`, and a leading `-` negates a
//! position or a rotation.

use std::collections::BTreeMap;

use crate::fonts;
use crate::palette;
use crate::theme::{ClassDef, Theme};

/// The state a variant binds a utility to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Variant {
    Hover,
    Active,
    Focus,
    GroupHover,
}

impl Variant {
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "hover" => Some(Self::Hover),
            "active" => Some(Self::Active),
            "focus" => Some(Self::Focus),
            "group-hover" => Some(Self::GroupHover),
            _ => None,
        }
    }

    /// The key of the state in the helper's table.
    pub fn key(self) -> &'static str {
        match self {
            Self::Hover => "hover",
            Self::Active => "active",
            Self::Focus => "focus",
            Self::GroupHover => "group_hover",
        }
    }
}

/// One class token, split into its variants, its sign, and its base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    pub token: String,
    pub variants: Vec<Variant>,
    pub negative: bool,
    pub base: String,
    /// A variant word Enamel does not know, `md:` or `dark:`.
    pub unknown_variant: Option<String>,
}

impl Class {
    pub fn parse(token: &str) -> Self {
        let mut rest = token;
        let mut variants = Vec::new();
        let mut unknown_variant = None;

        // A colon inside brackets belongs to the value,
        // `image-[rbxassetid://1]`; a variant's colon comes before any.
        while let Some(colon) = rest.find(':')
            && rest.find('[').is_none_or(|b| colon < b)
        {
            let word = &rest[..colon];

            match Variant::parse(word) {
                Some(v) => variants.push(v),

                None => unknown_variant = Some(word.to_string()),
            }

            rest = &rest[colon + 1..];
        }

        let negative = rest.starts_with('-') && rest.len() > 1;
        let base = if negative { &rest[1..] } else { rest };

        Self {
            token: token.to_string(),
            variants,
            negative,
            base: base.to_string(),
            unknown_variant,
        }
    }
}

/// The GUI classes Enamel knows, by what they carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Element {
    Frame,
    ScrollingFrame,
    TextLabel,
    TextButton,
    TextBox,
    ImageLabel,
    ImageButton,
    CanvasGroup,
    ViewportFrame,
    VideoFrame,
    /// `ScreenGui`, `SurfaceGui`, `BillboardGui`: a container with
    /// `Enabled` and `DisplayOrder`, no size or color.
    LayerGui,
}

impl Element {
    pub fn parse(tag: &str) -> Option<Self> {
        Some(match tag {
            "Frame" => Self::Frame,
            "ScrollingFrame" => Self::ScrollingFrame,
            "TextLabel" => Self::TextLabel,
            "TextButton" => Self::TextButton,
            "TextBox" => Self::TextBox,
            "ImageLabel" => Self::ImageLabel,
            "ImageButton" => Self::ImageButton,
            "CanvasGroup" => Self::CanvasGroup,
            "ViewportFrame" => Self::ViewportFrame,
            "VideoFrame" => Self::VideoFrame,
            "ScreenGui" | "SurfaceGui" | "BillboardGui" => Self::LayerGui,
            _ => return Self::html(tag),
        })
    }

    /// The class Silk writes for an HTML element, so its `className`
    /// reads against the right properties.
    fn html(tag: &str) -> Option<Self> {
        Some(match tag {
            "button" | "a" => Self::TextButton,
            "input" | "textarea" => Self::TextBox,
            "img" => Self::ImageLabel,
            "canvas" => Self::CanvasGroup,
            "video" => Self::VideoFrame,
            "p" | "span" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "label" | "li" | "td"
            | "th" | "b" | "strong" | "i" | "em" | "small" | "code" | "pre" | "blockquote"
            | "dt" | "dd" | "figcaption" | "legend" | "caption" | "summary" | "mark" | "u"
            | "s" | "q" => Self::TextLabel,
            "div" | "section" | "article" | "main" | "header" | "footer" | "nav" | "aside"
            | "form" | "fieldset" | "figure" | "ul" | "ol" | "dl" | "table" | "tr" | "body"
            | "details" | "dialog" | "menu" | "hgroup" | "search" | "address" | "picture" => {
                Self::Frame
            }
            _ => return None,
        })
    }

    pub fn is_gui_object(self) -> bool {
        !matches!(self, Self::LayerGui)
    }

    pub fn has_text(self) -> bool {
        matches!(self, Self::TextLabel | Self::TextButton | Self::TextBox)
    }

    pub fn has_image(self) -> bool {
        matches!(self, Self::ImageLabel | Self::ImageButton)
    }
}

/// What a property needs of the element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    /// A theme property: the author knows the element.
    Any,
    Gui,
    Text,
    TextBox,
    Image,
    Scrolling,
    Canvas,
    Layer,
}

impl Needs {
    pub fn met_by(self, e: Element) -> bool {
        match self {
            Self::Any => true,
            Self::Gui => e.is_gui_object(),
            Self::Text => e.has_text(),
            Self::TextBox => e == Element::TextBox,
            Self::Image => e.has_image(),
            Self::Scrolling => e == Element::ScrollingFrame,
            Self::Canvas => e == Element::CanvasGroup,
            Self::Layer => e == Element::LayerGui,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Self::Any => "any element",
            Self::Gui => "a GuiObject",
            Self::Text => "a text element",
            Self::TextBox => "a TextBox",
            Self::Image => "an image element",
            Self::Scrolling => "a ScrollingFrame",
            Self::Canvas => "a CanvasGroup",
            Self::Layer => "a ScreenGui",
        }
    }
}

/// A `UDim` as scale and offset.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dim {
    pub scale: f64,
    pub offset: f64,
}

impl Dim {
    pub const fn px(offset: f64) -> Self {
        Self { scale: 0.0, offset }
    }

    pub const fn scale(scale: f64) -> Self {
        Self { scale, offset: 0.0 }
    }

    pub fn luau(self) -> String {
        format!("UDim.new({}, {})", num(self.scale), num(self.offset))
    }
}

/// One thing a utility sets. Pieces of one element combine in
/// [`resolve`]: two `Size` pieces make one `Size` property, every
/// `Layout` piece one `UIListLayout` child.
#[derive(Debug, Clone, PartialEq)]
pub enum Piece {
    /// A property of the element itself, as Luau text.
    Prop {
        name: String,
        value: String,
        needs: Needs,
    },
    SizeX(Dim),
    SizeY(Dim),
    AutoX,
    AutoY,
    /// A position along one axis; `from_end` anchors at the far side.
    PosX {
        dim: Dim,
        from_end: bool,
    },
    PosY {
        dim: Dim,
        from_end: bool,
    },
    AnchorX(f64),
    AnchorY(f64),
    /// A field of the `UIListLayout` or `UIGridLayout` child.
    Layout(&'static str, String),
    /// `justify-*` and `items-*`, resolved once the direction is known.
    Justify(&'static str),
    Items(&'static str),
    Gap {
        x: Option<f64>,
        y: Option<f64>,
    },
    Padding(&'static str, f64),
    Corner(Dim),
    Stroke(&'static str, String),
    /// A gradient stop, as the Luau expression of its color.
    GradientStop(&'static str, String),
    GradientRotation(f64),
    SizeConstraint(&'static str, &'static str, f64),
    Aspect(f64),
    Scale(f64),
    FlexItem(&'static str, String),
    /// `group`: the element marks itself for `group-hover:` below it.
    Group,
    /// `transition-colors`: what a state change tweens.
    Transition(&'static str),
    /// `duration-300`: the tween's seconds.
    Duration(f64),
    /// `ease-out`: the tween's easing style and direction.
    Ease(&'static str, &'static str),
    /// `delay-100`: the tween's delay, in seconds.
    Delay(f64),
    /// A value of the theme file: the file needs the prelude.
    Theme,
    /// A utility Roblox has no property for.
    NoEffect(&'static str),
}

/// A utility, parsed: what it sets and a line for the hover.
#[derive(Debug, Clone, PartialEq)]
pub struct Utility {
    pub pieces: Vec<Piece>,
    pub summary: String,
    /// The color the class names, for the swatch.
    pub color: Option<((u8, u8, u8), f64)>,
}

fn prop(name: &str, value: impl Into<String>, needs: Needs) -> Piece {
    Piece::Prop {
        name: name.to_string(),
        value: value.into(),
        needs,
    }
}

fn utility(pieces: Vec<Piece>, summary: impl Into<String>) -> Utility {
    Utility {
        pieces,
        summary: summary.into(),
        color: None,
    }
}

/// A number as Luau writes it: no trailing zeros, no `-0`.
pub fn num(v: f64) -> String {
    let v = if v == 0.0 { 0.0 } else { v };
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');

    if s.is_empty() || s == "-" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

pub fn color3((r, g, b): (u8, u8, u8)) -> String {
    format!("Color3.fromRGB({r}, {g}, {b})")
}

/// The spacing scale: `4` is 16 pixels, `px` one, `0.5` two, and
/// `[12px]` twelve.
fn spacing(word: &str) -> Option<f64> {
    if word == "px" {
        return Some(1.0);
    }

    if let Some(inner) = arbitrary(word) {
        return length(inner);
    }

    word.parse::<f64>().ok().map(|n| n * 4.0)
}

/// A time in seconds: `300` and `[300ms]` are milliseconds, `[0.3s]`
/// seconds.
fn seconds(word: &str) -> Option<f64> {
    let inner = arbitrary(word).unwrap_or(word);

    if let Some(ms) = inner.strip_suffix("ms") {
        return ms.parse::<f64>().ok().map(|n| n / 1000.0);
    }

    if let Some(secs) = inner.strip_suffix('s') {
        return secs.parse::<f64>().ok();
    }

    inner.parse::<f64>().ok().map(|n| n / 1000.0)
}

/// An easing word as a Roblox style and direction: `in-out` is Quad,
/// `back` is Back going out, `back-in` Back going in.
fn easing(word: &str) -> Option<(&'static str, &'static str)> {
    match word {
        "linear" => return Some(("Linear", "InOut")),
        "in" => return Some(("Quad", "In")),
        "out" => return Some(("Quad", "Out")),
        "in-out" => return Some(("Quad", "InOut")),
        _ => {}
    }

    let (style, direction) = if let Some(s) = word.strip_suffix("-in-out") {
        (s, "InOut")
    } else if let Some(s) = word.strip_suffix("-in") {
        (s, "In")
    } else if let Some(s) = word.strip_suffix("-out") {
        (s, "Out")
    } else {
        (word, "Out")
    };
    let style = match style {
        "sine" => "Sine",
        "quad" => "Quad",
        "cubic" => "Cubic",
        "quart" => "Quart",
        "quint" => "Quint",
        "back" => "Back",
        "bounce" => "Bounce",
        "elastic" => "Elastic",
        "expo" => "Exponential",
        "circ" => "Circular",
        _ => return None,
    };

    Some((style, direction))
}

/// A fraction or a word that names a scale: `1/2`, `full`, `screen`.
fn fraction(word: &str) -> Option<f64> {
    match word {
        "full" | "screen" => return Some(1.0),
        _ => {}
    }

    let (a, b) = word.split_once('/')?;
    let a: f64 = a.parse().ok()?;
    let b: f64 = b.parse().ok()?;

    (b != 0.0).then_some(a / b)
}

/// A size word as a `UDim`: a fraction is scale, a number is offset.
fn dim(word: &str) -> Option<Dim> {
    if let Some(inner) = arbitrary(word) {
        if let Some(p) = inner.strip_suffix('%') {
            return p.parse::<f64>().ok().map(|n| Dim::scale(n / 100.0));
        }

        return length(inner).map(Dim::px);
    }

    if let Some(s) = fraction(word) {
        return Some(Dim::scale(s));
    }

    spacing(word).map(Dim::px)
}

/// The inside of `[...]`.
fn arbitrary(word: &str) -> Option<&str> {
    word.strip_prefix('[')?.strip_suffix(']')
}

/// A plain number, or one in brackets: `3`, `[3]`, `[-2.5]`.
fn number(word: &str) -> Option<f64> {
    arbitrary(word).unwrap_or(word).parse().ok()
}

/// A ratio: `50` is half, `[0.35]` is as written.
fn ratio(word: &str) -> Option<f64> {
    if let Some(inner) = arbitrary(word) {
        return inner.strip_suffix('%').map_or_else(
            || inner.parse().ok(),
            |p| p.parse::<f64>().ok().map(|n| n / 100.0),
        );
    }

    word.parse::<f64>().ok().map(|n| n / 100.0)
}

/// A CSS length in pixels: `12px`, `12`, `0.5rem`.
fn length(text: &str) -> Option<f64> {
    if let Some(px) = text.strip_suffix("px") {
        return px.parse().ok();
    }

    if let Some(rem) = text.strip_suffix("rem") {
        return rem.parse::<f64>().ok().map(|n| n * 16.0);
    }

    text.parse().ok()
}

/// A color as the generated code writes it, with the color the reader
/// sees when it is one the ingot can read.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorValue {
    pub expr: String,
    pub rgb: Option<(u8, u8, u8)>,
    pub alpha: f64,
    /// The color is a name from the theme file, so the file needs the
    /// theme's prelude: the expression may name a local there.
    pub theme: bool,
}

impl ColorValue {
    /// The utility that sets this color through `pieces`, with its
    /// swatch at `alpha`. Every color class ends here, so a theme color
    /// marks the theme as used in one place.
    fn utility(&self, mut pieces: Vec<Piece>, summary: String, alpha: f64) -> Utility {
        if self.theme {
            pieces.push(Piece::Theme);
        }

        Utility {
            pieces,
            summary,
            color: self.rgb.map(|rgb| (rgb, alpha)),
        }
    }
}

/// A color word: a palette name, `white`, a name from the theme,
/// `[#ff0000]`, or `[rgb(1,2,3)]`, with an optional `/50` alpha.
pub fn color(word: &str, theme: &Theme) -> Option<ColorValue> {
    let (name, alpha) = match word.rsplit_once('/') {
        Some((n, a)) if !n.starts_with('[') || n.ends_with(']') => {
            let a: f64 = a.parse().ok()?;

            (n, a / 100.0)
        }

        _ => (word, 1.0),
    };

    if let Some(inner) = arbitrary(name) {
        if let Some(c) = hex(inner).or_else(|| rgb(inner)) {
            return Some(ColorValue {
                expr: color3(c),
                rgb: Some(c),
                alpha,
                theme: false,
            });
        }

        // Anything else in the brackets is Luau, copied as written; an
        // underscore stands for a space, since a class holds none.
        return Some(ColorValue {
            expr: inner.replace('_', " "),
            rgb: None,
            alpha,
            theme: false,
        });
    }

    if let Some(entry) = theme.colors.get(name) {
        // A theme color written as a string, `gold = "#ffd08a"`, is a
        // hex the property cannot take as written.
        let expr = if crate::theme::string_literal(&entry.expr).is_some() {
            format!("Color3.fromHex({})", entry.expr)
        } else {
            entry.expr.clone()
        };

        return Some(ColorValue {
            expr,
            rgb: entry.color(),
            alpha,
            theme: true,
        });
    }

    let c = palette::lookup(name)?;

    Some(ColorValue {
        expr: color3(c),
        rgb: Some(c),
        alpha,
        theme: false,
    })
}

/// The hex of a color value for a summary, or the expression.
fn color_words(c: &ColorValue) -> String {
    match c.rgb {
        Some(rgb) => palette::hex(rgb),

        None => format!("`{}`", c.expr),
    }
}

pub fn hex(text: &str) -> Option<(u8, u8, u8)> {
    let h = text.strip_prefix('#')?;
    let h: String = if h.len() == 3 {
        h.chars().flat_map(|c| [c, c]).collect()
    } else {
        h.to_string()
    };

    if h.len() != 6 {
        return None;
    }

    let n = u32::from_str_radix(&h, 16).ok()?;

    Some(((n >> 16) as u8, (n >> 8 & 0xff) as u8, (n & 0xff) as u8))
}

fn rgb(text: &str) -> Option<(u8, u8, u8)> {
    let inner = text.strip_prefix("rgb(")?.strip_suffix(')')?;
    let parts: Vec<u8> = inner
        .split(',')
        .map(|p| p.trim().parse().ok())
        .collect::<Option<_>>()?;

    (parts.len() == 3).then(|| (parts[0], parts[1], parts[2]))
}

fn text_size(word: &str) -> Option<f64> {
    Some(match word {
        "xs" => 12.0,
        "sm" => 14.0,
        "base" => 16.0,
        "lg" => 18.0,
        "xl" => 20.0,
        "2xl" => 24.0,
        "3xl" => 30.0,
        "4xl" => 36.0,
        "5xl" => 48.0,
        "6xl" => 60.0,
        "7xl" => 72.0,
        "8xl" => 96.0,
        "9xl" => 128.0,
        _ => return arbitrary(word).and_then(length),
    })
}

fn radius(word: &str) -> Option<Dim> {
    Some(match word {
        "" => Dim::px(4.0),
        "none" => Dim::px(0.0),
        "sm" => Dim::px(2.0),
        "md" => Dim::px(6.0),
        "lg" => Dim::px(8.0),
        "xl" => Dim::px(12.0),
        "2xl" => Dim::px(16.0),
        "3xl" => Dim::px(24.0),
        "full" => Dim::scale(1.0),
        _ => {
            let inner = arbitrary(word)?;

            match inner.strip_suffix('%') {
                Some(p) => Dim::scale(p.parse::<f64>().ok()? / 100.0),
                None => Dim::px(length(inner)?),
            }
        }
    })
}

fn max_width(word: &str) -> Option<f64> {
    Some(match word {
        "xs" => 320.0,
        "sm" => 384.0,
        "md" => 448.0,
        "lg" => 512.0,
        "xl" => 576.0,
        "2xl" => 672.0,
        "3xl" => 768.0,
        "4xl" => 896.0,
        "5xl" => 1024.0,
        "6xl" => 1152.0,
        "7xl" => 1280.0,
        "none" | "full" => f64::INFINITY,
        _ => spacing(word)?,
    })
}

fn weight(word: &str) -> Option<&'static str> {
    Some(match word {
        "thin" => "Thin",
        "extralight" => "ExtraLight",
        "light" => "Light",
        "normal" => "Regular",
        "medium" => "Medium",
        "semibold" => "SemiBold",
        "bold" => "Bold",
        "extrabold" => "ExtraBold",
        "black" => "Heavy",
        _ => return None,
    })
}

fn leading(word: &str) -> Option<f64> {
    Some(match word {
        "none" => 1.0,
        "tight" => 1.25,
        "snug" => 1.375,
        "normal" => 1.5,
        "relaxed" => 1.625,
        "loose" => 2.0,
        // In brackets the multiplier is as written; a numeric leading is
        // a length on the web, and against the base text size a ratio.
        _ if arbitrary(word).is_some() => arbitrary(word)?.parse().ok()?,
        _ => spacing(word)? / 16.0,
    })
}

fn gradient_rotation(word: &str) -> Option<f64> {
    Some(match word {
        "r" => 0.0,
        "br" => 45.0,
        "b" => 90.0,
        "bl" => 135.0,
        "l" => 180.0,
        "tl" => 225.0,
        "t" => 270.0,
        "tr" => 315.0,
        _ => return None,
    })
}

/// The font families, from the options.
#[derive(Debug, Clone)]
pub struct Fonts {
    pub sans: String,
    pub serif: String,
    pub mono: String,
}

impl Default for Fonts {
    fn default() -> Self {
        Self {
            sans: "rbxasset://fonts/families/GothamSSm.json".into(),
            serif: "rbxasset://fonts/families/Merriweather.json".into(),
            mono: "rbxasset://fonts/families/RobotoMono.json".into(),
        }
    }
}

/// What the utilities read: the font options and the project's theme.
#[derive(Debug, Clone, Default)]
pub struct Context {
    pub fonts: Fonts,
    pub theme: Theme,
}

/// The utility a base name (the token without variants and sign) names.
pub fn parse(class: &Class, ctx: &Context) -> Option<Utility> {
    parse_depth(class, ctx, 0)
}

#[allow(clippy::too_many_lines)]
fn parse_depth(class: &Class, ctx: &Context, depth: usize) -> Option<Utility> {
    let fonts = &ctx.fonts;
    let theme = &ctx.theme;
    let base = class.base.as_str();

    // A class of the theme: a list of utilities, or properties as written.
    if let Some((def, _)) = theme.classes.get(base)
        && depth < 8
    {
        return Some(match def {
            ClassDef::Classes(list) => {
                let mut pieces = Vec::new();

                for word in list.split_whitespace() {
                    if let Some(u) = parse_depth(&Class::parse(word), ctx, depth + 1) {
                        pieces.extend(u.pieces);
                    }
                }

                pieces.push(Piece::Theme);

                Utility {
                    pieces,
                    summary: format!("the theme's `{base}`: {list}"),
                    color: None,
                }
            }

            ClassDef::Props(props) => {
                let mut pieces: Vec<Piece> = props
                    .iter()
                    .map(|(k, v)| prop(k, v.clone(), Needs::Any))
                    .collect();
                pieces.push(Piece::Theme);

                Utility {
                    pieces,
                    summary: format!("the theme's `{base}`"),
                    color: None,
                }
            }
        });
    }

    let neg = if class.negative { -1.0 } else { 1.0 };
    let no = |what: &'static str| Some(utility(vec![Piece::NoEffect(what)], what));

    // Exact words first.
    let exact = match base {
        "hidden" => Some(utility(
            vec![
                prop("Visible", "false", Needs::Gui),
                prop("Enabled", "false", Needs::Layer),
            ],
            "hides the element",
        )),
        "visible" | "block" | "inline" | "inline-block" => Some(utility(
            vec![
                prop("Visible", "true", Needs::Gui),
                prop("Enabled", "true", Needs::Layer),
            ],
            "shows the element",
        )),
        "flex" | "inline-flex" | "flex-row" => Some(utility(
            vec![Piece::Layout(
                "FillDirection",
                "Enum.FillDirection.Horizontal".into(),
            )],
            "a UIListLayout that lays the children out in a row",
        )),
        "flex-col" => Some(utility(
            vec![Piece::Layout(
                "FillDirection",
                "Enum.FillDirection.Vertical".into(),
            )],
            "a UIListLayout that lays the children out in a column",
        )),
        "flex-wrap" => Some(utility(
            vec![Piece::Layout("Wraps", "true".into())],
            "the list wraps onto new lines",
        )),
        "flex-nowrap" => Some(utility(
            vec![Piece::Layout("Wraps", "false".into())],
            "the list stays on one line",
        )),
        "grid" => Some(utility(
            vec![Piece::Layout("__grid", "true".into())],
            "a UIGridLayout for the children",
        )),
        "justify-start" => Some(utility(
            vec![Piece::Justify("start")],
            "children packed at the start",
        )),
        "justify-center" => Some(utility(
            vec![Piece::Justify("center")],
            "children packed at the center",
        )),
        "justify-end" => Some(utility(
            vec![Piece::Justify("end")],
            "children packed at the end",
        )),
        "justify-between" => Some(utility(
            vec![Piece::Justify("between")],
            "space between the children",
        )),
        "justify-around" => Some(utility(
            vec![Piece::Justify("around")],
            "space around the children",
        )),
        "justify-evenly" => Some(utility(
            vec![Piece::Justify("evenly")],
            "even space around the children",
        )),
        "justify-stretch" => Some(utility(
            vec![Piece::Justify("fill")],
            "children stretched along the list",
        )),
        "items-start" => Some(utility(
            vec![Piece::Items("start")],
            "children aligned at the start of the cross axis",
        )),
        "items-center" => Some(utility(
            vec![Piece::Items("center")],
            "children centered on the cross axis",
        )),
        "items-end" => Some(utility(
            vec![Piece::Items("end")],
            "children aligned at the end of the cross axis",
        )),
        "items-stretch" => Some(utility(
            vec![Piece::Items("fill")],
            "children stretched across the cross axis",
        )),
        "grow" | "flex-1" | "flex-auto" => Some(utility(
            vec![Piece::FlexItem("FlexMode", "Enum.UIFlexMode.Grow".into())],
            "a UIFlexItem that grows to fill the list",
        )),
        "grow-0" | "flex-none" => Some(utility(
            vec![Piece::FlexItem("FlexMode", "Enum.UIFlexMode.None".into())],
            "a UIFlexItem that keeps its size",
        )),
        "shrink" => Some(utility(
            vec![Piece::FlexItem("FlexMode", "Enum.UIFlexMode.Shrink".into())],
            "a UIFlexItem that shrinks to fit the list",
        )),
        "self-start" => Some(utility(
            vec![Piece::FlexItem(
                "ItemLineAlignment",
                "Enum.ItemLineAlignment.Start".into(),
            )],
            "this child at the start of its line",
        )),
        "self-center" => Some(utility(
            vec![Piece::FlexItem(
                "ItemLineAlignment",
                "Enum.ItemLineAlignment.Center".into(),
            )],
            "this child centered on its line",
        )),
        "self-end" => Some(utility(
            vec![Piece::FlexItem(
                "ItemLineAlignment",
                "Enum.ItemLineAlignment.End".into(),
            )],
            "this child at the end of its line",
        )),
        "self-stretch" => Some(utility(
            vec![Piece::FlexItem(
                "ItemLineAlignment",
                "Enum.ItemLineAlignment.Stretch".into(),
            )],
            "this child stretched across its line",
        )),
        "appearance-none" => Some(utility(
            Vec::new(),
            "Silk drops the look a browser gives a control; nothing to set",
        )),
        "absolute" | "relative" | "fixed" | "static" | "sticky" => Some(utility(
            Vec::new(),
            "positions are always absolute on Roblox; nothing to set",
        )),
        "inset-0" => Some(utility(
            vec![
                Piece::PosX {
                    dim: Dim::px(0.0),
                    from_end: false,
                },
                Piece::PosY {
                    dim: Dim::px(0.0),
                    from_end: false,
                },
                Piece::SizeX(Dim::scale(1.0)),
                Piece::SizeY(Dim::scale(1.0)),
            ],
            "fills the parent",
        )),
        "inset-x-0" => Some(utility(
            vec![
                Piece::PosX {
                    dim: Dim::px(0.0),
                    from_end: false,
                },
                Piece::SizeX(Dim::scale(1.0)),
            ],
            "fills the parent's width",
        )),
        "inset-y-0" => Some(utility(
            vec![
                Piece::PosY {
                    dim: Dim::px(0.0),
                    from_end: false,
                },
                Piece::SizeY(Dim::scale(1.0)),
            ],
            "fills the parent's height",
        )),
        "overflow-hidden" | "overflow-clip" => Some(utility(
            vec![prop("ClipsDescendants", "true", Needs::Gui)],
            "clips the children to the element",
        )),
        "overflow-visible" => Some(utility(
            vec![prop("ClipsDescendants", "false", Needs::Gui)],
            "children may draw outside the element",
        )),
        "overflow-auto" | "overflow-scroll" | "overflow-y-auto" | "overflow-y-scroll"
        | "overflow-x-auto" | "overflow-x-scroll" => Some(utility(
            vec![prop("ScrollingEnabled", "true", Needs::Scrolling)],
            "the frame scrolls",
        )),
        "scrollbar-none" => Some(utility(
            vec![prop("ScrollBarThickness", "0", Needs::Scrolling)],
            "no scroll bar",
        )),
        "truncate" | "text-ellipsis" => Some(utility(
            vec![prop("TextTruncate", "Enum.TextTruncate.AtEnd", Needs::Text)],
            "text past the box ends in an ellipsis",
        )),
        "text-clip" => Some(utility(
            vec![prop("TextTruncate", "Enum.TextTruncate.None", Needs::Text)],
            "text past the box is cut",
        )),
        "text-wrap" | "whitespace-normal" => Some(utility(
            vec![prop("TextWrapped", "true", Needs::Text)],
            "text wraps in the box",
        )),
        "text-nowrap" | "whitespace-nowrap" => Some(utility(
            vec![prop("TextWrapped", "false", Needs::Text)],
            "text stays on one line",
        )),
        "text-scaled" => Some(utility(
            vec![prop("TextScaled", "true", Needs::Text)],
            "text scales to the box",
        )),
        "text-left" => Some(utility(
            vec![prop(
                "TextXAlignment",
                "Enum.TextXAlignment.Left",
                Needs::Text,
            )],
            "text at the left",
        )),
        "text-center" => Some(utility(
            vec![prop(
                "TextXAlignment",
                "Enum.TextXAlignment.Center",
                Needs::Text,
            )],
            "text centered",
        )),
        "text-right" => Some(utility(
            vec![prop(
                "TextXAlignment",
                "Enum.TextXAlignment.Right",
                Needs::Text,
            )],
            "text at the right",
        )),
        "text-justify" => Some(utility(
            vec![prop(
                "TextXAlignment",
                "Enum.TextXAlignment.Left",
                Needs::Text,
            )],
            "text at the left; Roblox does not justify",
        )),
        "align-top" => Some(utility(
            vec![prop(
                "TextYAlignment",
                "Enum.TextYAlignment.Top",
                Needs::Text,
            )],
            "text at the top",
        )),
        "align-middle" => Some(utility(
            vec![prop(
                "TextYAlignment",
                "Enum.TextYAlignment.Center",
                Needs::Text,
            )],
            "text at the middle",
        )),
        "align-bottom" => Some(utility(
            vec![prop(
                "TextYAlignment",
                "Enum.TextYAlignment.Bottom",
                Needs::Text,
            )],
            "text at the bottom",
        )),
        "rich-text" | "rich" => Some(utility(
            vec![prop("RichText", "true", Needs::Text)],
            "the text is rich text",
        )),
        "italic" => Some(utility(
            vec![prop("__italic", "true", Needs::Text)],
            "italic text",
        )),
        "not-italic" => Some(utility(
            vec![prop("__italic", "false", Needs::Text)],
            "upright text",
        )),
        "font-sans" => Some(utility(
            vec![prop("__family", fonts.sans.clone(), Needs::Text)],
            "the sans-serif family",
        )),
        "font-serif" => Some(utility(
            vec![prop("__family", fonts.serif.clone(), Needs::Text)],
            "the serif family",
        )),
        "font-mono" => Some(utility(
            vec![prop("__family", fonts.mono.clone(), Needs::Text)],
            "the monospace family",
        )),
        "bg-transparent" => Some(utility(
            vec![prop("BackgroundTransparency", "1", Needs::Gui)],
            "no background",
        )),
        "bg-none" => Some(utility(Vec::new(), "no background image; nothing to set")),
        "text-transparent" => Some(utility(
            vec![prop("TextTransparency", "1", Needs::Text)],
            "invisible text",
        )),
        "border-transparent" | "ring-transparent" => Some(utility(
            vec![Piece::Stroke("Transparency", "1".into())],
            "an invisible stroke",
        )),
        "border" | "ring" => Some(utility(
            vec![Piece::Stroke("Thickness", "1".into())],
            "a UIStroke one pixel wide",
        )),
        "border-0" | "ring-0" => Some(utility(
            vec![Piece::Stroke("Thickness", "0".into())],
            "no stroke",
        )),
        "rounded" => Some(utility(
            vec![Piece::Corner(Dim::px(4.0))],
            "a UICorner of 4 pixels",
        )),
        "rounded-none" => Some(utility(vec![Piece::Corner(Dim::px(0.0))], "square corners")),
        "rounded-full" => Some(utility(
            vec![Piece::Corner(Dim::scale(1.0))],
            "fully round corners",
        )),
        "aspect-square" => Some(utility(
            vec![Piece::Aspect(1.0)],
            "a UIAspectRatioConstraint of 1",
        )),
        "aspect-video" => Some(utility(
            vec![Piece::Aspect(16.0 / 9.0)],
            "a UIAspectRatioConstraint of 16:9",
        )),
        "aspect-auto" => Some(utility(Vec::new(), "no aspect ratio; nothing to set")),
        "object-cover" => Some(utility(
            vec![prop("ScaleType", "Enum.ScaleType.Crop", Needs::Image)],
            "the image crops to fill",
        )),
        "object-contain" => Some(utility(
            vec![prop("ScaleType", "Enum.ScaleType.Fit", Needs::Image)],
            "the image fits inside",
        )),
        "object-fill" => Some(utility(
            vec![prop("ScaleType", "Enum.ScaleType.Stretch", Needs::Image)],
            "the image stretches to fill",
        )),
        "object-none" => Some(utility(
            vec![prop("ScaleType", "Enum.ScaleType.Tile", Needs::Image)],
            "the image tiles",
        )),
        "pointer-events-none" => Some(utility(
            vec![prop("Active", "false", Needs::Gui)],
            "input passes through the element",
        )),
        "pointer-events-auto" => Some(utility(
            vec![prop("Active", "true", Needs::Gui)],
            "the element takes input",
        )),
        "select-none" => Some(utility(
            vec![prop("Selectable", "false", Needs::Gui)],
            "gamepad selection skips the element",
        )),
        "select-auto" | "select-text" => Some(utility(
            vec![prop("Selectable", "true", Needs::Gui)],
            "gamepad selection reaches the element",
        )),
        "group" => Some(utility(
            vec![Piece::Group],
            "marks the element for `group-hover:` on its descendants",
        )),
        "transition" => Some(utility(
            vec![Piece::Transition("default")],
            "a state change tweens colors, transparencies, position, size, and rotation, 150ms ease-in-out unless `duration-*` and `ease-*` say otherwise",
        )),
        "order-first" => Some(utility(
            vec![prop("LayoutOrder", "-9999", Needs::Gui)],
            "first in the layout",
        )),
        "order-last" => Some(utility(
            vec![prop("LayoutOrder", "9999", Needs::Gui)],
            "last in the layout",
        )),
        "order-none" => Some(utility(
            vec![prop("LayoutOrder", "0", Needs::Gui)],
            "layout order 0",
        )),
        "z-auto" => Some(utility(Vec::new(), "the default ZIndex; nothing to set")),
        "w-auto" | "w-fit" | "w-max" | "w-min" => {
            Some(utility(vec![Piece::AutoX], "the width follows the content"))
        }
        "h-auto" | "h-fit" | "h-max" | "h-min" => Some(utility(
            vec![Piece::AutoY],
            "the height follows the content",
        )),
        "size-auto" | "size-fit" => Some(utility(
            vec![Piece::AutoX, Piece::AutoY],
            "the size follows the content",
        )),
        "clip" => Some(utility(
            vec![prop("ClipsDescendants", "true", Needs::Gui)],
            "clips the children to the element",
        )),
        "no-clip" => Some(utility(
            vec![prop("ClipsDescendants", "false", Needs::Gui)],
            "children may draw outside the element",
        )),
        "anchor-center" => Some(utility(
            vec![Piece::AnchorX(0.5), Piece::AnchorY(0.5)],
            "anchors at the center",
        )),
        "anchor-tl" => Some(utility(
            vec![Piece::AnchorX(0.0), Piece::AnchorY(0.0)],
            "anchors at the top left",
        )),
        "anchor-t" => Some(utility(
            vec![Piece::AnchorX(0.5), Piece::AnchorY(0.0)],
            "anchors at the top center",
        )),
        "anchor-tr" => Some(utility(
            vec![Piece::AnchorX(1.0), Piece::AnchorY(0.0)],
            "anchors at the top right",
        )),
        "anchor-l" => Some(utility(
            vec![Piece::AnchorX(0.0), Piece::AnchorY(0.5)],
            "anchors at the left",
        )),
        "anchor-r" => Some(utility(
            vec![Piece::AnchorX(1.0), Piece::AnchorY(0.5)],
            "anchors at the right",
        )),
        "anchor-bl" => Some(utility(
            vec![Piece::AnchorX(0.0), Piece::AnchorY(1.0)],
            "anchors at the bottom left",
        )),
        "anchor-b" => Some(utility(
            vec![Piece::AnchorX(0.5), Piece::AnchorY(1.0)],
            "anchors at the bottom center",
        )),
        "anchor-br" => Some(utility(
            vec![Piece::AnchorX(1.0), Piece::AnchorY(1.0)],
            "anchors at the bottom right",
        )),
        "center" => Some(utility(
            vec![
                Piece::PosX {
                    dim: Dim::scale(0.5),
                    from_end: false,
                },
                Piece::PosY {
                    dim: Dim::scale(0.5),
                    from_end: false,
                },
                Piece::AnchorX(0.5),
                Piece::AnchorY(0.5),
            ],
            "centers the element in its parent",
        )),
        "stroke" => Some(utility(
            vec![Piece::Stroke("Thickness", "1".into())],
            "a UIStroke one pixel wide",
        )),
        "stroke-0" => Some(utility(
            vec![Piece::Stroke("Thickness", "0".into())],
            "no stroke",
        )),
        "stroke-transparent" => Some(utility(
            vec![Piece::Stroke("Transparency", "1".into())],
            "an invisible stroke",
        )),
        "stroke-contextual" => Some(utility(
            vec![Piece::Stroke(
                "ApplyStrokeMode",
                "Enum.ApplyStrokeMode.Contextual".into(),
            )],
            "the stroke follows the text, not the border",
        )),
        "stroke-border" => Some(utility(
            vec![Piece::Stroke(
                "ApplyStrokeMode",
                "Enum.ApplyStrokeMode.Border".into(),
            )],
            "the stroke follows the border",
        )),
        "stroke-round" => Some(utility(
            vec![Piece::Stroke(
                "LineJoinMode",
                "Enum.LineJoinMode.Round".into(),
            )],
            "round stroke joins",
        )),
        "stroke-bevel" => Some(utility(
            vec![Piece::Stroke(
                "LineJoinMode",
                "Enum.LineJoinMode.Bevel".into(),
            )],
            "beveled stroke joins",
        )),
        "stroke-miter" => Some(utility(
            vec![Piece::Stroke(
                "LineJoinMode",
                "Enum.LineJoinMode.Miter".into(),
            )],
            "mitered stroke joins",
        )),
        "text-stroke-transparent" => Some(utility(
            vec![prop("TextStrokeTransparency", "1", Needs::Text)],
            "no text outline",
        )),
        "sort-name" => Some(utility(
            vec![Piece::Layout("SortOrder", "Enum.SortOrder.Name".into())],
            "children sorted by name",
        )),
        "sort-order" => Some(utility(
            vec![Piece::Layout(
                "SortOrder",
                "Enum.SortOrder.LayoutOrder".into(),
            )],
            "children sorted by LayoutOrder",
        )),
        "fill" => Some(utility(
            vec![Piece::FlexItem("FlexMode", "Enum.UIFlexMode.Fill".into())],
            "a UIFlexItem that fills the list",
        )),
        "image-slice" => Some(utility(
            vec![prop("ScaleType", "Enum.ScaleType.Slice", Needs::Image)],
            "the image nine-slices",
        )),
        "image-tile" => Some(utility(
            vec![prop("ScaleType", "Enum.ScaleType.Tile", Needs::Image)],
            "the image tiles",
        )),
        "image-transparent" => Some(utility(
            vec![prop("ImageTransparency", "1", Needs::Image)],
            "an invisible image",
        )),
        "scroll-x" => Some(utility(
            vec![prop(
                "ScrollingDirection",
                "Enum.ScrollingDirection.X",
                Needs::Scrolling,
            )],
            "scrolls sideways only",
        )),
        "scroll-y" => Some(utility(
            vec![prop(
                "ScrollingDirection",
                "Enum.ScrollingDirection.Y",
                Needs::Scrolling,
            )],
            "scrolls up and down only",
        )),
        "scroll-xy" => Some(utility(
            vec![prop(
                "ScrollingDirection",
                "Enum.ScrollingDirection.XY",
                Needs::Scrolling,
            )],
            "scrolls both ways",
        )),
        "no-scroll" => Some(utility(
            vec![prop("ScrollingEnabled", "false", Needs::Scrolling)],
            "the frame does not scroll",
        )),
        "canvas-auto" => Some(utility(
            vec![prop(
                "AutomaticCanvasSize",
                "Enum.AutomaticSize.Y",
                Needs::Scrolling,
            )],
            "the canvas grows with the content",
        )),
        "canvas-auto-x" => Some(utility(
            vec![prop(
                "AutomaticCanvasSize",
                "Enum.AutomaticSize.X",
                Needs::Scrolling,
            )],
            "the canvas grows sideways with the content",
        )),
        "canvas-auto-xy" => Some(utility(
            vec![prop(
                "AutomaticCanvasSize",
                "Enum.AutomaticSize.XY",
                Needs::Scrolling,
            )],
            "the canvas grows both ways",
        )),
        "elastic" => Some(utility(
            vec![prop(
                "ElasticBehavior",
                "Enum.ElasticBehavior.Always",
                Needs::Scrolling,
            )],
            "the canvas overscrolls",
        )),
        "no-elastic" => Some(utility(
            vec![prop(
                "ElasticBehavior",
                "Enum.ElasticBehavior.Never",
                Needs::Scrolling,
            )],
            "the canvas stops at its edge",
        )),
        "auto-color" => Some(utility(
            vec![prop("AutoButtonColor", "true", Needs::Gui)],
            "the button darkens when pressed",
        )),
        "no-auto-color" => Some(utility(
            vec![prop("AutoButtonColor", "false", Needs::Gui)],
            "the button keeps its color when pressed",
        )),
        "modal" => Some(utility(
            vec![prop("Modal", "true", Needs::Gui)],
            "the button frees the mouse",
        )),
        "ignore-inset" => Some(utility(
            vec![prop("IgnoreGuiInset", "true", Needs::Layer)],
            "the gui covers the top bar",
        )),
        "reset-on-spawn" => Some(utility(
            vec![prop("ResetOnSpawn", "true", Needs::Layer)],
            "the gui resets when the character spawns",
        )),
        "keep-on-spawn" => Some(utility(
            vec![prop("ResetOnSpawn", "false", Needs::Layer)],
            "the gui survives a respawn",
        )),
        "sibling-z" => Some(utility(
            vec![prop(
                "ZIndexBehavior",
                "Enum.ZIndexBehavior.Sibling",
                Needs::Layer,
            )],
            "ZIndex counts among siblings",
        )),
        "global-z" => Some(utility(
            vec![prop(
                "ZIndexBehavior",
                "Enum.ZIndexBehavior.Global",
                Needs::Layer,
            )],
            "ZIndex counts across the gui",
        )),
        "uppercase" | "lowercase" | "capitalize" | "normal-case" => {
            no("a text transform runs at render time on the web; set the text itself")
        }
        "underline" | "line-through" | "no-underline" | "overline" => {
            no("Roblox has no text decoration; use rich text")
        }
        _ => None,
    };

    if exact.is_some() {
        return exact;
    }

    // A compound head first, `bg-transparency-50`; then the plain one.
    const COMPOUND: &[&str] = &[
        "bg-transparency",
        "bg-opacity",
        "text-transparency",
        "text-opacity",
        "stroke-transparency",
        "border-transparency",
        "border-opacity",
        "image-transparency",
        "image-opacity",
        "group-transparency",
        "text-stroke-transparency",
        "text-stroke",
        "text-size",
        "max-graphemes",
    ];
    let (head, rest) = COMPOUND
        .iter()
        .find_map(|h| {
            base.strip_prefix(h)
                .and_then(|r| r.strip_prefix('-'))
                .map(|r| (*h, r))
        })
        .or_else(|| base.split_once('-'))?;

    match head {
        "transition" => {
            let (kind, what) = match rest {
                "all" => ("all", "every property a tween can move"),
                "colors" => ("colors", "the colors"),
                "opacity" => ("opacity", "the transparencies"),
                "transform" => ("transform", "position, size, anchor, and rotation"),
                "none" => (
                    "none",
                    "nothing: a state change sets its properties at once",
                ),
                _ => return None,
            };

            Some(utility(
                vec![Piece::Transition(kind)],
                format!("a state change tweens {what}"),
            ))
        }
        "duration" => {
            let secs = seconds(rest)?;

            Some(utility(
                vec![Piece::Duration(secs)],
                format!("a state change tweens over {}s", num(secs)),
            ))
        }
        "delay" => {
            let secs = seconds(rest)?;

            Some(utility(
                vec![Piece::Delay(secs)],
                format!("a state change waits {}s before its tween", num(secs)),
            ))
        }
        "ease" => {
            let (style, direction) = easing(rest)?;

            Some(utility(
                vec![Piece::Ease(style, direction)],
                format!(
                    "a state change tweens with Enum.EasingStyle.{style}, Enum.EasingDirection.{direction}"
                ),
            ))
        }
        "bg-transparency"
        | "bg-opacity"
        | "text-transparency"
        | "text-opacity"
        | "stroke-transparency"
        | "border-transparency"
        | "border-opacity"
        | "image-transparency"
        | "image-opacity"
        | "group-transparency"
        | "text-stroke-transparency"
        | "transparency" => {
            let n = ratio(rest)?;
            let value = if head.ends_with("opacity") {
                1.0 - n
            } else {
                n
            };
            let t = num(value);
            let pieces = match head {
                "bg-transparency" | "bg-opacity" => {
                    vec![prop("BackgroundTransparency", t.clone(), Needs::Gui)]
                }
                "text-transparency" | "text-opacity" => {
                    vec![prop("TextTransparency", t.clone(), Needs::Text)]
                }
                "image-transparency" | "image-opacity" => {
                    vec![prop("ImageTransparency", t.clone(), Needs::Image)]
                }
                "group-transparency" => vec![prop("GroupTransparency", t.clone(), Needs::Canvas)],
                "text-stroke-transparency" => {
                    vec![prop("TextStrokeTransparency", t.clone(), Needs::Text)]
                }
                "transparency" => vec![
                    prop("BackgroundTransparency", t.clone(), Needs::Gui),
                    prop("TextTransparency", t.clone(), Needs::Text),
                    prop("ImageTransparency", t.clone(), Needs::Image),
                ],
                _ => vec![Piece::Stroke("Transparency", t.clone())],
            };

            Some(utility(
                pieces,
                format!(
                    "{} transparency {t}",
                    head.split('-').next().unwrap_or(head)
                ),
            ))
        }
        "stroke" => {
            if let Some(n) = arbitrary(rest)
                .and_then(length)
                .or_else(|| rest.parse().ok())
            {
                return Some(utility(
                    vec![Piece::Stroke("Thickness", num(n))],
                    format!("a UIStroke {} pixels wide", num(n)),
                ));
            }

            let c = color(rest, theme)?;
            let mut pieces = vec![Piece::Stroke("Color", c.expr.clone())];

            if c.alpha < 1.0 {
                pieces.push(Piece::Stroke("Transparency", num(1.0 - c.alpha)));
            }

            Some(c.utility(
                pieces,
                format!("a UIStroke colored {}", color_words(&c)),
                c.alpha,
            ))
        }
        "text-stroke" => {
            let c = color(rest, theme)?;
            let pieces = vec![
                prop("TextStrokeColor3", c.expr.clone(), Needs::Text),
                prop("TextStrokeTransparency", num(1.0 - c.alpha), Needs::Text),
            ];

            Some(c.utility(
                pieces,
                format!("a text outline colored {}", color_words(&c)),
                c.alpha,
            ))
        }
        "text-size" => {
            let n = arbitrary(rest)
                .and_then(length)
                .or_else(|| rest.parse().ok())?;

            Some(utility(
                vec![prop("TextSize", num(n), Needs::Text)],
                format!("text size {}", num(n)),
            ))
        }
        "layout" | "display" => {
            let n = number(rest)?;
            let n = n * neg;

            Some(utility(
                vec![
                    prop("LayoutOrder", num(n), Needs::Gui),
                    prop("DisplayOrder", num(n), Needs::Layer),
                ],
                format!("order {}", num(n)),
            ))
        }
        "canvas" => {
            let (axis, value) = rest.split_once('-')?;
            // A canvas is measured in pixels, as the Explorer shows it.
            let d = fraction(value).map(Dim::scale).or_else(|| {
                arbitrary(value)
                    .and_then(length)
                    .or_else(|| value.parse().ok())
                    .map(Dim::px)
            })?;
            let (x, y) = match axis {
                "w" => (d, Dim::px(0.0)),
                "h" => (Dim::px(0.0), d),
                _ => return None,
            };

            Some(utility(
                vec![prop(
                    "CanvasSize",
                    format!(
                        "UDim2.new({}, {}, {}, {})",
                        num(x.scale),
                        num(x.offset),
                        num(y.scale),
                        num(y.offset)
                    ),
                    Needs::Scrolling,
                )],
                format!("canvas {axis} {}", dim_words(d)),
            ))
        }
        "max-graphemes" => {
            let n = number(rest)?;

            Some(utility(
                vec![prop("MaxVisibleGraphemes", num(n), Needs::Text)],
                format!("{} visible graphemes", num(n)),
            ))
        }
        "w" => Some(utility(
            vec![Piece::SizeX(dim(rest)?)],
            format!("width {}", dim_words(dim(rest)?)),
        )),
        "h" => Some(utility(
            vec![Piece::SizeY(dim(rest)?)],
            format!("height {}", dim_words(dim(rest)?)),
        )),
        "size" => {
            let d = dim(rest)?;

            Some(utility(
                vec![Piece::SizeX(d), Piece::SizeY(d)],
                format!("width and height {}", dim_words(d)),
            ))
        }
        "min" => {
            let (axis, value) = rest.split_once('-')?;
            let (field, ax) = match axis {
                "w" => ("MinSize", "X"),
                "h" => ("MinSize", "Y"),
                _ => return None,
            };
            let px = spacing(value)?;

            Some(utility(
                vec![Piece::SizeConstraint(field, ax, px)],
                format!("a UISizeConstraint minimum of {} pixels", num(px)),
            ))
        }
        "max" => {
            let (axis, value) = rest.split_once('-')?;
            let (field, ax) = match axis {
                "w" => ("MaxSize", "X"),
                "h" => ("MaxSize", "Y"),
                _ => return None,
            };
            let px = max_width(value)?;

            Some(utility(
                vec![Piece::SizeConstraint(field, ax, px)],
                format!("a UISizeConstraint maximum of {} pixels", num(px)),
            ))
        }
        "p" | "px" | "py" | "pt" | "pr" | "pb" | "pl" => {
            let px = spacing(rest)?;
            let sides: &[&'static str] = match head {
                "p" => &["PaddingTop", "PaddingRight", "PaddingBottom", "PaddingLeft"],
                "px" => &["PaddingLeft", "PaddingRight"],
                "py" => &["PaddingTop", "PaddingBottom"],
                "pt" => &["PaddingTop"],
                "pr" => &["PaddingRight"],
                "pb" => &["PaddingBottom"],
                _ => &["PaddingLeft"],
            };

            Some(utility(
                sides.iter().map(|s| Piece::Padding(s, px)).collect(),
                format!("a UIPadding of {} pixels", num(px)),
            ))
        }
        "m" | "mx" | "my" | "mt" | "mr" | "mb" | "ml" => {
            spacing(rest)?;

            no("a margin has no property on a GuiObject; `gap-*` on the parent spaces the children")
        }
        "space" => {
            no("a margin has no property on a GuiObject; `gap-*` on the parent spaces the children")
        }
        "gap" => {
            if let Some((axis, value)) = rest.split_once('-')
                && matches!(axis, "x" | "y")
            {
                let px = spacing(value)?;
                let (x, y) = if axis == "x" {
                    (Some(px), None)
                } else {
                    (None, Some(px))
                };

                return Some(utility(
                    vec![Piece::Gap { x, y }],
                    format!("{} pixels between the children", num(px)),
                ));
            }

            let px = spacing(rest)?;

            Some(utility(
                vec![Piece::Gap {
                    x: Some(px),
                    y: Some(px),
                }],
                format!("{} pixels between the children", num(px)),
            ))
        }
        "grid" => {
            let (kind, n) = rest.split_once('-')?;
            let n = number(n)?;

            match kind {
                "cols" => Some(utility(
                    vec![
                        Piece::Layout("__grid", "true".into()),
                        Piece::Layout("__cols", num(n)),
                    ],
                    format!("a UIGridLayout with {} columns", num(n)),
                )),
                "rows" => Some(utility(
                    vec![
                        Piece::Layout("__grid", "true".into()),
                        Piece::Layout("__rows", num(n)),
                    ],
                    format!("a UIGridLayout with {} rows", num(n)),
                )),
                _ => None,
            }
        }
        "auto" => {
            let (kind, value) = rest.split_once('-')?;
            let px = spacing(value)?;

            match kind {
                "rows" => Some(utility(
                    vec![Piece::Layout("__row_height", num(px))],
                    format!("grid rows {} pixels tall", num(px)),
                )),
                "cols" => Some(utility(
                    vec![Piece::Layout("__col_width", num(px))],
                    format!("grid columns {} pixels wide", num(px)),
                )),
                _ => None,
            }
        }
        "top" | "bottom" | "left" | "right" => {
            let d = dim(rest)?;
            let d = Dim {
                scale: d.scale * neg,
                offset: d.offset * neg,
            };
            let piece = match head {
                "top" => Piece::PosY {
                    dim: d,
                    from_end: false,
                },
                "bottom" => Piece::PosY {
                    dim: d,
                    from_end: true,
                },
                "left" => Piece::PosX {
                    dim: d,
                    from_end: false,
                },
                _ => Piece::PosX {
                    dim: d,
                    from_end: true,
                },
            };

            Some(utility(vec![piece], format!("{head} {}", dim_words(d))))
        }
        "translate" => {
            let (axis, value) = rest.split_once('-')?;
            let f = fraction(value).or_else(|| arbitrary(value).and_then(|a| a.parse().ok()))?;
            let anchor = if class.negative { f } else { -f };
            let piece = match axis {
                "x" => Piece::AnchorX(anchor),
                "y" => Piece::AnchorY(anchor),
                _ => return None,
            };

            Some(utility(
                vec![piece],
                format!("anchors the {axis} axis at {}", num(anchor)),
            ))
        }
        "z" => {
            let n: f64 = arbitrary(rest).unwrap_or(rest).parse().ok()?;
            let n = n * neg;

            Some(utility(
                vec![
                    prop("ZIndex", num(n), Needs::Gui),
                    prop("DisplayOrder", num(n), Needs::Layer),
                ],
                format!("ZIndex {}", num(n)),
            ))
        }
        "order" => {
            let n = number(rest)?;

            Some(utility(
                vec![prop("LayoutOrder", num(n * neg), Needs::Gui)],
                format!("LayoutOrder {}", num(n * neg)),
            ))
        }
        "opacity" => {
            let n: f64 = arbitrary(rest).map_or_else(
                || rest.parse::<f64>().ok().map(|n| n / 100.0),
                |a| a.parse().ok(),
            )?;
            let t = num(1.0 - n);

            Some(utility(
                vec![
                    prop("BackgroundTransparency", t.clone(), Needs::Gui),
                    prop("TextTransparency", t.clone(), Needs::Text),
                    prop("ImageTransparency", t.clone(), Needs::Image),
                    prop("GroupTransparency", t.clone(), Needs::Canvas),
                ],
                format!("transparency {t}"),
            ))
        }
        "rotate" => {
            let deg: f64 = arbitrary(rest).map_or_else(
                || rest.parse().ok(),
                |a| a.strip_suffix("deg").unwrap_or(a).parse().ok(),
            )?;

            Some(utility(
                vec![prop("Rotation", num(deg * neg), Needs::Gui)],
                format!("rotation {} degrees", num(deg * neg)),
            ))
        }
        "scale" => {
            let n: f64 = arbitrary(rest).map_or_else(
                || rest.parse::<f64>().ok().map(|n| n / 100.0),
                |a| a.parse().ok(),
            )?;

            Some(utility(
                vec![Piece::Scale(n)],
                format!("a UIScale of {}", num(n)),
            ))
        }
        "aspect" => {
            let inner = arbitrary(rest)?;
            let ratio = fraction(inner).or_else(|| inner.parse().ok())?;

            Some(utility(
                vec![Piece::Aspect(ratio)],
                format!("a UIAspectRatioConstraint of {}", num(ratio)),
            ))
        }
        "rounded" => {
            let d = radius(rest)?;

            Some(utility(
                vec![Piece::Corner(d)],
                format!("a UICorner of {}", dim_words(d)),
            ))
        }
        "border" | "ring" | "outline" => {
            if let Some(n) = arbitrary(rest)
                .and_then(length)
                .or_else(|| rest.parse().ok())
            {
                return Some(utility(
                    vec![Piece::Stroke("Thickness", num(n))],
                    format!("a UIStroke {} pixels wide", num(n)),
                ));
            }

            let c = color(rest, theme)?;
            let mut pieces = vec![Piece::Stroke("Color", c.expr.clone())];

            if c.alpha < 1.0 {
                pieces.push(Piece::Stroke("Transparency", num(1.0 - c.alpha)));
            }

            Some(c.utility(
                pieces,
                format!("a UIStroke colored {}", color_words(&c)),
                c.alpha,
            ))
        }
        "bg" => {
            if let Some(dir) = rest.strip_prefix("gradient-to-") {
                let deg = gradient_rotation(dir)?;

                return Some(utility(
                    vec![Piece::GradientRotation(deg)],
                    format!("a UIGradient rotated {} degrees", num(deg)),
                ));
            }

            let c = color(rest, theme)?;
            let mut pieces = vec![prop("BackgroundColor3", c.expr.clone(), Needs::Gui)];

            if c.alpha < 1.0 {
                pieces.push(prop(
                    "BackgroundTransparency",
                    num(1.0 - c.alpha),
                    Needs::Gui,
                ));
            }

            Some(c.utility(pieces, format!("background {}", color_words(&c)), c.alpha))
        }
        "from" | "via" | "to" => {
            let c = color(rest, theme)?;
            let stop = if head == "from" {
                "from"
            } else if head == "via" {
                "via"
            } else {
                "to"
            };

            Some(c.utility(
                vec![Piece::GradientStop(stop, c.expr.clone())],
                format!("gradient stop {}", color_words(&c)),
                1.0,
            ))
        }
        "text" => {
            if let Some(size) = text_size(rest) {
                return Some(utility(
                    vec![prop("TextSize", num(size), Needs::Text)],
                    format!("text size {}", num(size)),
                ));
            }

            let c = color(rest, theme)?;
            let mut pieces = vec![prop("TextColor3", c.expr.clone(), Needs::Text)];

            if c.alpha < 1.0 {
                pieces.push(prop("TextTransparency", num(1.0 - c.alpha), Needs::Text));
            }

            Some(c.utility(pieces, format!("text {}", color_words(&c)), c.alpha))
        }
        "placeholder" => {
            let c = color(rest, theme)?;

            Some(c.utility(
                vec![prop("PlaceholderColor3", c.expr.clone(), Needs::TextBox)],
                format!("placeholder {}", color_words(&c)),
                1.0,
            ))
        }
        "image" => {
            if let Some(inner) = arbitrary(rest)
                && (inner.starts_with("rbxassetid://")
                    || inner.starts_with("rbxasset://")
                    || inner.starts_with("http"))
            {
                return Some(utility(
                    vec![prop("Image", format!("\"{inner}\""), Needs::Image)],
                    format!("the image `{inner}`"),
                ));
            }

            let c = color(rest, theme)?;
            let mut pieces = vec![prop("ImageColor3", c.expr.clone(), Needs::Image)];

            if c.alpha < 1.0 {
                pieces.push(prop("ImageTransparency", num(1.0 - c.alpha), Needs::Image));
            }

            Some(c.utility(pieces, format!("image tint {}", color_words(&c)), c.alpha))
        }
        "font" => {
            if let Some(w) = weight(rest) {
                return Some(utility(
                    vec![prop("__weight", w, Needs::Text)],
                    format!("font weight {w}"),
                ));
            }

            if let Some(entry) = theme.fonts.get(rest) {
                return Some(utility(
                    vec![
                        prop("__face", entry.expr.clone(), Needs::Text),
                        Piece::Theme,
                    ],
                    format!("the theme's font `{rest}`"),
                ));
            }

            if let Some(inner) = arbitrary(rest) {
                let family = if inner.contains("://") {
                    inner.replace('_', " ")
                } else {
                    fonts::family_path(inner)
                };

                return Some(utility(
                    vec![prop("__family", family.clone(), Needs::Text)],
                    format!("the family `{family}`"),
                ));
            }

            let f = fonts::lookup(rest)?;
            let mut pieces = vec![prop("__family", fonts::family_path(f.family), Needs::Text)];

            if f.weight != "Regular" {
                pieces.push(prop("__weight", f.weight, Needs::Text));
            }

            if f.italic {
                pieces.push(prop("__italic", "true", Needs::Text));
            }

            Some(utility(pieces, format!("the {} family", f.family)))
        }
        "leading" => {
            let n = leading(rest)?;

            Some(utility(
                vec![prop("LineHeight", num(n), Needs::Text)],
                format!("line height {}", num(n)),
            ))
        }
        "scrollbar" => {
            let n = spacing(rest)?;

            Some(utility(
                vec![prop("ScrollBarThickness", num(n), Needs::Scrolling)],
                format!("scroll bar {} pixels", num(n)),
            ))
        }
        "shadow" | "drop" | "blur" | "brightness" | "contrast" | "cursor" | "tracking"
        | "decoration" | "animate" | "backdrop" | "will" | "touch" | "resize" | "list"
        | "divide" | "outline-offset" | "ring-offset" | "content" | "float" | "clear"
        | "columns" | "break" | "box" | "isolate" | "mix" | "table" | "caption" | "appearance"
        | "accent" | "caret" | "scroll" | "snap" | "indent" | "align" | "whitespace"
        | "hyphens" | "sr" => no("this utility has no property on a Roblox instance"),
        _ => None,
    }
}

fn dim_words(d: Dim) -> String {
    match (d.scale != 0.0, d.offset != 0.0) {
        (true, false) => format!("{}% of the parent", num(d.scale * 100.0)),
        (false, _) => format!("{} pixels", num(d.offset)),
        (true, true) => format!(
            "{}% of the parent and {} pixels",
            num(d.scale * 100.0),
            num(d.offset)
        ),
    }
}

/// One property or child of the resolved element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Child {
    pub class: &'static str,
    pub props: Vec<(String, String)>,
}

/// A problem a class had on this element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Unknown,
    UnknownVariant(String),
    NoEffect(&'static str),
    WrongElement(String, Needs),
    /// A child or a marker under a variant: only a property can change
    /// with a state.
    VariantNeedsProperty,
}

/// Every class of one element, combined.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Resolved {
    pub props: Vec<(String, String)>,
    pub children: Vec<Child>,
    /// The properties of each state, `hover` and the rest.
    pub states: BTreeMap<&'static str, Vec<(String, String)>>,
    pub group: bool,
    /// The tween a state change plays, from `transition` and its
    /// settings.
    pub transition: Option<Transition>,
    /// Whether a value of the theme file is in use, so the file needs
    /// the theme's prelude.
    pub uses_theme: bool,
    /// The axes of `Size` the classes name, an automatic one at 0.
    pub size: (Option<Dim>, Option<Dim>),
    /// Problems by class index.
    pub problems: Vec<(usize, Problem)>,
}

/// The tween of a state change.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// What tweens: `default`, `all`, `colors`, `opacity`, or
    /// `transform`.
    pub kind: &'static str,
    pub duration: f64,
    pub style: &'static str,
    pub direction: &'static str,
    pub delay: f64,
}

impl Transition {
    /// The table the state helper takes.
    pub fn luau(&self) -> String {
        format!(
            "{{ kind = \"{}\", info = TweenInfo.new({}, Enum.EasingStyle.{}, Enum.EasingDirection.{}, 0, false, {}) }}",
            self.kind,
            num(self.duration),
            self.style,
            self.direction,
            num(self.delay)
        )
    }
}

/// Combines the classes of one element.
#[allow(clippy::too_many_lines)]
pub fn resolve(element: Element, classes: &[Class], ctx: &Context) -> Resolved {
    let fonts = &ctx.fonts;
    let mut out = Resolved::default();
    let mut size = (None::<Dim>, None::<Dim>);
    let mut auto = (false, false);
    let mut pos = (None::<(Dim, bool)>, None::<(Dim, bool)>);
    let mut anchor = (None::<f64>, None::<f64>);
    let mut layout: BTreeMap<&'static str, String> = BTreeMap::new();
    let mut justify = None;
    let mut items = None;
    let mut gap = (None::<f64>, None::<f64>);
    let mut padding: BTreeMap<&'static str, f64> = BTreeMap::new();
    let mut corner = None;
    let mut stroke: BTreeMap<&'static str, String> = BTreeMap::new();
    let mut stops: Vec<(&'static str, String)> = Vec::new();
    let mut rotation = None;
    let mut constraint: BTreeMap<(&'static str, &'static str), f64> = BTreeMap::new();
    let mut aspect = None;
    let mut scale = None;
    let mut flex: BTreeMap<&'static str, String> = BTreeMap::new();
    let mut font: BTreeMap<&'static str, String> = BTreeMap::new();
    let mut state_fonts: BTreeMap<&'static str, BTreeMap<&'static str, String>> = BTreeMap::new();
    // The child properties a state changes, by state and child class.
    let mut state_children: BTreeMap<(&'static str, &'static str), BTreeMap<&'static str, String>> =
        BTreeMap::new();
    let mut transition = None;
    let mut duration = None;
    let mut ease = None;
    let mut delay = None;

    for (i, class) in classes.iter().enumerate() {
        if let Some(v) = &class.unknown_variant {
            out.problems.push((i, Problem::UnknownVariant(v.clone())));

            continue;
        }

        let Some(u) = parse(class, ctx) else {
            out.problems.push((i, Problem::Unknown));

            continue;
        };

        let state = class.variants.last().map(|v| v.key());

        for piece in u.pieces {
            if let Some(state) = state {
                match piece {
                    Piece::Prop { name, value, needs } => {
                        if !needs.met_by(element) {
                            // A utility that sets a few properties by class
                            // is fine on any of them; none at all reports
                            // below.
                        } else if let Some(field) = name.strip_prefix("__") {
                            state_fonts
                                .entry(state)
                                .or_default()
                                .insert(field_key(field), value);
                        } else {
                            out.states.entry(state).or_default().push((name, value));
                        }
                    }

                    // The helper finds the child by its class and sets
                    // the property there.
                    Piece::Scale(n) => {
                        state_children
                            .entry((state, "UIScale"))
                            .or_default()
                            .insert("Scale", num(n));
                    }

                    Piece::Stroke(k, v) => {
                        state_children
                            .entry((state, "UIStroke"))
                            .or_default()
                            .insert(k, v);
                    }

                    Piece::NoEffect(what) => out.problems.push((i, Problem::NoEffect(what))),

                    Piece::Theme => out.uses_theme = true,

                    // One report for the class, not one for each side
                    // of a padding.
                    _ if out.problems.contains(&(i, Problem::VariantNeedsProperty)) => {}

                    _ => out.problems.push((i, Problem::VariantNeedsProperty)),
                }

                continue;
            }

            match piece {
                Piece::Prop { name, value, needs } => {
                    if !needs.met_by(element) {
                        // A utility that sets a few properties by class,
                        // `opacity-50`, is fine on any of them.
                        continue;
                    }

                    if let Some(field) = name.strip_prefix("__") {
                        font.insert(field_key(field), value);
                    } else {
                        out.props.push((name, value));
                    }
                }

                Piece::SizeX(d) => size.0 = Some(d),
                Piece::SizeY(d) => size.1 = Some(d),
                Piece::AutoX => auto.0 = true,
                Piece::AutoY => auto.1 = true,
                Piece::PosX { dim, from_end } => pos.0 = Some((dim, from_end)),
                Piece::PosY { dim, from_end } => pos.1 = Some((dim, from_end)),
                Piece::AnchorX(a) => anchor.0 = Some(a),
                Piece::AnchorY(a) => anchor.1 = Some(a),
                Piece::Layout(k, v) => {
                    layout.insert(k, v);
                }
                Piece::Justify(j) => justify = Some(j),
                Piece::Items(a) => items = Some(a),
                Piece::Gap { x, y } => {
                    if x.is_some() {
                        gap.0 = x;
                    }

                    if y.is_some() {
                        gap.1 = y;
                    }
                }
                Piece::Padding(side, px) => {
                    padding.insert(side, px);
                }
                Piece::Corner(d) => corner = Some(d),
                Piece::Stroke(k, v) => {
                    stroke.insert(k, v);
                }
                Piece::GradientStop(k, c) => stops.push((k, c)),
                Piece::GradientRotation(deg) => rotation = Some(deg),
                Piece::SizeConstraint(field, axis, px) => {
                    constraint.insert((field, axis), px);
                }
                Piece::Aspect(r) => aspect = Some(r),
                Piece::Scale(s) => scale = Some(s),
                Piece::FlexItem(k, v) => {
                    flex.insert(k, v);
                }
                Piece::Group => out.group = true,
                Piece::Transition(kind) => transition = Some(kind),
                Piece::Duration(secs) => duration = Some(secs),
                Piece::Ease(style, direction) => ease = Some((style, direction)),
                Piece::Delay(secs) => delay = Some(secs),
                Piece::Theme => out.uses_theme = true,
                Piece::NoEffect(what) => out.problems.push((i, Problem::NoEffect(what))),
            }
        }

        // The pieces above skip a property the element lacks; a class
        // that set nothing at all on this element says so. The theme
        // marker sets nothing of its own.
        if let Some(u) = parse(class, ctx)
            && !u.pieces.is_empty()
            && u.pieces.iter().all(|p| match p {
                Piece::Prop { needs, .. } => !needs.met_by(element),

                p => *p == Piece::Theme,
            })
            && let Some(Piece::Prop { name, needs, .. }) = u.pieces.first()
        {
            out.problems
                .push((i, Problem::WrongElement(name.clone(), *needs)));
        }
    }

    // A duration, an easing, or a delay alone implies `transition`.
    let kind = match transition {
        Some("none") => None,
        Some(kind) => Some(kind),
        None if duration.is_some() || ease.is_some() || delay.is_some() => Some("default"),
        None => None,
    };

    if let Some(kind) = kind {
        let (style, direction) = ease.unwrap_or(("Quad", "InOut"));
        out.transition = Some(Transition {
            kind,
            duration: duration.unwrap_or(0.15),
            style,
            direction,
            delay: delay.unwrap_or(0.0),
        });
    }

    let gui = element.is_gui_object();

    // Size. An automatic axis grows from 0, so the content sets it. An
    // axis no class names keeps the size of a new element, the Roblox
    // 100 pixels; the transform takes it from a `Size` attribute.
    let axes = (
        size.0.or(auto.0.then_some(Dim::px(0.0))),
        size.1.or(auto.1.then_some(Dim::px(0.0))),
    );

    if gui && (axes.0.is_some() || axes.1.is_some()) {
        out.size = axes;
        let x = axes.0.unwrap_or(Dim::px(100.0));
        let y = axes.1.unwrap_or(Dim::px(100.0));
        out.props.push((
            "Size".into(),
            format!(
                "UDim2.new({}, {}, {}, {})",
                num(x.scale),
                num(x.offset),
                num(y.scale),
                num(y.offset)
            ),
        ));
    }

    if gui && (auto.0 || auto.1) {
        let word = match auto {
            (true, true) => "XY",
            (true, false) => "X",
            _ => "Y",
        };
        out.props
            .push(("AutomaticSize".into(), format!("Enum.AutomaticSize.{word}")));
    }

    // Position: an offset from the end anchors the element at that end.
    if gui && (pos.0.is_some() || pos.1.is_some()) {
        let (x, ax) = match pos.0 {
            Some((d, true)) => (
                Dim {
                    scale: 1.0 - d.scale,
                    offset: -d.offset,
                },
                Some(1.0),
            ),
            Some((d, false)) => (d, None),
            None => (Dim::px(0.0), None),
        };
        let (y, ay) = match pos.1 {
            Some((d, true)) => (
                Dim {
                    scale: 1.0 - d.scale,
                    offset: -d.offset,
                },
                Some(1.0),
            ),
            Some((d, false)) => (d, None),
            None => (Dim::px(0.0), None),
        };
        out.props.push((
            "Position".into(),
            format!(
                "UDim2.new({}, {}, {}, {})",
                num(x.scale),
                num(x.offset),
                num(y.scale),
                num(y.offset)
            ),
        ));

        if anchor.0.is_none() {
            anchor.0 = ax;
        }

        if anchor.1.is_none() {
            anchor.1 = ay;
        }
    }

    if gui && (anchor.0.is_some() || anchor.1.is_some()) {
        out.props.push((
            "AnchorPoint".into(),
            format!(
                "Vector2.new({}, {})",
                num(anchor.0.unwrap_or(0.0)),
                num(anchor.1.unwrap_or(0.0))
            ),
        ));
    }

    // Fonts: family, weight, and style make one FontFace.
    if element.has_text() && !font.is_empty() {
        out.props.push(("FontFace".into(), font_face(&font, fonts)));
    }

    for (state, f) in state_fonts {
        let mut merged = font.clone();
        merged.extend(f);
        out.states
            .entry(state)
            .or_default()
            .push(("FontFace".into(), font_face(&merged, fonts)));
    }

    // The list or grid layout.
    let grid = layout.contains_key("__grid");
    let has_layout = gui
        && (!layout.is_empty()
            || justify.is_some()
            || items.is_some()
            || gap.0.is_some()
            || gap.1.is_some());

    if has_layout && grid {
        let mut props = Vec::new();
        let gap_x = gap.0.unwrap_or(0.0);
        let gap_y = gap.1.unwrap_or(0.0);
        props.push((
            "CellPadding".to_string(),
            format!("UDim2.new(0, {}, 0, {})", num(gap_x), num(gap_y)),
        ));

        let cols: Option<f64> = layout.get("__cols").and_then(|c| c.parse().ok());
        let rows: Option<f64> = layout.get("__rows").and_then(|c| c.parse().ok());
        let row_height: f64 = layout
            .get("__row_height")
            .and_then(|c| c.parse().ok())
            .unwrap_or(100.0);
        let col_width: f64 = layout
            .get("__col_width")
            .and_then(|c| c.parse().ok())
            .unwrap_or(100.0);
        let cell = match (cols, rows) {
            (Some(n), _) => format!(
                "UDim2.new({}, {}, 0, {})",
                num(1.0 / n),
                num(-gap_x * (n - 1.0) / n),
                num(row_height)
            ),
            (None, Some(n)) => {
                props.push((
                    "FillDirection".to_string(),
                    "Enum.FillDirection.Vertical".to_string(),
                ));

                format!(
                    "UDim2.new(0, {}, {}, {})",
                    num(col_width),
                    num(1.0 / n),
                    num(-gap_y * (n - 1.0) / n)
                )
            }
            (None, None) => format!("UDim2.new(0, {}, 0, {})", num(col_width), num(row_height)),
        };
        props.push(("CellSize".to_string(), cell));
        props.push((
            "SortOrder".to_string(),
            "Enum.SortOrder.LayoutOrder".to_string(),
        ));

        if let Some(j) = justify {
            props.push((
                "HorizontalAlignment".to_string(),
                alignment("Horizontal", j),
            ));
        }

        if let Some(a) = items {
            props.push(("VerticalAlignment".to_string(), alignment("Vertical", a)));
        }

        out.children.push(Child {
            class: "UIGridLayout",
            props,
        });
    } else if has_layout {
        let horizontal = layout
            .get("FillDirection")
            .is_none_or(|d| d.contains("Horizontal"));
        let mut props = vec![(
            "FillDirection".to_string(),
            if horizontal {
                "Enum.FillDirection.Horizontal".to_string()
            } else {
                "Enum.FillDirection.Vertical".to_string()
            },
        )];
        props.push((
            "SortOrder".to_string(),
            "Enum.SortOrder.LayoutOrder".to_string(),
        ));

        if let Some(w) = layout.get("Wraps") {
            props.push(("Wraps".to_string(), w.clone()));
        }

        let main_gap = if horizontal { gap.0 } else { gap.1 };

        if let Some(g) = main_gap {
            props.push(("Padding".to_string(), Dim::px(g).luau()));
        }

        let (main, cross) = if horizontal {
            ("Horizontal", "Vertical")
        } else {
            ("Vertical", "Horizontal")
        };

        if let Some(j) = justify {
            match j {
                "between" | "around" | "evenly" | "fill" => {
                    props.push((
                        format!("{main}Flex"),
                        format!("Enum.UIFlexAlignment.{}", flex_word(j)),
                    ));
                }

                _ => props.push((format!("{main}Alignment"), alignment(main, j))),
            }
        }

        if let Some(a) = items {
            match a {
                "fill" => props.push((
                    format!("{cross}Flex"),
                    "Enum.UIFlexAlignment.Fill".to_string(),
                )),

                _ => props.push((format!("{cross}Alignment"), alignment(cross, a))),
            }
        }

        out.children.push(Child {
            class: "UIListLayout",
            props,
        });
    }

    if gui && !padding.is_empty() {
        out.children.push(Child {
            class: "UIPadding",
            props: padding
                .iter()
                .map(|(side, px)| ((*side).to_string(), Dim::px(*px).luau()))
                .collect(),
        });
    }

    if gui && let Some(d) = corner {
        out.children.push(Child {
            class: "UICorner",
            props: vec![("CornerRadius".to_string(), d.luau())],
        });
    }

    // A state that changes a stroke or a scale needs the child at rest:
    // a stroke of no width and a scale of 1, as in Tailwind.
    for ((state, class), props) in &state_children {
        match *class {
            "UIStroke" if stroke.is_empty() => {
                stroke.insert("Thickness", "0".into());
            }

            "UIScale" if scale.is_none() => scale = Some(1.0),

            _ => {}
        }

        let fields = props
            .iter()
            .map(|(k, v)| format!("{k} = {v}"))
            .collect::<Vec<_>>()
            .join(", ");
        out.states
            .entry(state)
            .or_default()
            .push(((*class).to_string(), format!("{{ {fields} }}")));
    }

    if gui && !stroke.is_empty() {
        let mut props: Vec<(String, String)> = stroke
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect();

        if !stroke.contains_key("Thickness") {
            props.push(("Thickness".to_string(), "1".to_string()));
        }

        // The border is the Tailwind reading. `stroke-contextual` asks
        // for the outline of the text, so the default stays out.
        if !stroke.contains_key("ApplyStrokeMode") {
            props.push((
                "ApplyStrokeMode".to_string(),
                "Enum.ApplyStrokeMode.Border".to_string(),
            ));
        }
        out.children.push(Child {
            class: "UIStroke",
            props,
        });
    }

    if gui && (!stops.is_empty() || rotation.is_some()) {
        let stop = |k: &str| stops.iter().find(|(s, _)| *s == k).map(|(_, c)| c.clone());
        let (from, via, to) = (stop("from"), stop("via"), stop("to"));
        let white = || color3((255, 255, 255));
        let first = from
            .clone()
            .or_else(|| via.clone())
            .or_else(|| to.clone())
            .unwrap_or_else(white);
        let last = to
            .clone()
            .or_else(|| via.clone())
            .or_else(|| from.clone())
            .unwrap_or_else(white);
        let keypoints = match via {
            Some(v) if from.is_some() || to.is_some() => format!(
                "ColorSequence.new({{ ColorSequenceKeypoint.new(0, {first}), ColorSequenceKeypoint.new(0.5, {v}), ColorSequenceKeypoint.new(1, {last}) }})"
            ),

            _ => format!("ColorSequence.new({first}, {last})"),
        };
        let mut props = vec![("Color".to_string(), keypoints)];

        if let Some(deg) = rotation {
            props.push(("Rotation".to_string(), num(deg)));
        }

        out.children.push(Child {
            class: "UIGradient",
            props,
        });
    }

    if gui && !constraint.is_empty() {
        let get =
            |f: &str, a: &str, default: f64| constraint.get(&(f, a)).copied().unwrap_or(default);
        let mut props = Vec::new();

        if constraint.keys().any(|(f, _)| *f == "MinSize") {
            props.push((
                "MinSize".to_string(),
                format!(
                    "Vector2.new({}, {})",
                    num(get("MinSize", "X", 0.0)),
                    num(get("MinSize", "Y", 0.0))
                ),
            ));
        }

        if constraint.keys().any(|(f, _)| *f == "MaxSize") {
            let v = |a: &str| {
                let n = get("MaxSize", a, f64::INFINITY);

                if n.is_infinite() {
                    "math.huge".to_string()
                } else {
                    num(n)
                }
            };
            props.push((
                "MaxSize".to_string(),
                format!("Vector2.new({}, {})", v("X"), v("Y")),
            ));
        }

        out.children.push(Child {
            class: "UISizeConstraint",
            props,
        });
    }

    if gui && let Some(r) = aspect {
        out.children.push(Child {
            class: "UIAspectRatioConstraint",
            props: vec![("AspectRatio".to_string(), num(r))],
        });
    }

    if gui && let Some(s) = scale {
        out.children.push(Child {
            class: "UIScale",
            props: vec![("Scale".to_string(), num(s))],
        });
    }

    if gui && !flex.is_empty() {
        out.children.push(Child {
            class: "UIFlexItem",
            props: flex
                .iter()
                .map(|(k, v)| ((*k).to_string(), v.clone()))
                .collect(),
        });
    }

    out
}

fn field_key(field: &str) -> &'static str {
    match field {
        "family" => "family",
        "weight" => "weight",
        "face" => "face",
        _ => "italic",
    }
}

/// The `FontFace` of the font pieces. A theme font is a whole `Font`;
/// a weight or a style class beside it takes the family from it.
fn font_face(font: &BTreeMap<&'static str, String>, fonts: &Fonts) -> String {
    let weight = font.get("weight").cloned();
    let italic = font.get("italic").map(|i| i == "true");

    if let Some(face) = font.get("face") {
        if weight.is_none() && italic.is_none() {
            return face.clone();
        }

        let w = weight.map_or_else(
            || format!("({face}).Weight"),
            |w| format!("Enum.FontWeight.{w}"),
        );
        let s = match italic {
            Some(true) => "Enum.FontStyle.Italic".to_string(),
            Some(false) => "Enum.FontStyle.Normal".to_string(),
            None => format!("({face}).Style"),
        };

        return format!("Font.new(({face}).Family, {w}, {s})");
    }

    let family = font
        .get("family")
        .cloned()
        .unwrap_or_else(|| fonts.sans.clone());
    let weight = weight.unwrap_or_else(|| "Regular".to_string());
    let style = if italic == Some(true) {
        "Italic"
    } else {
        "Normal"
    };

    format!("Font.new(\"{family}\", Enum.FontWeight.{weight}, Enum.FontStyle.{style})")
}

fn alignment(axis: &str, word: &str) -> String {
    let name = match (axis, word) {
        ("Horizontal", "start") => "Left",
        ("Horizontal", "end") => "Right",
        ("Vertical", "start") => "Top",
        ("Vertical", "end") => "Bottom",
        _ => "Center",
    };

    format!("Enum.{axis}Alignment.{name}")
}

fn flex_word(word: &str) -> &'static str {
    match word {
        "between" => "SpaceBetween",
        "around" => "SpaceAround",
        "evenly" => "SpaceEvenly",
        _ => "Fill",
    }
}

/// A color as three channels.
pub type Rgb = (u8, u8, u8);

/// One entry of the catalog: the name, a summary, and a color when the
/// utility names one.
pub type Entry = (String, String, Option<Rgb>);

/// The heads that take a number, `w-16`, `gap-2`, `duration-300`. Each
/// names a scale with no end, so the items behind one come from the
/// digits the user types; a fixed list would hold `w-1` and hide
/// `w-11`.
pub const SCALE_HEADS: &[&str] = &[
    "w",
    "h",
    "size",
    "min-w",
    "min-h",
    "max-w",
    "max-h",
    "p",
    "px",
    "py",
    "pt",
    "pr",
    "pb",
    "pl",
    "gap",
    "gap-x",
    "gap-y",
    "top",
    "left",
    "right",
    "bottom",
    "translate-x",
    "translate-y",
    "z",
    "order",
    "opacity",
    "rotate",
    "scale",
    "grid-cols",
    "auto-rows",
    "leading",
    "border",
    "stroke",
    "text-size",
    "duration",
    "delay",
    "scrollbar",
    "canvas-h",
    "canvas-w",
    "max-graphemes",
    "transparency",
    "bg-transparency",
    "text-transparency",
    "image-transparency",
    "group-transparency",
    "stroke-transparency",
    "text-stroke-transparency",
];

/// The denominators Tailwind's fractions use.
const DENOMINATORS: &[u32] = &[2, 3, 4, 5, 6, 12];

/// The head and the number a typed word names, when a scale head owns
/// it: `w-1` gives `("w", "1")`, `w-1/2` gives `("w", "1/2")`.
fn scale_tail(typed: &str) -> Option<(&'static str, &str)> {
    let word = typed.strip_prefix('-').unwrap_or(typed);
    let mut best: Option<(&'static str, &str)> = None;

    for head in SCALE_HEADS {
        let Some(rest) = word.strip_prefix(head).and_then(|r| r.strip_prefix('-')) else {
            continue;
        };
        let (num, den) = match rest.split_once('/') {
            Some((n, d)) => (n, d),

            None => (rest, ""),
        };

        if num.is_empty()
            || !num.bytes().all(|b| b.is_ascii_digit())
            || !den.bytes().all(|b| b.is_ascii_digit())
        {
            continue;
        }

        // `text-size-14` and `text-14` both start with `text`; the
        // longer head is the one the user is typing.
        if best.is_none_or(|(b, _)| b.len() < head.len()) {
            best = Some((head, rest));
        }
    }

    best
}

/// Whether the list behind a typed word grows with the word. The editor
/// caches a list it is told is whole and filters that itself, which
/// hides every step a fixed list leaves out.
pub fn takes_a_number(typed: &str) -> bool {
    let word = typed.strip_prefix('-').unwrap_or(typed);

    word.is_empty()
        || scale_tail(typed).is_some()
        || SCALE_HEADS
            .iter()
            .any(|h| h.starts_with(word) || *h == word.trim_end_matches('-'))
}

/// The utilities a typed word names when it ends in a number: the number
/// itself, every number that carries it, and the fractions. The steps
/// are Tailwind's and have no end, so the entries come from the digits
/// rather than from a list. A word with no number gives nothing, and the
/// static catalog answers it.
pub fn expand(typed: &str, ctx: &Context) -> Vec<Entry> {
    let Some((head, rest)) = scale_tail(typed) else {
        return Vec::new();
    };
    let sign = if typed.starts_with('-') { "-" } else { "" };
    let mut names = vec![format!("{sign}{head}-{rest}")];

    match rest.split_once('/') {
        // A fraction: the denominators the digits so far allow.
        Some((num, den)) => {
            for d in DENOMINATORS
                .iter()
                .filter(|d| d.to_string().starts_with(den))
            {
                names.push(format!("{sign}{head}-{num}/{d}"));
            }
        }

        None => {
            for d in 0..=9 {
                names.push(format!("{sign}{head}-{rest}{d}"));
            }

            for d in DENOMINATORS {
                names.push(format!("{sign}{head}-{rest}/{d}"));
            }
        }
    }

    let mut seen = std::collections::HashSet::new();

    names
        .into_iter()
        .filter(|n| seen.insert(n.clone()))
        .filter_map(|n| {
            let u = parse(&Class::parse(&n), ctx)?;

            // A margin parses and sets nothing. The static catalog
            // leaves those out and so does this list.
            if u.pieces.iter().all(|p| matches!(p, Piece::NoEffect(_))) {
                return None;
            }

            Some((n, u.summary, None))
        })
        .collect()
}

/// Every utility name Enamel completes, with a summary and a color when
/// the utility names one. Sizes and spacings list a few common steps.
pub fn catalog(ctx: &Context) -> Vec<Entry> {
    let mut names: Vec<String> = [
        "hidden",
        "visible",
        "flex",
        "flex-row",
        "flex-col",
        "flex-wrap",
        "flex-nowrap",
        "grid",
        "justify-start",
        "justify-center",
        "justify-end",
        "justify-between",
        "justify-around",
        "justify-evenly",
        "justify-stretch",
        "items-start",
        "items-center",
        "items-end",
        "items-stretch",
        "grow",
        "grow-0",
        "shrink",
        "flex-1",
        "self-start",
        "self-center",
        "self-end",
        "self-stretch",
        "inset-0",
        "inset-x-0",
        "inset-y-0",
        "overflow-hidden",
        "overflow-visible",
        "overflow-auto",
        "overflow-scroll",
        "overflow-x-auto",
        "overflow-y-auto",
        "appearance-none",
        "scrollbar-none",
        "truncate",
        "text-clip",
        "text-wrap",
        "text-nowrap",
        "text-scaled",
        "text-left",
        "text-center",
        "text-right",
        "align-top",
        "align-middle",
        "align-bottom",
        "rich-text",
        "italic",
        "not-italic",
        "font-sans",
        "font-serif",
        "font-mono",
        "font-thin",
        "font-light",
        "font-normal",
        "font-medium",
        "font-semibold",
        "font-bold",
        "font-extrabold",
        "font-black",
        "bg-transparent",
        "text-transparent",
        "border-transparent",
        "border",
        "border-0",
        "border-2",
        "border-4",
        "border-8",
        "rounded",
        "rounded-none",
        "rounded-sm",
        "rounded-md",
        "rounded-lg",
        "rounded-xl",
        "rounded-2xl",
        "rounded-3xl",
        "rounded-full",
        "aspect-square",
        "aspect-video",
        "object-cover",
        "object-contain",
        "object-fill",
        "object-none",
        "pointer-events-none",
        "pointer-events-auto",
        "select-none",
        "group",
        "transition",
        "transition-all",
        "transition-colors",
        "transition-opacity",
        "transition-transform",
        "transition-none",
        "duration-75",
        "duration-100",
        "duration-150",
        "duration-200",
        "duration-300",
        "duration-500",
        "duration-700",
        "duration-1000",
        "delay-75",
        "delay-100",
        "delay-150",
        "delay-200",
        "delay-300",
        "delay-500",
        "ease-linear",
        "ease-in",
        "ease-out",
        "ease-in-out",
        "ease-sine",
        "ease-quad",
        "ease-cubic",
        "ease-quart",
        "ease-quint",
        "ease-back",
        "ease-bounce",
        "ease-elastic",
        "ease-expo",
        "ease-circ",
        "order-first",
        "order-last",
        "w-full",
        "h-full",
        "size-full",
        "w-auto",
        "h-auto",
        "w-1/2",
        "w-1/3",
        "w-2/3",
        "w-1/4",
        "w-3/4",
        "h-1/2",
        "h-1/3",
        "h-1/4",
        "bg-gradient-to-r",
        "bg-gradient-to-l",
        "bg-gradient-to-t",
        "bg-gradient-to-b",
        "bg-gradient-to-tr",
        "bg-gradient-to-tl",
        "bg-gradient-to-br",
        "bg-gradient-to-bl",
        "leading-none",
        "leading-tight",
        "leading-snug",
        "leading-normal",
        "leading-relaxed",
        "leading-loose",
        "-translate-x-1/2",
        "-translate-y-1/2",
        "max-w-xs",
        "max-w-sm",
        "max-w-md",
        "max-w-lg",
        "max-w-xl",
        "max-w-2xl",
        "clip",
        "no-clip",
        "center",
        "anchor-center",
        "anchor-tl",
        "anchor-t",
        "anchor-tr",
        "anchor-l",
        "anchor-r",
        "anchor-bl",
        "anchor-b",
        "anchor-br",
        "stroke",
        "stroke-0",
        "stroke-2",
        "stroke-transparent",
        "stroke-contextual",
        "stroke-border",
        "stroke-round",
        "stroke-bevel",
        "stroke-miter",
        "text-stroke-transparent",
        "sort-name",
        "sort-order",
        "fill",
        "image-slice",
        "image-tile",
        "image-transparent",
        "scroll-x",
        "scroll-y",
        "scroll-xy",
        "no-scroll",
        "canvas-auto",
        "canvas-auto-x",
        "canvas-auto-xy",
        "elastic",
        "no-elastic",
        "auto-color",
        "no-auto-color",
        "modal",
        "ignore-inset",
        "reset-on-spawn",
        "keep-on-spawn",
        "sibling-z",
        "global-z",
        "bg-transparency-50",
        "text-transparency-50",
        "stroke-transparency-50",
        "image-transparency-50",
        "group-transparency-50",
        "transparency-50",
        "text-size-14",
        "text-size-18",
        "text-size-24",
        "canvas-h-200",
        "canvas-w-200",
        "max-graphemes-20",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect();

    for size in ["xs", "sm", "base", "lg", "xl", "2xl", "3xl", "4xl", "5xl"] {
        names.push(format!("text-{size}"));
    }

    for n in [0, 1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32, 40, 48, 64] {
        for head in [
            "w", "h", "p", "px", "py", "pt", "pr", "pb", "pl", "gap", "top", "left", "right",
            "bottom", "size", "z", "order",
        ] {
            names.push(format!("{head}-{n}"));
        }
    }

    for n in [0, 25, 50, 75, 100] {
        names.push(format!("opacity-{n}"));
    }

    for n in [0, 45, 90, 180] {
        names.push(format!("rotate-{n}"));
    }

    for n in [50, 75, 90, 100, 110, 125, 150] {
        names.push(format!("scale-{n}"));
    }

    for n in 1..=6 {
        names.push(format!("grid-cols-{n}"));
    }

    for f in fonts::FONTS {
        names.push(format!("font-{}", f.class));
    }

    for name in ctx.theme.fonts.keys() {
        names.push(format!("font-{name}"));
    }

    names.extend(ctx.theme.classes.keys().cloned());

    let mut out: Vec<Entry> = names
        .into_iter()
        .filter_map(|n| {
            let u = parse(&Class::parse(&n), ctx)?;

            Some((n, u.summary, None))
        })
        .collect();
    let color_names: Vec<(String, Option<Rgb>)> = palette::PALETTE
        .iter()
        .map(|(n, rgb)| ((*n).to_string(), Some(*rgb)))
        .chain(ctx.theme.colors.iter().map(|(n, e)| (n.clone(), e.color())))
        .collect();

    for (name, rgb) in color_names {
        for head in [
            "bg",
            "text",
            "border",
            "stroke",
            "text-stroke",
            "from",
            "via",
            "to",
            "placeholder",
            "image",
        ] {
            let n = format!("{head}-{name}");
            let Some(u) = parse(&Class::parse(&n), ctx) else {
                continue;
            };
            out.push((n, u.summary, rgb));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(tag: &str, classes: &str) -> Resolved {
        let cs: Vec<Class> = classes.split_whitespace().map(Class::parse).collect();

        resolve(Element::parse(tag).unwrap(), &cs, &Context::default())
    }

    /// A scale has no end, so the list behind a number comes from the
    /// digits typed. The fixed steps hold `w-1` and `w-10`; `w-11` is
    /// only here.
    #[test]
    fn a_number_grows_the_list_it_names() {
        let ctx = Context::default();
        let names = |typed: &str| {
            expand(typed, &ctx)
                .into_iter()
                .map(|(n, _, _)| n)
                .collect::<Vec<_>>()
        };
        let w = names("w-1");

        assert_eq!(w[0], "w-1");
        assert!(w.contains(&"w-11".to_string()), "{w:?}");
        assert!(w.contains(&"w-19".to_string()), "{w:?}");
        assert!(w.contains(&"w-1/2".to_string()), "{w:?}");
        assert!(w.contains(&"w-1/3".to_string()), "{w:?}");
        assert_eq!(expand("w-1", &ctx)[0].1, "width 4 pixels");
        assert_eq!(expand("w-16", &ctx)[0].1, "width 64 pixels");

        // The other heads the number reaches.
        assert!(names("p-1").contains(&"p-11".to_string()));
        assert!(names("gap-2").contains(&"gap-24".to_string()));
        assert!(names("text-size-1").contains(&"text-size-18".to_string()));
        assert!(names("duration-3").contains(&"duration-30".to_string()));
        assert!(names("bg-transparency-2").contains(&"bg-transparency-25".to_string()));

        // A fraction narrows to the denominators the digits allow.
        assert_eq!(
            names("w-1/"),
            ["w-1/2", "w-1/3", "w-1/4", "w-1/5", "w-1/6", "w-1/12"]
        );
        assert_eq!(names("w-1/1"), ["w-1/1", "w-1/12"]);
        assert_eq!(names("-translate-x-1")[0], "-translate-x-1/2");

        // A margin parses and sets nothing, so it stays out.
        assert!(names("m-4").is_empty());
        // A word with no number is the static catalog's alone.
        assert!(names("flex").is_empty());
        assert!(names("bg-red-5").is_empty());
    }

    /// The editor filters a list it is told is whole, so a list built
    /// from the word being typed has to say it is not.
    #[test]
    fn a_scale_word_asks_the_ingot_again() {
        assert!(takes_a_number(""));
        assert!(takes_a_number("w"));
        assert!(takes_a_number("w-"));
        assert!(takes_a_number("w-1"));
        assert!(takes_a_number("g"));
        assert!(takes_a_number("gap-2"));
        assert!(takes_a_number("text-"));
        assert!(takes_a_number("-translate-x-1"));

        // A fixed list: the editor may filter what it holds.
        assert!(!takes_a_number("flex"));
        assert!(!takes_a_number("rounded-l"));
        assert!(!takes_a_number("bg-red-5"));
        assert!(!takes_a_number("justify-c"));
    }

    /// Silk reads `appearance-none` and the sideways overflow, so Enamel
    /// knows them and reports no problem.
    #[test]
    fn the_classes_silk_reads_are_known() {
        let r = resolved("TextButton", "appearance-none");
        assert!(r.problems.is_empty() && r.props.is_empty(), "{r:?}");

        let r = resolved("ScrollingFrame", "overflow-x-auto");
        assert_eq!(
            r.props,
            [("ScrollingEnabled".to_string(), "true".to_string())]
        );
        assert!(r.problems.is_empty(), "{r:?}");
    }

    #[test]
    fn a_transition_reads_its_time_and_easing() {
        let r = resolved("TextButton", "transition hover:bg-red-600");
        let t = r.transition.expect("transition");
        assert_eq!(
            (t.kind, t.duration, t.style, t.direction, t.delay),
            ("default", 0.15, "Quad", "InOut", 0.0)
        );

        let r = resolved(
            "TextButton",
            "transition-colors duration-[300ms] ease-back-in delay-[0.1s]",
        );
        let t = r.transition.expect("transition");
        assert_eq!(
            (t.kind, t.duration, t.style, t.direction, t.delay),
            ("colors", 0.3, "Back", "In", 0.1)
        );
        assert_eq!(
            t.luau(),
            "{ kind = \"colors\", info = TweenInfo.new(0.3, Enum.EasingStyle.Back, Enum.EasingDirection.In, 0, false, 0.1) }"
        );

        // A time alone implies the transition; `none` turns it off.
        assert!(resolved("Frame", "duration-500").transition.is_some());
        assert!(
            resolved("Frame", "transition-none duration-500")
                .transition
                .is_none()
        );
        assert_eq!(seconds("[2s]"), Some(2.0));
        assert_eq!(easing("elastic"), Some(("Elastic", "Out")));
        assert_eq!(easing("expo-in-out"), Some(("Exponential", "InOut")));
        assert_eq!(easing("wobble"), None);
    }

    #[test]
    fn theme_colors_fonts_and_classes_copy_their_expressions() {
        let ctx = Context {
            fonts: Fonts::default(),
            theme: crate::theme::parse(
                "local purple = Color3.fromRGB(138, 61, 245)\nreturn {\n    colors = { brand = purple },\n    fonts = { title = Font.new(\"rbxasset://fonts/families/Montserrat.json\", Enum.FontWeight.Bold) },\n    classes = { card = \"bg-brand rounded-xl\", glow = { ZIndex = 2 } },\n}\n",
            ),
        };
        let cs: Vec<Class> = "card glow font-title font-bold text-brand/50"
            .split_whitespace()
            .map(Class::parse)
            .collect();
        let r = resolve(Element::TextLabel, &cs, &ctx);
        assert!(r.uses_theme);
        assert!(
            r.props
                .contains(&("BackgroundColor3".into(), "purple".into()))
        );
        assert!(r.props.contains(&("ZIndex".into(), "2".into())));
        assert!(r.props.contains(&("TextColor3".into(), "purple".into())));
        assert!(r.props.contains(&("TextTransparency".into(), "0.5".into())));
        assert!(r.props.iter().any(|(k, v)| k == "FontFace"
            && v.starts_with("Font.new((Font.new(")
            && v.contains("Enum.FontWeight.Bold")));
        assert!(r.children.iter().any(|c| c.class == "UICorner"));
        assert!(r.problems.is_empty(), "{:?}", r.problems);
        let f = resolve(
            Element::TextLabel,
            &[Class::parse("font-gotham-bold")],
            &ctx,
        );
        assert!(
            f.props
                .iter()
                .any(|(_, v)| v.contains("GothamSSm") && v.contains("Bold"))
        );
    }

    /// A theme color on any utility, a gradient stop too, puts the
    /// theme's prelude in the file: the color may name a local there.
    #[test]
    fn a_theme_color_marks_the_theme_as_used() {
        let ctx = Context {
            fonts: Fonts::default(),
            theme: crate::theme::parse(
                "local gold = Color3.fromRGB(255, 214, 92)\nexport const colors = { accent = gold }\n",
            ),
        };
        let uses = |tag: &str, classes: &str| {
            let cs: Vec<Class> = classes.split_whitespace().map(Class::parse).collect();
            let r = resolve(Element::parse(tag).unwrap(), &cs, &ctx);

            (r.uses_theme, r.problems)
        };

        for (tag, classes) in [
            ("Frame", "bg-gradient-to-b from-accent to-orange-500"),
            ("Frame", "via-accent"),
            ("Frame", "to-accent"),
            ("Frame", "bg-accent/50"),
            ("Frame", "stroke-accent"),
            ("Frame", "ring-accent"),
            ("Frame", "hover:bg-accent"),
            ("TextLabel", "text-accent"),
            ("TextLabel", "text-stroke-accent"),
            ("TextBox", "placeholder-accent"),
            ("ImageLabel", "image-accent"),
        ] {
            assert_eq!(uses(tag, classes), (true, Vec::new()), "{classes}");
        }

        assert!(!uses("Frame", "bg-red-500 from-white").0);
        // The marker does not hide a class the element lacks.
        assert!(matches!(
            &uses("Frame", "text-accent").1[..],
            [(0, Problem::WrongElement(n, Needs::Text))] if n == "TextColor3"
        ));
    }

    #[test]
    fn a_class_splits_into_variants_sign_and_base() {
        let c = Class::parse("hover:-top-2");
        assert_eq!(c.variants, vec![Variant::Hover]);
        assert!(c.negative);
        assert_eq!(c.base, "top-2");
        assert_eq!(
            Class::parse("md:flex").unknown_variant.as_deref(),
            Some("md")
        );
    }

    #[test]
    fn colors_size_and_layout_resolve() {
        let r = resolved(
            "Frame",
            "flex-col gap-2 p-4 bg-red-500 rounded-lg w-full h-10",
        );
        assert!(r.props.contains(&(
            "BackgroundColor3".into(),
            "Color3.fromRGB(239, 68, 68)".into()
        )));
        assert!(
            r.props
                .contains(&("Size".into(), "UDim2.new(1, 0, 0, 40)".into()))
        );
        let list = r
            .children
            .iter()
            .find(|c| c.class == "UIListLayout")
            .unwrap();
        assert!(
            list.props
                .contains(&("FillDirection".into(), "Enum.FillDirection.Vertical".into()))
        );
        assert!(
            list.props
                .contains(&("Padding".into(), "UDim.new(0, 8)".into()))
        );
        assert!(
            r.children
                .iter()
                .any(|c| c.class == "UIPadding" && c.props.len() == 4)
        );
        assert!(r.children.iter().any(|c| c.class == "UICorner"));
        assert!(r.problems.is_empty());
    }

    #[test]
    fn states_and_problems_report() {
        let r = resolved(
            "TextButton",
            "bg-red-500 hover:bg-red-600 text-white font-bold shadow-lg nonsense text-[#123456]/50",
        );
        assert_eq!(
            r.states["hover"],
            vec![(
                "BackgroundColor3".to_string(),
                "Color3.fromRGB(220, 38, 38)".to_string()
            )]
        );
        assert!(
            r.props
                .iter()
                .any(|(k, v)| k == "FontFace" && v.contains("Bold"))
        );
        assert!(r.props.contains(&("TextTransparency".into(), "0.5".into())));
        assert!(
            r.problems
                .iter()
                .any(|(i, p)| *i == 4 && matches!(p, Problem::NoEffect(_)))
        );
        assert!(
            r.problems
                .iter()
                .any(|(i, p)| *i == 5 && matches!(p, Problem::Unknown))
        );
    }

    /// A state changes a scale or a stroke through the child. With no
    /// class at rest, the child starts at no width and a scale of 1.
    #[test]
    fn a_state_changes_a_scale_and_a_stroke() {
        let r = resolved(
            "TextButton",
            "transition hover:scale-110 hover:stroke-yellow-400 hover:ring-4",
        );

        assert!(r.problems.is_empty(), "{:?}", r.problems);
        assert_eq!(
            r.states["hover"],
            vec![
                ("UIScale".to_string(), "{ Scale = 1.1 }".to_string()),
                (
                    "UIStroke".to_string(),
                    "{ Color = Color3.fromRGB(250, 204, 21), Thickness = 4 }".to_string()
                ),
            ]
        );
        let child = |class: &str| {
            r.children
                .iter()
                .find(|c| c.class == class)
                .unwrap()
                .props
                .clone()
        };
        assert_eq!(child("UIScale"), [("Scale".to_string(), "1".to_string())]);
        assert!(child("UIStroke").contains(&("Thickness".to_string(), "0".to_string())));

        // A class at rest keeps its value.
        let r = resolved("Frame", "scale-95 stroke-2 hover:scale-105 hover:stroke-4");
        assert!(
            r.children
                .iter()
                .any(|c| c.class == "UIScale" && c.props[0].1 == "0.95")
        );
        assert!(r.children.iter().any(|c| {
            c.class == "UIStroke"
                && c.props
                    .contains(&("Thickness".to_string(), "2".to_string()))
        }));

        // A layout child cannot follow a state.
        assert!(matches!(
            resolved("Frame", "hover:p-4").problems[..],
            [(0, Problem::VariantNeedsProperty)]
        ));
    }

    #[test]
    fn a_text_utility_on_a_frame_is_reported() {
        let r = resolved("Frame", "text-white");
        assert!(r.props.is_empty());
        assert!(matches!(
            &r.problems[0].1,
            Problem::WrongElement(n, Needs::Text) if n == "TextColor3"
        ));
    }

    #[test]
    fn roblox_properties_have_their_own_words() {
        let r = resolved(
            "TextButton",
            "bg-transparency-50 stroke-2 stroke-red-500 stroke-transparency-25 text-stroke-black text-size-18 anchor-center auto-color",
        );
        assert!(
            r.props
                .contains(&("BackgroundTransparency".into(), "0.5".into()))
        );
        assert!(
            r.props
                .contains(&("TextStrokeColor3".into(), "Color3.fromRGB(0, 0, 0)".into()))
        );
        assert!(
            r.props
                .contains(&("TextStrokeTransparency".into(), "0".into()))
        );
        assert!(r.props.contains(&("TextSize".into(), "18".into())));
        assert!(
            r.props
                .contains(&("AnchorPoint".into(), "Vector2.new(0.5, 0.5)".into()))
        );
        assert!(r.props.contains(&("AutoButtonColor".into(), "true".into())));
        let stroke = r.children.iter().find(|c| c.class == "UIStroke").unwrap();
        assert!(stroke.props.contains(&("Thickness".into(), "2".into())));
        assert!(
            stroke
                .props
                .contains(&("Transparency".into(), "0.25".into()))
        );
        assert!(r.problems.is_empty(), "{:?}", r.problems);
        let s = resolved(
            "ScrollingFrame",
            "scroll-y canvas-auto canvas-h-400 no-elastic scrollbar-4",
        );
        assert!(
            s.props
                .contains(&("CanvasSize".into(), "UDim2.new(0, 0, 0, 400)".into()))
        );
        assert!(
            s.props
                .contains(&("AutomaticCanvasSize".into(), "Enum.AutomaticSize.Y".into()))
        );
        assert!(s.problems.is_empty(), "{:?}", s.problems);
    }

    /// A UIStroke takes one `ApplyStrokeMode`: the class that names one,
    /// else the border.
    #[test]
    fn a_stroke_mode_class_replaces_the_border_mode() {
        let modes = |classes: &str| {
            resolved("TextLabel", classes)
                .children
                .into_iter()
                .find(|c| c.class == "UIStroke")
                .unwrap()
                .props
                .into_iter()
                .filter(|(k, _)| k == "ApplyStrokeMode")
                .map(|(_, v)| v)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            modes("stroke-4 stroke-contextual"),
            ["Enum.ApplyStrokeMode.Contextual"]
        );
        assert_eq!(modes("stroke-4"), ["Enum.ApplyStrokeMode.Border"]);
    }

    #[test]
    fn brackets_take_any_value() {
        let r = resolved(
            "ImageButton",
            "stroke-[3px] stroke-transparency-[0.4] bg-transparency-[35%] image-transparency-[0.1] order-[7] leading-[1.4] rounded-[50%] bg-[Color3.fromHSV(0.5,_1,_1)] image-[rbxassetid://123] font-[Montserrat]",
        );
        assert!(
            r.props
                .contains(&("BackgroundTransparency".into(), "0.35".into()))
        );
        assert!(
            r.props
                .contains(&("ImageTransparency".into(), "0.1".into()))
        );
        assert!(r.props.contains(&("LayoutOrder".into(), "7".into())));
        assert!(r.props.contains(&(
            "BackgroundColor3".into(),
            "Color3.fromHSV(0.5, 1, 1)".into()
        )));
        assert!(
            r.props
                .contains(&("Image".into(), "\"rbxassetid://123\"".into()))
        );

        let stroke = r.children.iter().find(|c| c.class == "UIStroke").unwrap();
        assert!(stroke.props.contains(&("Thickness".into(), "3".into())));
        assert!(
            stroke
                .props
                .contains(&("Transparency".into(), "0.4".into()))
        );
        assert!(
            r.children
                .iter()
                .any(|c| c.class == "UICorner" && c.props[0].1 == "UDim.new(0.5, 0)")
        );
        assert!(
            r.problems
                .iter()
                .all(|(_, p)| matches!(p, Problem::WrongElement(..))),
            "{:?}",
            r.problems
        );
        let t = resolved("TextLabel", "leading-[1.4] font-[Montserrat]");
        assert!(t.props.contains(&("LineHeight".into(), "1.4".into())));
        assert!(
            t.props
                .iter()
                .any(|(k, v)| k == "FontFace" && v.contains("Montserrat.json"))
        );
    }

    /// An automatic axis grows from 0, so the content sets it, as
    /// Tailwind's `auto` does. An axis no class names stays at the size
    /// of a new element.
    #[test]
    fn an_automatic_axis_starts_at_zero() {
        let size = |classes: &str| {
            let r = resolved("Frame", classes);
            let get = |k: &str| r.props.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());

            (get("Size"), get("AutomaticSize"))
        };

        assert_eq!(
            size("w-auto h-10"),
            (
                Some("UDim2.new(0, 0, 0, 40)".into()),
                Some("Enum.AutomaticSize.X".into())
            )
        );
        assert_eq!(
            size("h-auto"),
            (
                Some("UDim2.new(0, 100, 0, 0)".into()),
                Some("Enum.AutomaticSize.Y".into())
            )
        );
        assert_eq!(size("h-10").0, Some("UDim2.new(0, 100, 0, 40)".into()));
        assert_eq!(
            size("w-40 w-auto").0,
            Some("UDim2.new(0, 160, 0, 100)".into())
        );
    }

    #[test]
    fn positions_anchor_at_the_far_side() {
        let r = resolved("Frame", "right-4 bottom-0");
        assert!(
            r.props
                .contains(&("Position".into(), "UDim2.new(1, -16, 1, 0)".into()))
        );
        assert!(
            r.props
                .contains(&("AnchorPoint".into(), "Vector2.new(1, 1)".into()))
        );
        let c = resolved(
            "Frame",
            "top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2",
        );
        assert!(
            c.props
                .contains(&("AnchorPoint".into(), "Vector2.new(0.5, 0.5)".into()))
        );
    }
}
