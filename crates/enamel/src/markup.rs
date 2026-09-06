//! Finds the elements that carry a `ClassName` attribute in `.alx`
//! source. The scanner reads the text, not a tree: it tracks `{ }`
//! holes and strings, which is enough to find a tag's start, the end of
//! its open tag, and its close.

/// One element with a `ClassName` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub tag: String,
    /// The `<` of the open tag.
    pub start: usize,
    /// One past the `>` of the open tag.
    pub open_end: usize,
    /// The span of `/>` when the tag closes itself.
    pub self_close: Option<(usize, usize)>,
    /// One past the end of the element: its `/>` or its `</Tag>`.
    pub end: usize,
    /// The whole `ClassName="..."` attribute.
    pub attr: (usize, usize),
    /// Each class token with its span.
    pub classes: Vec<(String, (usize, usize))>,
    /// Whether the element sits among a parent's children, where an
    /// expression needs `{ }` around it.
    pub in_children: bool,
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}

/// Every element with a `ClassName` string attribute, in source order.
pub fn find(source: &str) -> Vec<Found> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;

    while let Some(i) = source[from..].find("ClassName=") {
        let at = from + i;
        from = at + 1;

        // An attribute: whitespace before, a quote after.
        if at == 0 || !bytes[at - 1].is_ascii_whitespace() {
            continue;
        }

        let q = at + "ClassName=".len();
        let Some(&quote) = bytes.get(q) else {
            continue;
        };

        if quote != b'"' && quote != b'\'' {
            continue;
        }

        let Some(close_rel) = source[q + 1..].find(quote as char) else {
            continue;
        };
        let value_start = q + 1;
        let value_end = q + 1 + close_rel;
        let attr = (at, value_end + 1);
        let Some(start) = tag_start(source, at) else {
            continue;
        };
        let name_end = start
            + 1
            + source[start + 1..]
                .bytes()
                .take_while(|b| is_name_byte(*b))
                .count();
        let tag = source[start + 1..name_end].to_string();
        let Some((open_end, self_close)) = open_tag_end(source, name_end) else {
            continue;
        };
        let end = if self_close.is_some() {
            open_end
        } else {
            match element_end(source, open_end) {
                Some(e) => e,

                None => continue,
            }
        };
        let mut classes = Vec::new();
        let mut k = value_start;

        while k < value_end {
            if bytes[k].is_ascii_whitespace() {
                k += 1;

                continue;
            }

            let s = k;

            while k < value_end && !bytes[k].is_ascii_whitespace() {
                k += 1;
            }

            classes.push((source[s..k].to_string(), (s, k)));
        }

        out.push(Found {
            tag,
            start,
            open_end,
            self_close,
            end,
            attr,
            classes,
            in_children: in_children(source, start),
        });
        from = attr.1;
    }

    out
}

/// The `<` that opens the tag an attribute at `at` belongs to.
fn tag_start(source: &str, at: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = at;

    while i > 0 {
        i -= 1;

        match bytes[i] {
            b'}' => depth += 1,
            b'{' if depth > 0 => depth -= 1,
            b'{' => return None,
            b'"' | b'\'' if depth == 0 => {
                // Skip back over a string attribute value.
                let q = bytes[i];
                let mut j = i;

                while j > 0 {
                    j -= 1;

                    if bytes[j] == q {
                        break;
                    }
                }

                i = j;
            }
            b'<' if depth == 0 => {
                let next = bytes.get(i + 1).copied().unwrap_or(b' ');

                if next.is_ascii_alphabetic() || next == b'_' {
                    return Some(i);
                }

                return None;
            }
            b'>' if depth == 0 => return None,
            _ => {}
        }
    }

    None
}

/// The end of an open tag, from the end of its name: one past `>`, and
/// the span of `/>` when it closes itself.
fn open_tag_end(source: &str, from: usize) -> Option<(usize, Option<(usize, usize)>)> {
    let bytes = source.as_bytes();
    let mut depth = 0i32;
    let mut i = from;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'"' | b'\'' if depth == 0 => {
                let q = bytes[i];
                i += 1;

                while i < bytes.len() && bytes[i] != q {
                    i += 1;
                }
            }
            b'"' | b'\'' | b'`' => {
                // A string inside a hole: skip it, escapes included.
                let q = bytes[i];
                i += 1;

                while i < bytes.len() && bytes[i] != q {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }

                    i += 1;
                }
            }
            b'>' if depth == 0 => {
                let self_close = (i > 0 && bytes[i - 1] == b'/').then(|| (i - 1, i + 1));

                return Some((i + 1, self_close));
            }
            _ => {}
        }

        i += 1;
    }

    None
}

