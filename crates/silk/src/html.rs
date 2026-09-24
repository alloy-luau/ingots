//! The HTML elements and attributes Silk knows, and the Roblox form of
//! each one.

/// How an HTML element maps to Roblox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A box that stacks its children: a Frame with a UIListLayout.
    Block,
    /// `ul` and `ol`: a Block with room for the markers.
    List,
    /// A block of text: a TextLabel as wide as its parent.
    Text,
    /// Text inside text: a RichText tag in the parent's text, or a
    /// TextLabel that sizes to its text on its own.
    Inline,
    /// `br` and `wbr`.
    Break,
    Button,
    Link,
    Input,
    TextArea,
    Image,
    Video,
    Audio,
    Canvas,
    /// `hr`.
    Rule,
    /// `progress` and `meter`: a bar with a fill.
    Progress,
    Table,
    Row,
    Cell,
    /// `thead`, `tbody`, `tfoot`: a fragment, so the rows reach the table.
    Group,
    Style,
    /// `col`, `colgroup`, `source`, `track`: no instance.
    Removed,
    /// No Roblox form.
    Unsupported,
}

pub struct Tag {
    pub name: &'static str,
    pub kind: Kind,
    pub doc: &'static str,
}

const fn t(name: &'static str, kind: Kind, doc: &'static str) -> Tag {
    Tag { name, kind, doc }
}

use Kind::*;

