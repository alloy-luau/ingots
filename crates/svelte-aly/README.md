# svelte-aly

Svelte's syntax for Alloy. The ingot compiles the runes and the blocks of
Svelte to calls of the
[svelte-aly](https://github.com/alloy-luau/packages/tree/main/svelte-aly)
package. The package works without the ingot; the ingot makes the code read
as Svelte does.

```alx
<script>
  let count = $state(0)
  let doubled = $derived(count * 2)
</script>

<TextButton Size={new UDim2(0, 260, 0, 40)} Activated={function() count += 1 end}>
  Clicked {count} times (doubled: {doubled})
</TextButton>

{#if count > 5}
  <TextLabel Size={new UDim2(0, 260, 0, 30)}>That's a lot of clicks</TextLabel>
{/if}
```

## Install

The ingot needs Alloy 0.1.8 or later. An older compiler rejects its
manifest: 0.1.7 has no `tokens` hook, and 0.1.6 cannot read `[macros]` or
`<script>`.

Install the package as its README says, then name the ingot in `alloy.toml`
and run `alloy ingot install`:

```toml
[ingots]
svelte-aly = { repo = "alloy-luau/ingots" }
```

## What it writes

Every edit keeps each line where it was, so each report points at the line
you wrote.

| You write | The ingot writes |
| --- | --- |
| `let n = $state(0)` | `local n = __svelte.state(0)`, or `const` when no code writes `n` |
| a read or a write of `n` | `n.value` |
| `let d = $derived(expr)` | `const d = __svelte.derived(function() return expr end)` |
| `$derived.by(fn)`, `$effect(fn)` | `__svelte.derived(fn)`, `__svelte.effect(fn)` |
| `let { a } = $props()` | `const { a } = (props :: any)`; each read of `a` is `__svelte.get(a)` |
| `{expr}` that reads a rune | `{function() return expr end}`; on a component tag, a derived value |
| `{#if c}` ... `{:else if d}` ... `{:else}` ... `{/if}` | one `__svelte.if_block(...)` hole |
| `{#each xs as x, i (key)}` ... `{:else}` ... `{/each}` | one `__svelte.each_block(...)` hole |
| `function name()` in a `<script>` | `local function name()` |

A file with a rune or a block gets `import * as __svelte from
'@packages/svelte-aly'` on its first line. A component file gets the markup
factory too, `import { create }`, when the script does not bind one. The
compiler then makes the script the body of the component.

The ingot reads names, not types. A name that a rune declares is the rune
everywhere in the file, so a second binding with that name is the
`shadowed_rune` error.

## In the editor

The runes are macros to the editor. The manifest declares them under
`[macros]`, so `$` lists `$state`, `$derived`, `$derived.by`, `$effect`,
`$effect.pre`, and `$props` beside the intrinsics, and a hover on one shows
what it does. The compiler itself knows none of them.

- A hover on a block tag, such as `{#if` or `{/each}`, explains the block.
- After `{#`, `{:`, or `{/`, completion offers the block tags as snippets.
- A rune reads as its value to the type check: `items:` completes the
  methods of the list, and the item of an `{#each}` takes the type of the
  list's items.
- The ingot colours the sigil and the words of each block tag, and the `as`
  of an `{#each}`, as keywords. It sends them as semantic tokens, so any
  editor that reads them shows the colours, and the grammar needs no rule.
- The Alloy extension colours `<script>` as Alloy, and the host colours each
  rune as a macro.

## Options

```toml
[ingot.svelte-aly]
import = "@packages/svelte-aly"   # the module path of the package
helper = "__svelte"               # the name the package takes in each file
```

## Lints

| Lint | Level | What it reports |
| --- | --- | --- |
| `shadowed_rune` | deny | A second binding with the name of a rune. |
| `derived_write` | deny | A write to a `$derived` value. |
| `mutation` | warn | A change in place to the table of a state, which runs no effect. |
| `props_outside_script` | deny | `$props()` outside a component file. |
| `bad_each` | deny | An `{#each}` the ingot cannot read. |
| `unclosed_block` | deny | A block with no close, or a close with no block. |
| `unsupported_block` | deny | `{#await}`, `{#key}`, `{#snippet}`, `{@html}`, `{@const}`, and `{@render}`. |
| `style_needs_silk` | warn | A `<style>` in a project without Silk. The ingot drops the element. |

`alloy doc svelte-aly/mutation` explains one.

## Build

```sh
cargo build --release -p svelte-aly
cargo test -p svelte-aly
```
