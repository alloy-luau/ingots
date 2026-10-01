# Alloy ingots

The official extensions of [Alloy](https://github.com/alloy-luau/alloy).
An ingot is an executable beside an `ingot.toml`; the compiler and the
language server start it once and talk to it over a framed pipe. Each
crate under `crates/` is one ingot.

| Ingot | What it does |
| --- | --- |
| [`enamel`](crates/enamel) | Utility classes on `.alx` elements: `ClassName="flex gap-2 bg-red-500 rounded-lg"` becomes Roblox properties and layout children. Tailwind's names, Roblox's values. |
| [`silk`](crates/silk) | HTML and CSS in `.alx`, the React way: `<div>`, `<p>`, and `<button>` become Roblox instances, attributes become properties, and `<style>` becomes a StyleSheet. |

## Install

Name an ingot by this repository, then install it:

```toml
[ingots]
enamel = { repo = "alloy-luau/ingots" }
silk = { repo = "alloy-luau/ingots" }
```

```sh
alloy ingot install
```

The command fetches the zip for your machine from the latest release
into `.alloy/ingots`, and `.alloy/ingots.lock` records the version.
`version = "0.1.0"` in the table pins one release. `alloy ingot update`
moves an unpinned ingot to the latest release.

A `.config.aly` project writes the same table in Alloy:

```alloy
ingots = {
  enamel = { repo = 'alloy-luau/ingots' },
  silk = { repo = 'alloy-luau/ingots' },
},
```

## Build

The guest crate, `alloy-ingot`, comes from the compiler repository
checked out beside this one as `../crates`, at `crates/alloy-ingot`.

```sh
cargo build --release
```

A project names a local build by the directory that holds its
`ingot.toml`:

```toml
[ingots]
enamel = "../ingots/crates/enamel"
silk = "../ingots/crates/silk"
```

## Release

A tag such as `v0.1.0` builds every ingot for five platforms and puts
`<ingot>-ingot-<platform>.zip` on one GitHub release. The tag must match
the version of each crate.

`alloy ingot info enamel` prints what it declares, and `alloy doc
ingots` explains the protocol.