/// Every HTML element, in alphabetical order.
pub const TAGS: &[Tag] = &[
    t(
        "a",
        Link,
        "A link. `href=\"#id\"` scrolls the ScrollingFrame that holds the element with that `id` to it. Inside text it is RichText and does not take a click.",
    ),
    t("abbr", Inline, "An abbreviation."),
    t("address", Block, "Contact information."),
    t(
        "area",
        Unsupported,
        "An image map area. Roblox has no image maps.",
    ),
    t("article", Block, "A self-contained composition."),
    t("aside", Block, "Content beside the main content."),
    t("audio", Audio, "A Sound. `src` is the `SoundId`."),
    t("b", Inline, "Bold text."),
    t(
        "base",
        Unsupported,
        "The document base URL. A Roblox UI has no document.",
    ),
    t("bdi", Inline, "Text isolated for bidirectional layout."),
    t("bdo", Inline, "Text with an explicit direction."),
    t("big", Inline, "Larger text. Obsolete in HTML."),
    t("blockquote", Block, "A quotation, indented."),
    t(
        "body",
        Block,
        "The document body: a Frame that fills its parent.",
    ),
    t("br", Break, "A line break."),
    t(
        "button",
        Button,
        "A TextButton with the look of a browser button.",
    ),
    t(
        "canvas",
        Canvas,
        "A CanvasGroup: its children draw as one group with one transparency.",
    ),
    t("caption", Text, "The title of a table."),
    t("center", Block, "Centered content. Obsolete in HTML."),
    t("cite", Inline, "The title of a work, in italics."),
    t("code", Inline, "Code, in the monospace font."),
    t(
        "col",
        Removed,
        "A table column. The UITableLayout sizes the columns.",
    ),
    t("colgroup", Removed, "A group of table columns."),
    t("data", Inline, "A value with a machine-readable form."),
    t(
        "datalist",
        Unsupported,
        "Options for an input. A Roblox TextBox has no suggestion list.",
    ),
    t("dd", Text, "A description in a description list."),
    t("del", Inline, "Deleted text, struck through."),
    t(
        "details",
        Block,
        "A disclosure box. Its content is always shown.",
    ),
    t("dfn", Inline, "A defined term, in italics."),
    t("dialog", Block, "A dialog: visible when it has `open`."),
    t(
        "div",
        Block,
        "A generic box: a Frame that stacks its children.",
    ),
    t("dl", Block, "A description list."),
    t("dt", Text, "A term in a description list."),
    t("em", Inline, "Emphasized text, in italics."),
    t(
        "embed",
        Unsupported,
        "Embedded content. Roblox has no plugins.",
    ),
    t(
        "fieldset",
        Block,
        "A group of form controls, with a border.",
    ),
    t("figcaption", Text, "The caption of a figure."),
    t("figure", Block, "A figure with an optional caption."),
    t("font", Inline, "Styled text. Obsolete in HTML."),
    t("footer", Block, "The footer of a section."),
    t("form", Block, "A form: a Frame. Roblox has no submission."),
    t(
        "frame",
        Unsupported,
        "A frame of a frameset. Obsolete in HTML.",
    ),
    t(
        "frameset",
        Unsupported,
        "A set of frames. Obsolete in HTML.",
    ),
    t("h1", Text, "A heading, 32 px and bold."),
    t("h2", Text, "A heading, 24 px and bold."),
    t("h3", Text, "A heading, 19 px and bold."),
    t("h4", Text, "A heading, 16 px and bold."),
    t("h5", Text, "A heading, 13 px and bold."),
    t("h6", Text, "A heading, 11 px and bold."),
    t(
        "head",
        Unsupported,
        "The document head. A Roblox UI has no document.",
    ),
    t("header", Block, "The header of a section."),
    t("hgroup", Block, "A heading and its subheadings."),
    t("hr", Rule, "A horizontal rule: a Frame one pixel high."),
    t(
        "html",
        Unsupported,
        "The document root. A Roblox UI has no document; start from `body` or `div`.",
    ),
    t("i", Inline, "Text in italics."),
    t(
        "iframe",
        Unsupported,
        "A nested browsing context. Roblox cannot show a web page.",
    ),
    t("img", Image, "An ImageLabel. `src` is the `Image`."),
    t(
        "input",
        Input,
        "A TextBox, or a TextButton for `type=\"button\"`, `\"submit\"`, and `\"reset\"`.",
    ),
    t("ins", Inline, "Inserted text, underlined."),
    t("kbd", Inline, "Keyboard input, in the monospace font."),
    t("label", Inline, "A caption for a control."),
    t("legend", Text, "The caption of a fieldset."),
    t(
        "li",
        Text,
        "A list item, with a bullet in `ul` and a number in `ol`.",
    ),
    t(
        "link",
        Unsupported,
        "An external resource. Put the CSS in a `<style>` element.",
    ),
    t("main", Block, "The main content."),
    t(
        "map",
        Unsupported,
        "An image map. Roblox has no image maps.",
    ),
    t("mark", Inline, "Highlighted text."),
    t("menu", Block, "A list of commands."),
    t(
        "meta",
        Unsupported,
        "Document metadata. A Roblox UI has no document.",
    ),
    t(
        "meter",
        Progress,
        "A gauge: a bar with a fill for `value` between `min` and `max`.",
    ),
    t("nav", Block, "A section of navigation links."),
    t(
        "noscript",
        Unsupported,
        "Content without scripts. Roblox always runs Luau.",
    ),
    t(
        "object",
        Unsupported,
        "An external object. Roblox has no plugins.",
    ),
    t("ol", List, "An ordered list: each `li` takes a number."),
    t(
        "optgroup",
        Unsupported,
        "A group of options. A Roblox UI has no native dropdown.",
    ),
    t(
        "option",
        Unsupported,
        "An option of a select. A Roblox UI has no native dropdown.",
    ),
    t("output", Inline, "The result of a calculation."),
    t("p", Text, "A paragraph: a TextLabel that wraps."),
    t(
        "param",
        Unsupported,
        "A parameter of an object. Obsolete in HTML.",
    ),
    t("picture", Block, "A container for an image."),
    t(
        "pre",
        Text,
        "Preformatted text, in the monospace font, with its line breaks.",
    ),
    t(
        "progress",
        Progress,
        "A progress bar: a fill for `value` of `max`.",
    ),
    t("q", Inline, "A short quotation, in quotation marks."),
    t("rp", Inline, "Ruby fallback parentheses."),
    t("rt", Inline, "Ruby text."),
    t("ruby", Inline, "A ruby annotation."),
    t("s", Inline, "Text that is no longer right, struck through."),
    t("samp", Inline, "Sample output, in the monospace font."),
    t(
        "script",
        Unsupported,
        "A script. Write Luau outside the markup.",
    ),
    t("search", Block, "A search section."),
    t("section", Block, "A section of a document."),
    t(
        "select",
        Unsupported,
        "A dropdown. A Roblox UI has no native dropdown; build one from buttons.",
    ),
    t(
        "slot",
        Unsupported,
        "A shadow DOM slot. Roblox has no shadow DOM.",
    ),
    t("small", Inline, "Small print, 13 px."),
    t(
        "source",
        Removed,
        "A media source. Put `src` on the `video` or `audio` element.",
    ),
    t("span", Inline, "A generic run of text."),
    t("strike", Inline, "Struck-through text. Obsolete in HTML."),
    t("strong", Inline, "Important text, in bold."),
    t(
        "style",
        Style,
        "CSS for the elements under this one's parent: a StyleLink to a StyleSheet of StyleRules.",
    ),
    t("sub", Inline, "Subscript, smaller."),
    t("summary", Text, "The summary of a details element."),
    t("sup", Inline, "Superscript, smaller."),
    t(
        "svg",
        Unsupported,
        "Vector graphics. Roblox draws no SVG; use an `img`.",
    ),
    t("table", Table, "A table: a Frame with a UITableLayout."),
    t("tbody", Group, "The body rows of a table."),
    t("td", Cell, "A table cell."),
    t(
        "template",
        Unsupported,
        "A template. Write a component instead.",
    ),
    t("textarea", TextArea, "A TextBox with `MultiLine`."),
    t("tfoot", Group, "The footer rows of a table."),
    t("th", Cell, "A table header cell, bold and centered."),
    t("thead", Group, "The header rows of a table."),
    t("time", Inline, "A date or a time."),
    t(
        "title",
        Unsupported,
        "The document title. A Roblox UI has no document.",
    ),
    t("tr", Row, "A table row."),
    t("track", Removed, "A text track of a media element."),
    t(
        "tt",
        Inline,
        "Teletype text, in the monospace font. Obsolete in HTML.",
    ),
    t("u", Inline, "Underlined text."),
    t("ul", List, "An unordered list: each `li` takes a bullet."),
    t("var", Inline, "A variable, in italics."),
    t("video", Video, "A VideoFrame. `src` is the `Video`."),
    t(
        "wbr",
        Break,
        "A place where a line may break. Roblox wraps on its own, so it writes nothing.",
    ),
];

