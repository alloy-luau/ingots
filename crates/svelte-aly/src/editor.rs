//! The editor side of the ingot: a hover on each block tag, and the
//! block tags as snippets after `{#`, `{:`, and `{/`. The runes need
//! nothing here: the manifest declares them under `[macros]`, so the
//! host lists and explains them as it does the intrinsics.

use alloy_ingot::{CompletionItem, Completions, Hover, ItemKind};

use crate::scan::{self, Tag};

/// What each block tag does, for its hover.
fn tag_doc(tag: &Tag) -> Option<(&'static str, &'static str)> {
    Some(match tag {
        Tag::If => (
            "{#if condition}",
            "Shows the nodes up to the next `{:else}`, `{:else if}`, or `{/if}` while `condition` is truthy. The block builds its nodes again only when the branch changes. Compiles to `if_block`.",
        ),
        Tag::ElseIf => (
            "{:else if condition}",
            "The next branch of an `{#if}`: its nodes show when no branch before it holds and `condition` is truthy.",
        ),
        Tag::Else => (
            "{:else}",
            "In an `{#if}`, the nodes that show when no branch holds. In an `{#each}`, the nodes that show while the list is empty.",
        ),
        Tag::EndIf => ("{/if}", "Closes an `{#if}` block."),
        Tag::Each => (
            "{#each items as item, i (key)}",
            "Builds the nodes up to `{:else}` or `{/each}` once for each item. `i`, the place of the item, is optional. With `(key)`, the nodes of an item stay while its key stays in the list; with no key, they belong to a place. Compiles to `each_block`.",
        ),
        Tag::EndEach => ("{/each}", "Closes an `{#each}` block."),
        Tag::Other(_) => return None,
    })
}

/// The hover on a block tag: on its sigil and its word, not on the
/// expression in its head, which the type check answers.
pub fn hover(src: &str, offset: usize) -> Option<Hover> {
    let s = scan::scan(src, true);

    s.blocks.iter().find_map(|b| {
        let word_end = b.head.map_or(b.end, |r| s.regions[r].start);

        if offset < b.start || offset >= word_end {
            return None;
        }

        let (form, doc) = tag_doc(&b.tag)?;
        let text = format!("```alx\n{form}\n```\n\nA block of the svelte-aly ingot. {doc}");

        Some(Hover::new(text).over((b.start as u32, word_end as u32)))
    })
}

/// The block tags that a sigil opens, as `(label, snippet)`.
fn snippets(sigil: char) -> &'static [(&'static str, &'static str)] {
    match sigil {
        '#' => &[
            ("#if", "{#if ${1:condition}}\n\t$0\n{/if}"),
            (
                "#each",
                "{#each ${1:items} as ${2:item} (${3:key})}\n\t$0\n{/each}",
            ),
        ],
        ':' => &[
            ("else", "{:else}"),
            ("else if", "{:else if ${1:condition}}"),
        ],
        '/' => &[("/if", "{/if}"), ("/each", "{/each}")],
        _ => &[],
    }
}

/// The block tags at an offset where the author types one: a `{`, a
/// sigil, and the start of a word. Anywhere else, nothing, so the host
/// and the type check answer.
pub fn complete(src: &str, offset: usize) -> Completions {
    let before = &src[..offset.min(src.len())];
    let word = before.len()
        - before
            .trim_end_matches(|c: char| c.is_ascii_alphabetic())
            .len();
    let head = &before[..before.len() - word];

    let Some(sigil) = head.chars().last().filter(|c| matches!(c, '#' | ':' | '/')) else {
        return Completions::default();
    };
    let Some(brace) = head
        .len()
        .checked_sub(2)
        .filter(|at| head[*at..].starts_with('{'))
    else {
        return Completions::default();
    };

    // An editor that closed the brace leaves `}` after the caret; the
    // snippet writes its own.
    let end = if src[offset..].starts_with('}') {
        offset + 1
    } else {
        offset
    };
    let items = snippets(sigil)
        .iter()
        .map(|(label, insert)| {
            let label = match sigil {
                ':' => format!(":{label}"),

                _ => label.to_string(),
            };

            CompletionItem::new(label)
                .kind(ItemKind::Snippet)
                .detail("svelte-aly block")
                .snippet(*insert)
                .over((brace as u32, end as u32))
        })
        .collect();

    Completions::new(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_block_tag_hovers_and_its_head_does_not() {
        let src = "<Frame>\n{#if open}\n<TextLabel />\n{:else}\n{/if}\n</Frame>";
        let at = src.find("#if").unwrap();
        let hover = hover(src, at).expect("the tag hovers");

        assert!(
            hover.contents.contains("{#if condition}"),
            "{}",
            hover.contents
        );
        assert!(super::hover(src, src.find("open").unwrap()).is_none());
        assert!(super::hover(src, src.find(":else").unwrap()).is_some());
    }

    #[test]
    fn the_sigil_offers_the_block_tags() {
        let src = "<Frame>\n  {#}\n</Frame>";
        let at = src.find("{#").unwrap() + 2;
        let items = complete(src, at).items;
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();

        assert_eq!(labels, ["#if", "#each"]);
        assert_eq!(items[0].span, Some((at as u32 - 2, at as u32 + 1)));
        assert!(complete("local t = {a}", 11).items.is_empty());
        assert!(complete("print(#xs)", 7).items.is_empty());
    }
}
