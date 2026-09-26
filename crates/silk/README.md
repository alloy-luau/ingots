# Silk

HTML and CSS in Alloy markup, written the way React and TSX write them.
In a `.alx` file, `<div>`, `<p>`, `<button>`, and the other HTML elements
become the Roblox instances behind them. Their attributes become
properties, and a `<style>` element becomes a StyleSheet of StyleRules.

```alx
export function Shop(props: { name: string, onBuy: () -> () })
    return <div style={{ backgroundColor = "#101014", padding = 16 }}>
        <style>
            :root { --accent: #7c5cff; }
            .card { background-color: #1b1b22; border-radius: 8px; padding: 12px; }
            .card:hover { background-color: var(--accent); }
            .row { display: flex; gap: 8px; align-items: center; }
        </style>
        <h1>Hello, <b>{props.name}</b>!</h1>
        <p className="card">Welcome &mdash; pick an item.</p>
        <div className="row">
            <img src="rbxassetid://123" width="32" height="32" />
            <button onClick={props.onBuy}>Buy</button>
            <a href="#refunds">Refunds</a>
        </div>
        <div id="faq" style={{ overflow = "auto", height = 120 }}>
            <p>How do I buy?</p>
            <p id="refunds">Can I get a refund?</p>
        </div>
    </div>
end
```

The outer `div` is a Frame with a UIPadding and a UIListLayout. The `h1`
is a bold 32 px TextLabel whose text is RichText. The `style` element is
a StyleLink to a StyleSheet with a StyleRule for each selector. The `#faq`
box is a ScrollingFrame, and the link scrolls it to the refunds question.

## The React rules

Silk follows React where React and HTML differ:

- A void tag closes itself: `<br />` and `<img />`, never `<br>`.
- Attributes use camel case: `className`, `htmlFor`, `tabIndex`,
  `readOnly`, and `onClick`. The `react_name` lint reports `class`,
  `onclick`, and the other HTML spellings, and its fix writes the React
  name.
- `style` takes a table of camel case properties:
  `style={{ backgroundColor = "#fff", padding = 8 }}`. A number is
  pixels, except for the unitless properties React knows (`opacity`,
  `zIndex`, `flexGrow`, `fontWeight`, `lineHeight`, and the rest). A
  `style` string is a `react_style` error.
- A value in `style` can be Luau, as in React, for a property with one
  Roblox property behind it: `backgroundColor`, `color`,
  `borderColor`, `borderWidth`, `rotate`, `scale`, and `zIndex`. The
  value is what the Roblox property takes, a `Color3` or a number, and
  a source stays live: `style={{ scale = grow }}` sets the `Scale` of
  a UIScale. Another property takes a literal alone.
- Text takes HTML character references: `&copy;`, `&nbsp;`, `&mdash;`,
  `&#169;`.

Real CSS goes in a `<style>` element. Its text runs to `</style>`, as HTML
reads it, so `--name` and `{` mean CSS there. The React form works too:
`<style>{[[ .a { color: red } ]]}</style>`.

## Elements