pub fn tag(name: &str) -> Option<&'static Tag> {
    TAGS.binary_search_by(|t| t.name.cmp(name))
        .ok()
        .map(|i| &TAGS[i])
}

/// The Roblox class of a tag, or `None` for a tag with no instance.
pub fn class_of(tag: &Tag, input_type: Option<&str>) -> Option<&'static str> {
    Some(match tag.kind {
        Block | List | Rule | Progress | Table | Row => "Frame",

        Text | Inline | Cell => "TextLabel",

        Button | Link => "TextButton",

        Input => match input_type.unwrap_or("text") {
            "button" | "submit" | "reset" => "TextButton",

            t if TEXT_INPUTS.contains(&t) => "TextBox",

            _ => return None,
        },

        TextArea => "TextBox",

        Image => "ImageLabel",

        Video => "VideoFrame",

        Audio => "Sound",

        Canvas => "CanvasGroup",

        Break | Group | Style | Removed | Unsupported => return None,
    })
}

/// The `type` values of an input that a TextBox can stand for.
pub const TEXT_INPUTS: &[&str] = &[
    "text", "password", "email", "number", "search", "url", "tel",
];

/// The `type` values of an input with no Roblox form.
pub const NO_INPUT: &[&str] = &[
    "checkbox",
    "radio",
    "range",
    "color",
    "file",
    "date",
    "datetime-local",
    "time",
    "month",
    "week",
    "image",
];

/// The text look a tag has by default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size: f64,
    pub weight: &'static str,
    pub italic: bool,
    pub mono: bool,
}

