//! What an Enamel class sets. Silk runs before Enamel and writes the
//! defaults a browser gives each tag. Where a class of the element sets
//! the same property, Silk leaves its default out, so the element has
//! one value and not two.
//!
//! The names follow Enamel's utilities. A key is a Roblox property, a
//! modifier class Enamel adds (`UIPadding`), or one of the keys below.

use std::collections::HashSet;

/// A class that lays the children out in a row: `flex`, `flex-row`.
pub const ROW: &str = "layout:row";
/// `flex-col`.
pub const COLUMN: &str = "layout:column";
/// `grid`, `grid-cols-3`.
pub const GRID: &str = "layout:grid";
/// A class that arranges the children in the layout: `gap-2`,
/// `justify-between`, `items-center`.
pub const ARRANGE: &str = "layout:arrange";
/// The width of `Size`, and the X axis of `AutomaticSize`.
pub const SIZE_X: &str = "size:x";
pub const SIZE_Y: &str = "size:y";
pub const AUTO_X: &str = "auto:x";
pub const AUTO_Y: &str = "auto:y";

/// Every key the classes of one element set. A class with a state
/// variant, `hover:bg-red-500`, applies on that state alone, so the
/// element keeps the default for its resting look.
pub fn sets<'a>(classes: impl IntoIterator<Item = &'a str>) -> HashSet<String> {
    classes
        .into_iter()
        .filter(|c| !has_variant(c))
        .flat_map(|c| utility(c.strip_prefix('-').filter(|b| !b.is_empty()).unwrap_or(c)))
        .map(|k| (*k).to_string())
        .collect()
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
        let mut out: Vec<String> = sets(classes.split_whitespace()).into_iter().collect();
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
}
