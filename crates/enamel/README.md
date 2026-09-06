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
| `w-[200px]`, `bg-[#ff0000]`, `text-[14px]` | arbitrary values |

A margin, a shadow, or a cursor has no property on a GuiObject: the
class parses and the `no_effect` lint says so. A text utility on a
`Frame` gets `wrong_element`. `ClassName` on a component gets
`not_an_element`, which fails the build by default.

In the editor, hover on a class shows what it sets, completion lists
every utility with a swatch beside each color, and a color class shows
its swatch in the text.

## Fonts

Every family Roblox ships is a class, by the name `Enum.Font` gives it:
`font-gotham`, `font-source-sans`, `font-roboto-mono`, `font-arcade`,
`font-builder-sans`, and the rest, with the weighted items as aliases,
`font-gotham-bold`. `font-bold`, `font-medium`, and `italic` combine
with any of them into one `FontFace`.

## The theme file

`enamel.luau` at the project root holds the colors, fonts, and classes
of the project, written as Luau with the Roblox API. Enamel never runs
the file: each entry is the text of an expression, copied into the
generated markup the way a macro copies its body, and the statements
before the `return` are copied once into any `.alx` file that uses an
entry, so a local defined there is in scope.

```luau
local purple = Color3.fromRGB(138, 61, 245)

return {
    colors = { brand = purple, accent = "#ff8a00" },
    fonts = { title = Font.new("rbxasset://fonts/families/Montserrat.json", Enum.FontWeight.Bold) },
    classes = {
        card = "bg-slate-900/80 rounded-xl p-4",
        glow = { BackgroundColor3 = purple, ZIndex = 2 },
    },
}
```

A color name works wherever a palette name does, `bg-brand`,
`text-brand/50`, `from-accent`; a font name as `font-title`, and
`font-bold` beside it keeps the family and sets the weight; a class
name expands to its list, or sets its properties as written. The editor
reads the file the way it reads any Luau, with the Roblox API, and
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