pub fn text_style(name: &str) -> TextStyle {
    let plain = TextStyle {
        size: 16.0,
        weight: "Regular",
        italic: false,
        mono: false,
    };
    let bold = |size| TextStyle {
        size,
        weight: "Bold",
        ..plain
    };

    match name {
        "h1" => bold(32.0),
        "h2" => bold(24.0),
        "h3" => bold(19.0),
        "h4" => bold(16.0),
        "h5" => bold(13.0),
        "h6" => bold(11.0),
        "b" | "strong" | "th" | "legend" | "summary" | "dt" | "caption" => bold(16.0),
        "i" | "em" | "cite" | "var" | "dfn" | "address" => TextStyle {
            italic: true,
            ..plain
        },
        "code" | "kbd" | "samp" | "tt" | "pre" => TextStyle {
            mono: true,
            size: 13.0,
            ..plain
        },
        "small" => TextStyle {
            size: 13.0,
            ..plain
        },
        "sub" | "sup" => TextStyle {
            size: 12.0,
            ..plain
        },
        "big" => TextStyle {
            size: 19.0,
            ..plain
        },
        "button" | "input" | "textarea" => TextStyle {
            size: 13.0,
            ..plain
        },
        _ => plain,
    }
}

/// The RichText tags an inline tag writes around its text.
pub fn rich(name: &str) -> (&'static str, &'static str) {
    match name {
        "b" | "strong" => ("<b>", "</b>"),
        "i" | "em" | "cite" | "var" | "dfn" => ("<i>", "</i>"),
        "u" | "ins" => ("<u>", "</u>"),
        "s" | "del" | "strike" => ("<s>", "</s>"),
        "code" | "kbd" | "samp" | "tt" => ("<font face=\"RobotoMono\">", "</font>"),
        "mark" => ("<mark color=\"#FFFF00\">", "</mark>"),
        "small" => ("<font size=\"13\">", "</font>"),
        "big" => ("<font size=\"19\">", "</font>"),
        "sub" | "sup" => ("<font size=\"12\">", "</font>"),
        "q" => ("\u{201C}", "\u{201D}"),
        "a" => ("<u><font color=\"#0000EE\">", "</font></u>"),
        _ => ("", ""),
    }
}

