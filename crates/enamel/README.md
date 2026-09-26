# Enamel

Utility classes for Alloy markup. On a Roblox GUI element in a `.alx`
file, `ClassName` holds Tailwind's class names, and the transform turns
them into the properties and the layout children behind them:

```alx
<Frame ClassName="flex-col gap-2 p-4 bg-slate-900/80 rounded-xl w-56 h-32">
    <TextLabel ClassName="w-full h-7 text-white text-scaled font-bold">Coins: 0</TextLabel>
    <TextButton ClassName="w-full h-9 bg-red-500 hover:bg-red-600 rounded-lg text-white">Swing</TextButton>
</Frame>
```

The `Frame` gets `BackgroundColor3`, `BackgroundTransparency`, and
`Size`, plus a `UIListLayout`, a `UIPadding`, and a `UICorner` child.
The button's `hover:` wraps the element in a one-line helper that
connects `MouseEnter` and `MouseLeave`.

## What maps

| Classes | Roblox |
| --- | --- |
| `w-*`, `h-*`, `size-*`, fractions, `full`, `auto` | `Size`, `AutomaticSize` |
| `top-*`, `left-*`, `right-*`, `bottom-*`, `inset-*`, `-translate-x-1/2` | `Position`, `AnchorPoint` |
| `flex`, `flex-col`, `gap-*`, `justify-*`, `items-*`, `flex-wrap` | `UIListLayout` |
| `grid`, `grid-cols-N`, `auto-rows-N` | `UIGridLayout` |
| `grow`, `shrink`, `self-*` | `UIFlexItem` |
| `p-*`, `px-*`, `pt-*` and the rest | `UIPadding` |
| `bg-*`, `text-*`, `border-*`, `image-*`, `placeholder-*`, with `/50` | `BackgroundColor3`, `TextColor3`, `UIStroke`, and their transparencies |
| `bg-gradient-to-*`, `from-*`, `via-*`, `to-*` | `UIGradient` |
| `rounded-*`, `border-*`, `ring-*` | `UICorner`, `UIStroke` |
| `text-xs` to `text-9xl`, `text-left`, `truncate`, `text-wrap`, `text-scaled`, `leading-*` | `TextSize`, `TextXAlignment`, `TextTruncate`, `TextWrapped`, `TextScaled`, `LineHeight` |
| `font-sans`, `font-mono`, `font-bold`, `italic` | `FontFace` |
| `opacity-*`, `hidden`, `z-*`, `order-*`, `rotate-*`, `scale-*` | transparencies, `Visible`, `ZIndex`, `LayoutOrder`, `Rotation`, `UIScale` |
| `min-w-*`, `max-w-*`, `aspect-*` | `UISizeConstraint`, `UIAspectRatioConstraint` |
| `object-cover`, `object-contain` | `ScaleType` |
| `overflow-hidden`, `overflow-scroll`, `scrollbar-*` | `ClipsDescendants`, `ScrollingEnabled`, `ScrollBarThickness` |
| `hover:`, `active:`, `focus:`, `group`, `group-hover:` | the state helper |
| `transition`, `transition-colors`, `duration-300`, `duration-[0.3s]`, `ease-out`, `ease-back-in`, `delay-100` | a TweenService tween on each state change |
| `w-[200px]`, `bg-[#ff0000]`, `text-[14px]` | arbitrary values |

A `transition` makes each state change a tween: 150ms, Quad in-out,
over the colors, the transparencies, position, size, rotation, scale,
and stroke width.
`transition-colors`, `transition-opacity`, `transition-transform`, and
`transition-all` narrow or widen that. `duration-*` and `delay-*` take
milliseconds, or a bracket time with a unit, `[0.3s]`. `ease-*` names a
Roblox easing style, `ease-back`, `ease-bounce`, `ease-expo`, with
`-in`, `-out`, or `-in-out` behind it; `ease-in`, `ease-out`, and
`ease-in-out` are Quad. A property a tween cannot move, a font or a
boolean, is set at once.

A state changes a property of the element, and also a scale or a
stroke: `hover:scale-110`, `hover:ring-4`, and `hover:stroke-yellow-400`
change the UIScale or the UIStroke child. With no such class at rest,
the child starts at a scale of 1 or a stroke of no width, as in
Tailwind. A padding, a layout, or a corner cannot change with a state,
and the `no_effect` lint says so.

A margin, a shadow, or a cursor has no property on a GuiObject: the
class parses and the `no_effect` lint says so. A text utility on a
`Frame` gets `wrong_element`. `ClassName` on a component gets
`not_an_element`, which fails the build by default.

In the editor, `ClassName` completes as a prop name on any GUI tag, the
way a Roblox property does; hover on a class shows what it sets;
completion lists every utility with a swatch beside each color; and a
color class shows its swatch in the text.

A scale has no end, so the list behind a number comes from the digits
typed: `w-1` offers `w-1`, `w-10` through `w-19`, and `w-1/2` through
`w-1/12`, each with the size it sets. `p-`, `gap-`, `z-`, `opacity-`,
`duration-`, `text-size-`, and the other heads that take a number work
the same. A name with no number, `rounded-`, `bg-red-`, keeps its fixed
list.

