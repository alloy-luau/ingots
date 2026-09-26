//! Silk: HTML and CSS in Alloy markup, written the way React and TSX
//! write them. `<div>`, `<p>`, `<button>`, and the other HTML elements
//! become the Roblox instances behind them, their attributes become
//! properties, and a `<style>` element becomes a StyleSheet of
//! StyleRules. The editor completes and explains each tag, attribute, and
//! CSS property.

pub mod css;
pub mod editor;
pub mod emit;
pub mod enamel;
pub mod html;
pub mod markup;
pub mod props;
pub mod roblox;