/// What an HTML attribute becomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mapped {
    /// The same value under a Roblox name: `src` to `Image`.
    Rename(&'static str),
    /// Properties with values Silk writes: `hidden` to `Visible={false}`.
    Props(Vec<(&'static str, String)>),
    /// A Roblox event that takes the handler as written.
    Event(&'static str),
    /// Silk reads it elsewhere: `id`, `class`, `style`, `href`, `type`.
    Read,
    /// Nothing on a Roblox instance, and nothing to report: `alt`, `aria-*`.
    Drop,
    /// Nothing on a Roblox instance; the lint says why.
    NoEffect(&'static str),
    /// A Roblox name, or a framework's own prop. It passes to the markup
    /// compiler as written.
    Keep,
    /// A name Silk does not know: it sets nothing, and the lint says so.
    Unknown,
}

/// The value of an attribute, as the source wrote it.
#[derive(Debug, Clone, Copy)]
pub enum Raw<'a> {
    Bare,
    /// The text between the quotes.
    Str(&'a str),
    Expr(&'a str),
}

impl Raw<'_> {
    /// The value as a Luau expression.
    pub fn luau(self) -> String {
        match self {
            Raw::Bare => "true".into(),

            Raw::Str(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),

            Raw::Expr(e) => e.trim().to_string(),
        }
    }

    /// Whether a boolean attribute is on: bare, `"true"`, or the name.
    pub fn on(self) -> Option<bool> {
        match self {
            Raw::Bare => Some(true),

            Raw::Str(s) => Some(!matches!(s, "false" | "0")),

            Raw::Expr(_) => None,
        }
    }

    fn truth(self) -> String {
        match self.on() {
            Some(b) => b.to_string(),

            None => self.luau(),
        }
    }

    fn not(self) -> String {
        match self.on() {
            Some(b) => (!b).to_string(),

            None => format!("not ({})", self.luau()),
        }
    }
}

/// The events a handler attribute names: the lower case HTML spelling,
/// the React spelling, the Roblox event, and what it does.
const EVENTS: &[(&str, &str, &str, &str)] = &[
    (
        "onclick",
        "onClick",
        "Activated",
        "fires when the element is clicked or tapped",
    ),
    (
        "onmousedown",
        "onMouseDown",
        "MouseButton1Down",
        "fires when the left button goes down on a button",
    ),
    (
        "onmouseup",
        "onMouseUp",
        "MouseButton1Up",
        "fires when the left button comes up on a button",
    ),
    (
        "oncontextmenu",
        "onContextMenu",
        "MouseButton2Click",
        "fires on a right click on a button",
    ),
    (
        "onmouseenter",
        "onMouseEnter",
        "MouseEnter",
        "fires when the pointer enters",
    ),
    (
        "onmouseover",
        "onMouseOver",
        "MouseEnter",
        "fires when the pointer enters",
    ),
    (
        "onmouseleave",
        "onMouseLeave",
        "MouseLeave",
        "fires when the pointer leaves",
    ),
    (
        "onmouseout",
        "onMouseOut",
        "MouseLeave",
        "fires when the pointer leaves",
    ),
    (
        "onmousemove",
        "onMouseMove",
        "MouseMoved",
        "fires when the pointer moves",
    ),
    (
        "onfocus",
        "onFocus",
        "Focused",
        "fires when a TextBox takes the focus",
    ),
    (
        "onblur",
        "onBlur",
        "FocusLost",
        "fires when a TextBox loses the focus",
    ),
    (
        "onchange",
        "onChange",
        "FocusLost",
        "fires when a TextBox commits: it loses the focus",
    ),
    (
        "onkeydown",
        "onKeyDown",
        "InputBegan",
        "fires on input that begins over the element",
    ),
    (
        "onkeyup",
        "onKeyUp",
        "InputEnded",
        "fires on input that ends over the element",
    ),
];

/// React events with no Roblox event behind them.
const NO_EVENTS: &[(&str, &str)] = &[
    ("ondblclick", "onDoubleClick"),
    ("oninput", "onInput"),
    ("onsubmit", "onSubmit"),
    ("onwheel", "onWheel"),
    ("onscroll", "onScroll"),
    ("onkeypress", "onKeyPress"),
];

pub fn event(name: &str) -> Option<(&'static str, &'static str)> {
    let lower = name.to_ascii_lowercase();

    EVENTS
        .iter()
        .find(|(n, ..)| *n == lower)
        .map(|(_, _, e, d)| (*e, *d))
}

/// The React spelling of an attribute that React spells in camel case,
/// when `name` is the lower case HTML one: `class` is `className`.
pub fn react_name(name: &str) -> Option<&'static str> {
    const NAMES: &[(&str, &str)] = &[
        ("class", "className"),
        ("for", "htmlFor"),
        ("tabindex", "tabIndex"),
        ("readonly", "readOnly"),
        ("maxlength", "maxLength"),
        ("minlength", "minLength"),
        ("autoplay", "autoPlay"),
        ("colspan", "colSpan"),
        ("rowspan", "rowSpan"),
        ("autofocus", "autoFocus"),
        ("contenteditable", "contentEditable"),
        ("spellcheck", "spellCheck"),
        ("crossorigin", "crossOrigin"),
        ("srcset", "srcSet"),
        ("accesskey", "accessKey"),
        ("enterkeyhint", "enterKeyHint"),
        ("inputmode", "inputMode"),
        ("autocomplete", "autoComplete"),
        ("playsinline", "playsInline"),
        ("referrerpolicy", "referrerPolicy"),
        ("datetime", "dateTime"),
        ("hreflang", "hrefLang"),
        ("novalidate", "noValidate"),
        ("formaction", "formAction"),
        ("enctype", "encType"),
    ];

    if name != name.to_ascii_lowercase() {
        return None;
    }

    NAMES
        .iter()
        .copied()
        .chain(EVENTS.iter().map(|(l, r, ..)| (*l, *r)))
        .chain(NO_EVENTS.iter().copied())
        .find(|(l, _)| *l == name)
        .map(|(_, r)| r)
}

/// The HTML attributes a tag takes, for completion: the name and what it
/// becomes.
pub fn attributes(tag: &Tag, input_type: Option<&str>) -> Vec<(&'static str, &'static str)> {
    let mut out = vec![
        (
            "id",
            "`Name`, which `#id` selectors and `href=\"#id\"` links match",
        ),
        (
            "className",
            "CollectionService tags, which `.class` selectors match; Enamel utilities when the project loads Enamel",
        ),
        (
            "style",
            "a table of CSS properties in camel case, as React takes it: `style={{ backgroundColor = \"#fff\", padding = 8 }}`",
        ),
        ("hidden", "`Visible={false}`"),
        ("tabIndex", "`SelectionOrder`"),
        ("title", "nothing: Roblox has no tooltips"),
        ("key", "the key the framework tracks the element by"),
    ];

    for (_, react, event, _) in EVENTS {
        let fits = match *event {
            "Focused" | "FocusLost" => matches!(tag.kind, Input | TextArea),

            _ => tag.kind != Audio,
        };

        if fits {
            out.push((react, event));
        }
    }

    let own: &[(&str, &str)] = match tag.kind {
        Link => &[("href", "`#id` scrolls to that element when clicked")],

        Image => &[
            ("src", "`Image`"),
            ("alt", "nothing: an ImageLabel has no text alternative"),
            ("width", "the width of `Size`, in pixels"),
            ("height", "the height of `Size`, in pixels"),
        ],

        Input => match input_type.unwrap_or("text") {
            "button" | "submit" | "reset" => &[
                ("type", "the kind of input"),
                ("value", "`Text`"),
                ("disabled", "`Interactable={false}`"),
            ],

            _ => &[
                (
                    "type",
                    "the kind of input: `text`, `password`, `number`, and the other text kinds",
                ),
                ("value", "`Text`"),
                ("defaultValue", "`Text`"),
                ("placeholder", "`PlaceholderText`"),
                ("readOnly", "`TextEditable={false}`"),
                (
                    "disabled",
                    "`TextEditable={false}` and `Interactable={false}`",
                ),
                ("maxLength", "nothing: a TextBox has no limit"),
            ],
        },

        TextArea => &[
            ("placeholder", "`PlaceholderText`"),
            ("readOnly", "`TextEditable={false}`"),
            ("defaultValue", "`Text`"),
            (
                "disabled",
                "`TextEditable={false}` and `Interactable={false}`",
            ),
            ("rows", "the height of `Size`"),
            ("cols", "the width of `Size`"),
        ],

        Button => &[
            (
                "disabled",
                "`Interactable={false}` and `AutoButtonColor={false}`",
            ),
            ("type", "nothing: Roblox has no forms"),
        ],

        Video => &[
            ("src", "`Video`"),
            ("autoPlay", "`Playing`"),
            ("loop", "`Looped`"),
            ("muted", "`Volume={0}`"),
            ("width", "the width of `Size`"),
            ("height", "the height of `Size`"),
        ],

        Audio => &[
            ("src", "`SoundId`"),
            ("autoPlay", "`Playing`"),
            ("loop", "`Looped`"),
            ("muted", "`Volume={0}`"),
        ],

        Canvas => &[
            ("width", "the width of `Size`"),
            ("height", "the height of `Size`"),
        ],

        Progress => &[
            ("value", "the fill, from `min` to `max`"),
            ("max", "the full value, 1 when unset"),
            ("min", "the empty value, 0 when unset"),
        ],

        List if tag.name == "ol" => &[
            ("start", "the number of the first item"),
            ("reversed", "nothing: the numbers count up"),
        ],

        Block if tag.name == "dialog" => &[("open", "`Visible`")],

        Cell => &[
            ("colSpan", "nothing: a UITableLayout has no spans"),
            ("rowSpan", "nothing: a UITableLayout has no spans"),
        ],

        _ => &[],
    };

    out.extend_from_slice(own);
    out.sort_by_key(|(n, _)| *n);
    out.dedup_by_key(|(n, _)| *n);

    out
}

/// What an attribute on a tag becomes.
pub fn map(tag: &Tag, name: &str, value: Raw, input_type: Option<&str>) -> Mapped {
    let lower = name.to_ascii_lowercase();

    // A Roblox name, `key`, or an ingot's prop passes as written.
    if name.chars().next().is_some_and(char::is_uppercase) || name == "key" {
        return Mapped::Keep;
    }

    if lower.starts_with("aria-") || lower.starts_with("data-") {
        return Mapped::Drop;
    }

    if let Some((event, _)) = event(&lower) {
        return Mapped::Event(event);
    }

    if NO_EVENTS.iter().any(|(l, _)| *l == lower) {
        return Mapped::NoEffect("a Roblox instance has no event for it");
    }

    if lower == "dangerouslysetinnerhtml" {
        return Mapped::NoEffect("Roblox parses no HTML at run time; write the markup");
    }

    match (tag.kind, lower.as_str()) {
        (_, "id" | "class" | "classname" | "style") => Mapped::Read,

        (Link, "href") => Mapped::Read,

        (Link, "target" | "rel" | "download" | "hreflang" | "referrerpolicy") => Mapped::Drop,

        (Input, "type") => Mapped::Read,

        (_, "hidden") => Mapped::Props(vec![("Visible", value.not())]),

        (_, "tabindex") => Mapped::Props(vec![(
            "SelectionOrder",
            value.luau().trim_matches('"').to_string(),
        )]),

        (Block, "open") if tag.name == "dialog" => Mapped::Props(vec![("Visible", value.truth())]),

        (Image | Video | Canvas, "width" | "height") => Mapped::Read,

        (Image, "src") => Mapped::Rename("Image"),

        (Image, "alt" | "loading" | "decoding" | "srcset" | "sizes" | "crossorigin") => {
            Mapped::Drop
        }

        (Video, "src") => Mapped::Rename("Video"),

        (Audio, "src") => Mapped::Rename("SoundId"),

        (Video | Audio, "autoplay") => Mapped::Props(vec![("Playing", value.truth())]),

        (Video | Audio, "loop") => Mapped::Props(vec![("Looped", value.truth())]),

        (Video | Audio, "muted") => Mapped::Props(vec![(
            "Volume",
            match value.on() {
                Some(true) => "0".to_string(),

                Some(false) => "0.5".to_string(),

                None => format!("if {} then 0 else 0.5", value.luau()),
            },
        )]),

        (Video | Audio, "controls" | "poster" | "preload" | "playsinline") => {
            Mapped::NoEffect("a Roblox media player shows no controls and no poster")
        }

        (Input | TextArea, "placeholder") => Mapped::Rename("PlaceholderText"),

        (Input | TextArea, "value" | "defaultvalue") | (Button, "value") => Mapped::Rename("Text"),

        (Input | TextArea, "readonly") => Mapped::Props(vec![("TextEditable", value.not())]),

        (Input, "disabled") if matches!(input_type, Some("button" | "submit" | "reset")) => {
            Mapped::Props(vec![("Interactable", value.not())])
        }

        (Input | TextArea, "disabled") => Mapped::Props(vec![
            ("TextEditable", value.not()),
            ("Interactable", value.not()),
        ]),

        (Button, "disabled") => Mapped::Props(vec![
            ("Interactable", value.not()),
            ("AutoButtonColor", value.not()),
        ]),

        (TextArea, "rows" | "cols" | "wrap") => Mapped::Read,

        (Progress, "value" | "max" | "min" | "low" | "high" | "optimum") => Mapped::Read,

        (List, "start" | "reversed" | "type") => Mapped::Read,

        (Text, "value") if tag.name == "li" => Mapped::Read,

        (
            Input | TextArea,
            "maxlength" | "minlength" | "pattern" | "required" | "min" | "max" | "step" | "name"
            | "autocomplete" | "form" | "size" | "list" | "inputmode" | "spellcheck"
            | "autocapitalize",
        ) => Mapped::NoEffect("a TextBox checks nothing the player types; check it in the handler"),

        (Button, "type" | "name" | "form" | "formaction" | "formmethod" | "popovertarget") => {
            Mapped::Drop
        }

        (Cell, "colspan" | "rowspan" | "headers" | "scope" | "abbr") => {
            Mapped::NoEffect("a UITableLayout has no cell spans")
        }

        (Block, "action" | "method" | "enctype" | "novalidate" | "name") if tag.name == "form" => {
            Mapped::NoEffect("Roblox has no form submission; handle the button's click")
        }

        (_, "title") => Mapped::NoEffect("Roblox has no tooltips"),

        (
            _,
            "lang" | "dir" | "translate" | "role" | "draggable" | "contenteditable" | "spellcheck"
            | "accesskey" | "autofocus" | "inert" | "is" | "part" | "slot" | "nonce" | "for"
            | "popover" | "enterkeyhint" | "itemprop" | "itemscope" | "itemtype" | "cite"
            | "datetime" | "align" | "border" | "cellpadding" | "cellspacing" | "valign"
            | "bgcolor" | "name",
        ) => Mapped::Drop,

        // A framework's own props.
        (_, "ref" | "children") => Mapped::Keep,

        // `htmlFor` ties a label to a control, which Roblox does not do;
        // a `details` shows its content, open or not.
        (_, "htmlfor") | (Block, "open") => Mapped::Drop,

        _ => Mapped::Unknown,
    }
}

/// The HTML character references Silk decodes: the common named ones.
/// `&lt;`, `&gt;`, `&amp;`, `&quot;`, and `&apos;` stay, because RichText
/// reads them itself.
pub const ENTITIES: &[(&str, &str)] = &[
    ("nbsp", "\u{A0}"),
    ("copy", "\u{A9}"),
    ("reg", "\u{AE}"),
    ("trade", "\u{2122}"),
    ("hellip", "\u{2026}"),
    ("mdash", "\u{2014}"),
    ("ndash", "\u{2013}"),
    ("lsquo", "\u{2018}"),
    ("rsquo", "\u{2019}"),
    ("ldquo", "\u{201C}"),
    ("rdquo", "\u{201D}"),
    ("bull", "\u{2022}"),
    ("middot", "\u{B7}"),
    ("times", "\u{D7}"),
    ("divide", "\u{F7}"),
    ("deg", "\u{B0}"),
    ("plusmn", "\u{B1}"),
    ("euro", "\u{20AC}"),
    ("pound", "\u{A3}"),
    ("yen", "\u{A5}"),
    ("cent", "\u{A2}"),
    ("sect", "\u{A7}"),
    ("para", "\u{B6}"),
    ("larr", "\u{2190}"),
    ("rarr", "\u{2192}"),
    ("uarr", "\u{2191}"),
    ("darr", "\u{2193}"),
    ("harr", "\u{2194}"),
    ("hearts", "\u{2665}"),
    ("star", "\u{2606}"),
    ("check", "\u{2713}"),
    ("laquo", "\u{AB}"),
    ("raquo", "\u{BB}"),
    ("frac12", "\u{BD}"),
    ("frac14", "\u{BC}"),
    ("frac34", "\u{BE}"),
];

/// The XML references RichText reads itself.
pub const RICH_ENTITIES: &[&str] = &["lt", "gt", "amp", "quot", "apos"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_is_sorted_and_every_tag_resolves() {
        assert!(TAGS.windows(2).all(|w| w[0].name < w[1].name));
        assert_eq!(class_of(tag("div").unwrap(), None), Some("Frame"));
        assert_eq!(
            class_of(tag("input").unwrap(), Some("submit")),
            Some("TextButton")
        );
        assert_eq!(class_of(tag("input").unwrap(), Some("checkbox")), None);
        assert_eq!(class_of(tag("img").unwrap(), None), Some("ImageLabel"));
        assert!(tag("marquee").is_none());
    }

    #[test]
    fn attributes_map_to_properties_and_events() {
        let img = tag("img").unwrap();
        let button = tag("button").unwrap();

        assert_eq!(
            map(img, "src", Raw::Str("rbxassetid://1"), None),
            Mapped::Rename("Image")
        );
        assert_eq!(
            map(button, "onClick", Raw::Expr("go"), None),
            Mapped::Event("Activated")
        );
        assert_eq!(
            map(button, "disabled", Raw::Bare, None),
            Mapped::Props(vec![
                ("Interactable", "false".into()),
                ("AutoButtonColor", "false".into())
            ])
        );
        assert_eq!(
            map(img, "hidden", Raw::Expr("x"), None),
            Mapped::Props(vec![("Visible", "not (x)".into())])
        );
        assert_eq!(map(img, "aria-label", Raw::Str("a"), None), Mapped::Drop);
        assert_eq!(react_name("class"), Some("className"));
        assert_eq!(react_name("onclick"), Some("onClick"));
        assert_eq!(react_name("onClick"), None);
        assert_eq!(react_name("src"), None);
        assert_eq!(
            map(img, "BackgroundColor3", Raw::Expr("c"), None),
            Mapped::Keep
        );
    }
}
