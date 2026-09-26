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
    /// Whether the tag is an HTML element that Silk writes, with the
    /// classes in `className` or `class`.
    pub html: bool,
    /// The expression of a `Size={...}` attribute on the open tag.
    pub size_attr: Option<(usize, usize)>,
    /// The `{ }` hole that is the whole body, with no `Text` attribute
    /// on the tag. The markup compiler reads it as the Text of a text
    /// element.
    pub text_hole: Option<(usize, usize)>,
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.'
}

/// Every element with a `ClassName` string attribute, in source order:
/// what the transform rewrites.
pub fn find(source: &str) -> Vec<Found> {
    find_named(source, "ClassName=")
}

/// Every element whose classes the editor reads: `ClassName`, and the
/// `className` or `class` of an HTML element that Silk turns into a
/// Roblox one, in source order.
pub fn find_all(source: &str) -> Vec<Found> {
    let mut out = find_named(source, "ClassName=");

    for name in ["className=", "class="] {
        out.extend(find_named(source, name).into_iter().filter(|f| f.html));
    }

    out.sort_by_key(|f| f.start);

    out
}

fn find_named(source: &str, attr_name: &str) -> Vec<Found> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;

    while let Some(i) = source[from..].find(attr_name) {
        let at = from + i;
        from = at + 1;

        // An attribute: whitespace before, a quote after.
        if at == 0 || !bytes[at - 1].is_ascii_whitespace() {
            continue;
        }

        let q = at + attr_name.len();
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

        let html = tag.starts_with(|c: char| c.is_ascii_lowercase());
        let size_attr = attr_value(source, name_end, open_end, "Size")
            .filter(|v| v.2)
            .map(|(s, e, _)| (s, e));
        let text_hole = self_close
            .is_none()
            .then(|| lone_hole(source, open_end, end))
            .flatten()
            .filter(|_| attr_value(source, name_end, open_end, "Text").is_none());
        out.push(Found {
            tag,
            start,
            open_end,
            self_close,
            end,
            attr,
            classes,
            in_children: in_children(source, start),
            html,
            size_attr,
            text_hole,
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
        if depth > 0
            && let Some(end) = comment_end(source, i)
        {
            i = end;

            continue;
        }

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

/// The value of a `name=` attribute of the open tag between `from` and
/// `to`: the span inside its braces or quotes, and whether it is a hole.
fn attr_value(source: &str, from: usize, to: usize, name: &str) -> Option<(usize, usize, bool)> {
    let bytes = source.as_bytes();
    let key = format!("{name}=");
    let named = |i: usize| {
        i > key.len()
            && source.get(i - key.len()..i) == Some(key.as_str())
            && bytes[i - key.len() - 1].is_ascii_whitespace()
    };
    let mut i = from;

    while i < to {
        match bytes[i] {
            b'{' => {
                let end = skip_hole(source, i);

                if named(i) {
                    return Some((i + 1, end - 1, true));
                }

                i = end;

                continue;
            }
            b'"' | b'\'' => {
                let (open, q) = (i, bytes[i]);
                i += 1;

                while i < to && bytes[i] != q {
                    i += 1;
                }

                if named(open) {
                    return Some((open + 1, i, false));
                }
            }
            _ => {}
        }

        i += 1;
    }

    None
}

/// The `{ }` hole that is the whole body of an element, between the end
/// of its open tag and its close tag.
fn lone_hole(source: &str, open_end: usize, end: usize) -> Option<(usize, usize)> {
    let close = source[..end].rfind("</")?;
    let body = &source[open_end..close];
    let start = open_end + body.len() - body.trim_start().len();
    let stop = open_end + body.trim_end().len();

    (source[start..].starts_with('{') && skip_hole(source, start) == stop).then_some((start, stop))
}

/// One past the end of the Luau comment that opens at `i`: a `--[[ ]]`
/// or `--[==[ ]==]` block, else the rest of the line. `None` when no
/// comment opens there. Inside a hole a comment is code, and a quote in
/// it, `-- it's here`, opens no string.
pub fn comment_end(source: &str, i: usize) -> Option<usize> {
    let after = source.get(i..)?.strip_prefix("--")?;
    let body = i + 2;
    let level = after
        .strip_prefix('[')
        .map(|r| r.len() - r.trim_start_matches('=').len())
        .filter(|l| after[1 + l..].starts_with('['));

    Some(match level {
        Some(l) => {
            let close = format!("]{}]", "=".repeat(l));

            source[body..]
                .find(&close)
                .map_or(source.len(), |n| body + n + close.len())
        }

        None => source[body..].find('\n').map_or(source.len(), |n| body + n),
    })
}

/// Skips a `{ ... }` hole that starts at `i`; returns one past its `}`.
pub(crate) fn skip_hole(source: &str, mut i: usize) -> usize {
    let bytes = source.as_bytes();
    let mut depth = 0i32;

    while i < bytes.len() {
        if let Some(end) = comment_end(source, i) {
            i = end;

            continue;
        }

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
                i = skip_hole(source, i);

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
    #[test]
    fn the_editor_reads_class_name_on_an_html_element() {
        let src = "local x = <div className=\"flex card\"><Frame ClassName=\"p-2\" /><p class=\"text-sm\">a</p></div>\n";
        let all = super::find_all(src);

        assert_eq!(
            all.iter()
                .map(|f| (f.tag.as_str(), f.html))
                .collect::<Vec<_>>(),
            vec![("div", true), ("Frame", false), ("p", true)]
        );
        assert_eq!(
            super::find(src).len(),
            1,
            "the transform reads ClassName alone"
        );
        assert_eq!(
            crate::classes::Element::parse("div"),
            Some(crate::classes::Element::Frame)
        );
        assert_eq!(
            crate::classes::Element::parse("button"),
            Some(crate::classes::Element::TextButton)
        );
    }

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

    /// A quote in a comment inside a hole opens no string, so the scan
    /// still finds the close of the element around it.
    #[test]
    fn a_quote_in_a_comment_does_not_open_a_string() {
        for comment in [
            "-- it's here\n",
            "--[[ it's here ]]",
            "--[==[ it's\n ]] here ]==]",
        ] {
            let src = format!(
                "return (\n  <Frame ClassName=\"w-full\">\n    <TextButton Activated={{function()\n      {comment}\n    end}} />\n  </Frame>\n)\n"
            );
            let found = find(&src);

            assert_eq!(found.len(), 1, "{src}");
            assert_eq!(&src[found[0].end - 8..found[0].end], "</Frame>");
        }

        let src = "local x = <Frame Activated={function() -- it's\nend} ClassName=\"p-2\">{f() --[[ ' ]]}</Frame>\n";
        let found = find(src);

        assert_eq!(found.len(), 1);
        assert_eq!(&src[found[0].end - 8..found[0].end], "</Frame>");
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
