//! svelte-aly: Svelte's runes and blocks for Alloy. The ingot compiles
//! `$state`, `$derived`, `$effect`, `$props`, a `<script>` file, and the
//! `{#if}` and `{#each}` blocks of `.alx` markup into calls of the
//! svelte-aly package.

pub mod editor;
pub mod rewrite;
pub mod scan;