| Elements | Roblox |
| --- | --- |
| `div`, `section`, `article`, `main`, `header`, `footer`, `nav`, `aside`, `form`, `figure`, and the other boxes | a Frame, full width, with a UIListLayout that stacks the children in source order |
| `p`, `h1` to `h6`, `li`, `dt`, `dd`, `pre`, `figcaption`, `caption`, `legend`, `summary` | a TextLabel, full width, text wrapped, with the browser's size and weight |
| `span`, `label`, `small`, `code`, and the other inline tags on their own | a TextLabel that sizes to its text |
| `b`, `strong`, `i`, `em`, `u`, `s`, `code`, `mark`, `small`, `br`, `q`, and `span` with a `style`, inside text | RichText tags in the text: `<b>`, `<i>`, `<u>`, `<s>`, `<font>`, `<mark>`, `<br />` |
| `button` | a TextButton with the browser's padding, border, and corner |
| `a` | a TextButton with link text; `href="#id"` scrolls the ScrollingFrame that holds the element with that `id` to it, and `href="#"` scrolls the link's own ScrollingFrame to the top |
| `input` | a TextBox, or a TextButton for `type="button"`, `"submit"`, and `"reset"` |
| `textarea` | a TextBox with `MultiLine` |
| `img`, `video`, `audio`, `canvas` | an ImageLabel, a VideoFrame, a Sound, and a CanvasGroup |
| `ul`, `ol` | a Frame with a left padding; each `li` starts with a bullet or its number |
| `table`, `tr`, `td`, `th` | a Frame with a UITableLayout, Frames for the rows, TextLabels for the cells; `thead`, `tbody`, and `tfoot` are fragments |
| `hr` | a Frame one pixel high |
| `progress`, `meter` | a bar with a fill for `value` of `max` |
| `style` | a StyleLink to a StyleSheet |
| `html`, `head`, `body`, `title`, `meta` | a ScreenGui, configured by its head, with a Frame that fills the screen; see [The document](#the-document) |

A box that holds only text becomes a TextLabel. A `{ }` hole is not text:
it may hold elements, as `{children}` does. So `<div>{name}</div>` stays
a Frame and shows nothing. Write `<p>{name}</p>`, or text beside the
hole, as in `<div>Name: {name}</div>`. A hole beside a `Text` you write
is a child too: `<button Text={label}>{icon}</button>`. A text element
that holds boxes becomes a Frame, and each run of its text becomes a
TextLabel of its own. An element with `onClick` becomes a button: a Frame is a TextButton,
and an ImageLabel is an ImageButton.

Each child of a box takes the `LayoutOrder` of its place, so the
UIListLayout keeps the source order. A hole and a component take theirs
through the `__silk_order` helper, which sets it on the instance they
give, on each instance of a list in turn, and again each time a
function child runs. In a box that holds a hole, one place is 1000
numbers wide, so a list in the hole keeps its own order.

A child that places itself stands out of the flow, as `position:
absolute` does in CSS. It has a `Position` attribute, `position:
absolute` or `fixed`, an offset such as `top` or `inset`, or an Enamel
class such as `absolute`, `inset-0`, or `center`. A UIListLayout would
move that child, so its box writes no layout, and each child stands
where it places itself, as in a Roblox Frame. A layout that the author
asks for, `display: flex` or a `flex` class, stays. A button keeps its
text beside such a child. So a screen of layers is a box of absolute
children:

```alx
<div className="w-full h-full">
    <div className="absolute inset-0">{background}</div>
    <main className="center w-[640px] p-6">...</main>
</div>
```

A child of a flex or a grid box is an item of its own, as CSS makes it:
`<div className="flex"><span>L</span><span>R</span></div>` is a Frame
with two TextLabels in a row. An inline tag with a class keeps its own
instance too, so the class has something to style, unless text stands
beside it. In `<p>Hi <b className="x">there</b></p>` the `b` is RichText,
and the `no_effect` lint says that the class sets nothing.

`select`, `iframe`, `svg`, `script`, a checkbox, and the other elements
with no Roblox form are `unsupported_tag` errors.

## The document

A component can return a whole document. `<html>` is a ScreenGui.
`<head>` makes no instance: its tags configure the ScreenGui. `<body>` is
a Frame that fills the screen, and it takes classes and children as a
`div` does.

```alx
export function App(props: { title: () -> string })
    return <html>
        <head>
            <title>{props.title}</title>
            <meta name="display-order" content="10" />
            <meta name="ignore-inset" />
            <meta name="reset-on-spawn" content="false" />
            <meta name="viewport" content="width=1280, height=720, minimum-scale=0.5, maximum-scale=2" />
            <style>
                .panel { background-color: #101014; border-radius: 12px; }
            </style>
        </head>
        <body>
            <main className="panel">Welcome</main>
        </body>
    </html>
end
```

| In the head | Roblox |
| --- | --- |
| `<title>` | the `Name` of the ScreenGui |
| `<meta name="display-order" content="10" />` | `DisplayOrder` |
| `<meta name="ignore-inset" />` | `ScreenInsets = None`; `content="false"` keeps `CoreUISafeInsets` |
| `<meta name="reset-on-spawn" content="false" />` | `ResetOnSpawn` |
| `<meta name="viewport" content="..." />` | the body in design pixels, scaled to the screen |
| `<style>` | a StyleLink of the ScreenGui, so its rules reach the whole document |

A meta with no `content` is on. A `{ }` value in `content` or in the
title is a Luau value, so a source stays live: `content={order}`. For
`ignore-inset`, such a value sets `IgnoreGuiInset`, which takes a
boolean as it is. `<meta charset>` sets nothing, and another name
reports `no_effect`. The ScreenGui stacks by `ZIndexBehavior.Sibling`,
as CSS stacks `z-index` among siblings.

The viewport takes the keys of HTML's viewport: `width` and `height` in
design pixels, `minimum-scale` and `maximum-scale` (`min-scale` and
`max-scale` also work), and `initial-scale` when there is no design
size. A UIScale scales the body to the camera's `ViewportSize`. The
scale is the smaller of the two ratios of the screen to the design size,
clamped to the bounds. The body is the screen divided by that scale, so
every size inside it reads in design pixels. `width=device-width,
initial-scale=1` scales nothing. The UIScale is the body's child
`viewport`, so code can read the scale from its `Scale`.

The viewport needs the body as an instance. In the table form, the
helper builds the body and fits it. In the element form, React gives
the instance to the helper as a `ref`, through a spread.

## Attributes

| Attribute | Roblox |
| --- | --- |
| `id` | `Name`, which `#id` selectors and links match |
| `className` | CollectionService tags, which `.class` selectors match |
| `style` | properties and modifier children |
| `hidden` | `Visible={false}` |
| `tabIndex` | `SelectionOrder` |
| `src` | `Image`, `Video`, or `SoundId` |
| `width`, `height` on media | `Size` |
| `placeholder`, `value`, `readOnly`, `disabled` | `PlaceholderText`, `Text`, `TextEditable`, `Interactable` |
| `autoPlay`, `loop`, `muted` | `Playing`, `Looped`, `Volume` |
| `onClick`, `onMouseEnter`, `onMouseLeave`, `onMouseDown`, `onMouseUp`, `onMouseMove`, `onContextMenu`, `onFocus`, `onBlur`, `onKeyDown`, `onKeyUp` | `Activated`, `MouseEnter`, `MouseLeave`, `MouseButton1Down`, `MouseButton1Up`, `MouseMoved`, `MouseButton2Click`, `Focused`, `FocusLost`, `InputBegan`, `InputEnded` |
| `onChange`, `onInput`, `maxLength` on a text input | a listener on `Text` that the helper connects |

`onChange` and `onInput` run on each key, as React runs `onChange`. The
handler takes the new text, so a source works as one:
`<input onChange={name} />` writes into `name`. `maxLength` cuts the text
to that many characters. `onBlur` runs when the input loses the focus.

`hidden`, `readOnly`, and `disabled` negate their value. A source stays
live: `disabled={busy}` writes a function that reads `busy`, so the
button follows it.

`aria-*`, `data-*`, `alt`, and the other attributes with nothing behind
them drop without a word. `title`, `minLength`, `colSpan`, and a few
others drop with a `no_effect` warning.

## CSS

A `style` table and a `<style>` rule take the same properties:

| CSS | Roblox |
| --- | --- |
| `background-color`, `background`, `linear-gradient()`, `color`, `opacity` | `BackgroundColor3`, a UIGradient, `TextColor3` or `ImageColor3`, and the transparencies |
| `width`, `height`, `min-*`, `max-*`, `aspect-ratio` | `Size`, `AutomaticSize`, a UISizeConstraint, a UIAspectRatioConstraint |
| `padding` | a UIPadding |
| `border`, `border-width`, `border-color`, `border-style`, `outline` | a UIStroke |
| `border-radius` | a UICorner |
| `display: flex`, `flex-direction`, `justify-content`, `align-items`, `gap`, `flex-wrap` | the UIListLayout |
| `display: grid`, `grid-template-columns`, `grid-auto-rows`, `gap` | a UIGridLayout |
| `flex`, `flex-grow`, `flex-shrink`, `align-self`, `order` | a UIFlexItem, `LayoutOrder` |
| `position`, `top`, `left`, `right`, `bottom`, `transform: translate()` | `Position`, `AnchorPoint` |
| `transform: rotate()`, `rotate`, `transform: scale()`, `scale`, `z-index` | `Rotation`, a UIScale, `ZIndex` |
| `display: none`, `visibility`, `overflow`, `pointer-events` | `Visible`, `ClipsDescendants`, a ScrollingFrame, `Interactable` |
| `appearance: none` on a `button`, an `input`, or a `textarea` | no background, border, corner, or padding from the browser's look |
| `font`, `font-family`, `font-size`, `font-weight`, `font-style`, `line-height` | `FontFace`, `TextSize`, `LineHeight` |
| `text-align`, `vertical-align`, `white-space`, `text-overflow` | `TextXAlignment`, `TextYAlignment`, `TextWrapped`, `TextTruncate` |
| `text-decoration`, `text-transform` in a `style` table | RichText `<u>`, `<s>`, `<uc>`, `<sc>` |
| `object-fit`, `image-rendering` | `ScaleType`, `ResampleMode` |
| `var(--name)` | the value a `:root` rule of the file gives it |

Colors are hex, `rgb()`, `rgba()`, `hsl()`, the CSS names, and
`transparent`. Lengths are `px`, `%`, `em`, `rem`, `pt`, `vw`, `vh`, and
`calc(100% - 20px)`, which is a UDim of a scale and an offset. A Roblox
property works in CSS as itself: `BackgroundColor3: #000;`.

A `<style>` selector writes a StyleRule selector:

| CSS | Roblox |
| --- | --- |
| `.card` | `.card`, a CollectionService tag |
| `#main`, `h1` | `#main`, `#h1`: the instance's `Name` |
| `.a > .b`, `.a .b` | `.a > .b`, `.a >> .b` |
| `:hover`, `:active`, `:disabled` | `:Hover`, `:Press`, `:NonInteractable` |
| `input::placeholder { color }` | `PlaceholderColor3` |
| `Frame`, `::UICorner` | as written, in a project that allows Roblox names |

A modifier property writes a rule on the pseudo-instance, `.card::UICorner`,
and a layout property writes one on the layout child, `.row > UIListLayout`.
The priority is the CSS specificity, then the order of the rules, and
`!important` wins over both. `margin`, `box-shadow`, `cursor`,
`transition`, and a few others set nothing and report `no_effect`.
Sibling combinators, attribute selectors, most pseudo-classes,
pseudo-elements, and at-rules report `unsupported_css`.

## With Enamel

In a project that loads Enamel, `className` holds both kinds of class.
Silk passes the list to Enamel as `ClassName`, and Enamel writes its
utilities. Silk leaves out each default that a utility sets, and only
that one: `text-white` replaces the text color and keeps the size of an
`h1`, and `bg-gradient-to-b` adds a UIGradient and keeps the clear
background. A class with a state variant, `hover:bg-red-500`, keeps the
default for the resting look.

A class of the project's `enamel.aly` counts as the utilities or the
properties behind it. With `panel = 'bg-glass/75 rounded-2xl stroke'`,
the class replaces the background, the corner, and the border of a
`<button className="panel">`. A `w-` or `h-` class sets its axis inside
the `Size` Silk writes, and the other axis keeps Silk's size. Enamel
writes `AutomaticSize`, so Silk passes its own automatic axis on as a
class: `<div className="w-full">` keeps its automatic height as
`w-full h-auto`.

`appearance-none` drops the browser's look of a control, as
`appearance: none` does, so `<button className="appearance-none">` is a
bare TextButton that the other classes style.

An overflow class makes the box a ScrollingFrame, as `overflow: auto`
does: `overflow-y-auto` scrolls down, `overflow-x-auto` sideways, and
`overflow-auto` both ways. The canvas grows with the content.

Every class also becomes a tag, so `.card` in a `<style>` still
matches. In the editor, Enamel completes and explains its utilities in
`className`, and does not report a class it does not know on an HTML
element, since that is a CSS class.

## The table form

Vide and Fusion use the table form of `[alx.factory]`. Silk reads the
factory where Alloy does: `alloy.toml`, else `.config.aly`, and
`luaux.toml` when that file sets no factory. Vide types its factory over 19 classes, with no UIPadding, UIStroke,
StyleLink, VideoFrame, or Sound, and types each event's handler by its
signal. So in the table form, Silk writes each child it adds, and each
`<video>` and `<audio>`, as the `__silk_child` component, which calls
`Instance.new`. A child with a Luau value from `style` also takes
`Make`, the `create` of the factory, so the library binds a source in
it. It passes each handler through `__silk_on`, so a
`() -> ()` fits `Activated`. The element form, for React, keeps the
Roblox tags and the handlers as written.

## Options

```toml
[ingot.silk]
roblox = true      # false: a Roblox class, property, or CSS name is an error and leaves completion
tags = true        # className becomes CollectionService tags
helper = "__silk"  # the name of the helper the transform writes
font = "rbxasset://fonts/families/SourceSansPro.json"
font_serif = "rbxasset://fonts/families/Merriweather.json"
font_mono = "rbxasset://fonts/families/RobotoMono.json"
color = "#000000"  # the text color by default
```

## The editor

Completion lists the HTML elements after `<`, with the components and,
when `roblox` is on, the Roblox classes. In an open tag it lists the
element's attributes and, when `roblox` is on, the properties of the
class the tag becomes. It also completes `className` with the classes the
file's `<style>` styles, `href` with the file's ids, an input `type`, the
keys and values of a `style` table, and the selectors, properties, and
values of a `<style>`. Hover explains a tag, an attribute, a CSS property,
and a selector with the StyleRule selector it writes. Every CSS color
shows its swatch.

## Limits

- Tags, links, `onChange`, and `maxLength` need a target whose elements
  are instances, as Vide and Fusion build them: the helper calls `AddTag`
  and connects `Activated` and the `Text` signal on the element. On
  React, set `tags = false`.
- A StyleRule overrides a property set on the instance. So a `<style>`
  rule wins over a `style` table, which is the reverse of CSS.
- A rule sets `Size` and `FontFace` whole. A rule with `width` alone takes
  an automatic height, and a rule with `font-weight` alone takes the
  default family.
- A type selector matches the instance's `Name`. An element with an `id`
  has that `id` as its `Name`, so `h1` does not match `<h1 id="title">`;
  `#title` does.
- A Roblox Frame has a layout for all its children or for none. When one
  child places itself, the others stand at the top left of the box.
  Put the children that flow in a box of their own.