## Roblox properties by name

Beside Tailwind's names, the properties a GuiObject has get words of
their own, so the file reads like the Explorer:

| Class | Property |
| --- | --- |
| `bg-transparency-50`, `text-transparency-50`, `image-transparency-50`, `group-transparency-50`, `transparency-50` | the `Transparency` properties, `[0.35]` for a ratio |
| `stroke`, `stroke-2`, `stroke-red-500`, `stroke-transparency-25`, `stroke-contextual`, `stroke-round` | a `UIStroke`: thickness, color, transparency, apply mode, join |
| `text-stroke-black`, `text-stroke-transparency-50` | `TextStrokeColor3` and `TextStrokeTransparency` |
| `text-size-18` | `TextSize` in pixels |
| `anchor-center`, `anchor-tl` through `anchor-br`, `center` | `AnchorPoint`, and `center` with the position that centers |
| `clip`, `no-clip` | `ClipsDescendants` |
| `layout-3`, `display-3` | `LayoutOrder` and `DisplayOrder` |
| `sort-name`, `sort-order` | the list layout's `SortOrder` |
| `fill` | a `UIFlexItem` with `FlexMode.Fill` |
| `image-slice`, `image-tile`, `image-transparent` | `ScaleType` and `ImageTransparency` |
| `scroll-x`, `scroll-y`, `scroll-xy`, `no-scroll`, `canvas-auto`, `canvas-h-400`, `canvas-w-200`, `elastic`, `no-elastic`, `scrollbar-4` | a `ScrollingFrame`'s direction, canvas, elastic behavior, bar |
| `auto-color`, `no-auto-color`, `modal` | a button's `AutoButtonColor` and `Modal` |
| `ignore-inset`, `reset-on-spawn`, `keep-on-spawn`, `sibling-z`, `global-z` | a `ScreenGui`'s inset, reset, and `ZIndexBehavior` |
| `max-graphemes-20` | `MaxVisibleGraphemes` |

## Brackets

A value in brackets is as written, on any utility that takes a number,
a length, a color, or a ratio: `w-[200px]`, `w-[50%]`, `p-[10px]`,
`gap-[6px]`, `rounded-[12px]`, `rounded-[50%]`, `text-[14px]`,
`text-size-[14px]`, `z-[100]`, `order-[7]`, `rotate-[45deg]`,
`scale-[1.2]`, `aspect-[4/3]`, `leading-[1.4]`, `stroke-[3px]`,
`stroke-transparency-[0.4]`, `bg-transparency-[35%]`, and the other
transparencies. A color in brackets is a hex, an `rgb(r,g,b)`, or any
Luau that makes one, `bg-[Color3.fromHSV(0.5,_1,_1)]`, with `_` for a
space; `image-[rbxassetid://123]` sets the image itself, and
`font-[Montserrat]` names a family file, or a whole path with `://`.

## Fonts

Every family Roblox ships is a class, by the name `Enum.Font` gives it:
`font-gotham`, `font-source-sans`, `font-roboto-mono`, `font-arcade`,
`font-builder-sans`, and the rest, with the weighted items as aliases,
`font-gotham-bold`. `font-bold`, `font-medium`, and `italic` combine
with any of them into one `FontFace`.

## The theme file

`enamel.aly` at the project root holds the colors, fonts, and classes
of the project, written as Alloy with the Roblox API. Enamel never runs
the file: each entry is the text of an expression, copied into the
generated markup the way a macro copies its body, and the other
statements are copied once into any `.alx` file that uses an entry, so
a local defined there is in scope.

The file is a module that exports the three tables, which game code
may import as well:

```alloy
local purple = Color3.fromRGB(138, 61, 245)

export const colors = { brand = purple, accent = "#ff8a00" }
export const fonts = { title = Font.new("rbxasset://fonts/families/Montserrat.json", Enum.FontWeight.Bold) }
export const classes = {
    card = "bg-slate-900/80 rounded-xl p-4",
    glow = { BackgroundColor3 = purple, ZIndex = 2 },
}
```

A `return { colors = ..., fonts = ..., classes = ... }` of one table
works the same, and there a table may be the name of one defined above
the return.

A color entry may name a local, `brand = purple`; the swatch reads
through to what the local holds. The tables are typed for the editor,
so a value of the wrong kind is marked there too.

A color name works wherever a palette name does, `bg-brand`,
`text-brand/50`, `from-accent`; a font name as `font-title`, and
`font-bold` beside it keeps the family and sets the weight; a class
name expands to its list, or sets its properties as written. The editor
reads the file the way it reads any Alloy, with the Roblox API, and
completes the entries in `ClassName` strings; a color it can read, a
`Color3` call or a hex string, gets its swatch. A structural problem in
the file is the `theme` lint, on the file itself.

## Options

```toml
[ingot.enamel]
font_sans = "rbxasset://fonts/families/GothamSSm.json"
font_serif = "rbxasset://fonts/families/Merriweather.json"
font_mono = "rbxasset://fonts/families/RobotoMono.json"
helper = "__enamel"
```