/// Skips a `{ ... }` hole that starts at `i`; returns one past its `}`.
fn skip_hole(bytes: &[u8], mut i: usize) -> usize {
    let mut depth = 0i32;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;

                if depth == 0 {
                    return i + 1;
                }
            }
            b'"' | b'\'' | b'`' => {
                let q = bytes[i];
                i += 1;

                while i < bytes.len() && bytes[i] != q {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }

                    i += 1;
                }
            }
            _ => {}
        }

        i += 1;
    }

    bytes.len()
}

/// One past the close of the element whose open tag ends at `from`.
fn element_end(source: &str, from: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 1i32;
    let mut i = from;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                i = skip_hole(bytes, i);

                continue;
            }
            b'<' => {
                if source[i..].starts_with("<!--") {
                    i = source[i..].find("-->").map_or(bytes.len(), |e| i + e + 3);

                    continue;
                }

                if source[i..].starts_with("</") {
                    let close = source[i..].find('>')? + i;
                    depth -= 1;

                    if depth == 0 {
                        return Some(close + 1);
                    }

                    i = close + 1;

                    continue;
                }

                let next = bytes.get(i + 1).copied().unwrap_or(b' ');

                if next.is_ascii_alphabetic() || next == b'_' || next == b'>' {
                    let name_end = i
                        + 1
                        + source[i + 1..]
                            .bytes()
                            .take_while(|b| is_name_byte(*b))
                            .count();
                    let (end, self_close) = open_tag_end(source, name_end)?;

                    if self_close.is_none() {
                        depth += 1;
                    }

                    i = end;

                    continue;
                }
            }
            _ => {}
        }

        i += 1;
    }

    None
}

/// Whether the element at `start` sits among a parent's children.
fn in_children(source: &str, start: usize) -> bool {
    let before = source[..start].trim_end();
    let Some(last) = before.chars().next_back() else {
        return false;
    };

    if matches!(last, '(' | '=' | ',' | '{' | '[') {
        return false;
    }

    let word_start = before
        .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .map_or(0, |i| i + 1);
    let word = &before[word_start..];

    !matches!(
        word,
        "return" | "then" | "else" | "do" | "and" | "or" | "not"
    )
}

/// The class token of a `ClassName` string that holds an offset.
pub fn class_at(found: &[Found], offset: usize) -> Option<(&Found, usize)> {
    for f in found {
        for (k, (_, span)) in f.classes.iter().enumerate() {
            if span.0 <= offset && offset <= span.1 {
                return Some((f, k));
            }
        }
    }

    None
}

/// The element whose `ClassName` string holds an offset, with the span
/// of the word being typed there.
pub fn string_at(found: &[Found], source: &str, offset: usize) -> Option<(usize, usize)> {
    let bytes = source.as_bytes();

    for f in found {
        let (s, e) = f.attr;
        let open = s + "ClassName=".len() + 1;
        let close = e - 1;

        if offset < open || offset > close {
            continue;
        }

        let mut a = offset;

        while a > open && !bytes[a - 1].is_ascii_whitespace() {
            a -= 1;
        }

        let mut b = offset;

        while b < close && !bytes[b].is_ascii_whitespace() {
            b += 1;
        }

        return Some((a, b));
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_an_element_and_its_bounds() {
        let src = "return (\n    <Frame Size={UDim2.new(1, 0, 0, 24)} ClassName=\"flex gap-2\">\n        <TextLabel ClassName=\"text-white\" />\n    </Frame>\n)\n";
        let found = find(src);
        assert_eq!(found.len(), 2);
        let frame = &found[0];
        assert_eq!(frame.tag, "Frame");
        assert_eq!(&src[frame.start..frame.start + 6], "<Frame");
        assert_eq!(
            frame
                .classes
                .iter()
                .map(|(c, _)| c.as_str())
                .collect::<Vec<_>>(),
            vec!["flex", "gap-2"]
        );
        assert!(frame.self_close.is_none());
        assert_eq!(&src[frame.end - 8..frame.end], "</Frame>");
        assert!(!frame.in_children);
        let label = &found[1];
        assert!(label.self_close.is_some());
        assert!(label.in_children);
        assert_eq!(&src[label.attr.0..label.attr.1], "ClassName=\"text-white\"");
    }

    #[test]
    fn holes_with_angle_brackets_do_not_confuse_the_scan() {
        let src = "local x = <Frame Visible={a < b and c > d} ClassName=\"p-2\">{y}</Frame>\n";
        let found = find(src);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].tag, "Frame");
        assert_eq!(&src[found[0].end - 8..found[0].end], "</Frame>");
    }
}
