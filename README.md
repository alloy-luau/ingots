# Alloy ingots

The official extensions of [Alloy](https://github.com/alloy-luau/alloy).
An ingot is an executable beside an `ingot.toml`; the compiler and the
language server start it once and talk to it over a framed pipe. Each
crate under `crates/` is one ingot.

| Ingot | What it does |
| --- | --- |
| [`enamel`](crates/enamel) | Utility classes on `.alx` elements: `ClassName="flex gap-2 bg-red-500 rounded-lg"` becomes Roblox properties and layout children. Tailwind's names, Roblox's values. |

## Build

The guest crate, `alloy-ingot`, comes from the compiler repository
checked out beside this one as `../crates`.

```sh
cargo build --release
```

A project names an ingot by the directory that holds its `ingot.toml`:

```toml
[ingots]
enamel = "../ingots/crates/enamel"
```

`alloy ingot info enamel` prints what it declares, and `alloy doc
ingots` explains the protocol.
