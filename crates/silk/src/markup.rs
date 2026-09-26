//! A reader for the markup of a `.alx` file. It finds each markup region
//! in the Luau around it, then reads the elements, their attributes, and
//! their children with byte spans. A `{ }` hole is read as Luau again, so
//! a tag inside a hole is an element too.
//!
//! The reader does not validate. A region it cannot read is skipped, and
//! the markup compiler reports the problem.

/// The value of an attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    /// A bare attribute: `hidden`.
    Bare,
    /// `"text"` or `'text'`: the span between the quotes.
    Str(usize, usize),
    /// `{expr}`: the span between the braces.
    Expr(usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attr {
    pub name: String,
    pub name_span: (usize, usize),
    pub value: Value,
    /// The whole attribute.
    pub span: (usize, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Child {
    Element(usize),
    Text(usize, usize),
    /// `{expr}`, the braces included.
    Hole(usize, usize),
    /// `<!-- -->` or a `{ }` that holds only comments.
    Comment(usize, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// The tag as written; empty for a fragment.
    pub name: String,
    pub name_span: (usize, usize),
    /// The `<` of the open tag.
    pub start: usize,
    /// One past the `>` of the open tag.
    pub open_end: usize,
    /// The span of `/>` when the tag closes itself. As in React, a void
    /// tag closes itself too: `<br />`, never `<br>`.
    pub self_close: Option<(usize, usize)>,
    /// The span of `</name>`.
    pub close: Option<(usize, usize)>,
    /// One past the end of the element.
    pub end: usize,
    pub attrs: Vec<Attr>,
    /// `{props}` and `={expr}` in attribute position.
    pub spreads: Vec<(usize, usize)>,
    pub children: Vec<Child>,
    pub parent: Option<usize>,
    /// The element whose `{ }` hole holds this one.
    pub hole_owner: Option<usize>,
    /// Whether the element stands among a parent's children, where an
    /// expression needs `{ }` around it.
    pub in_children: bool,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&Attr> {
        self.attrs.iter().find(|a| a.name == name)
    }

    /// The text of a string attribute, or `None` for another kind.
    pub fn text<'s>(&self, source: &'s str, name: &str) -> Option<&'s str> {
        match self.attr(name)?.value {
            Value::Str(s, e) => Some(&source[s..e]),

            _ => None,
        }
    }

    /// The span of the children: from the end of the open tag to the
    /// close tag.
    pub fn inner(&self) -> Option<(usize, usize)> {
        self.close.map(|(s, _)| (self.open_end, s))
    }

    /// The span of the CSS of a `<style>`: the text between the tags, or
    /// the string in its one hole, the React way: `{[[ ... ]]}` or a
    /// backtick string.
    pub fn css(&self, source: &str) -> Option<(usize, usize)> {
        let (s, t) = self.inner()?;
        let text = &source[s..t];
        let lead = text.len() - text.trim_start().len();
        let trimmed = text.trim();

        let Some(hole) = trimmed.strip_prefix('{').and_then(|r| r.strip_suffix('}')) else {
            return Some((s, t));
        };
        let hole_start = s + lead + 1;
        let inner_lead = hole.len() - hole.trim_start().len();
        let body = hole.trim();
        let at = hole_start + inner_lead;

        if let Some(rest) = body.strip_prefix('[') {
            let level = rest.bytes().take_while(|b| *b == b'=').count();
            let open = level + 2;
            let close = format!("]{}]", "=".repeat(level));

            return body[open..]
                .find(&close)
                .map(|n| (at + open, at + open + n));
        }

        match body.chars().next() {
            Some(q @ ('`' | '"' | '\'')) if body.len() >= 2 && body.ends_with(q) => {
                Some((at + 1, at + body.len() - 1))
            }

            _ => Some((s, t)),
        }
    }
}

/// Every element of a file, in the order the reader met them.
#[derive(Debug, Default)]
pub struct Markup {
    pub elements: Vec<Element>,
    /// The elements that start a markup region.
    pub roots: Vec<usize>,
}

impl Markup {
    pub fn read(source: &str) -> Self {
        let mut markup = Markup::default();
        let mut reader = Reader {
            src: source,
            out: &mut markup,
        };
        reader.scan(0, source.len(), None);

        markup
    }

    /// The innermost element whose span holds `offset`.
    pub fn at(&self, offset: usize) -> Option<usize> {
        self.elements
            .iter()
            .enumerate()
            .filter(|(_, e)| e.start <= offset && offset < e.end.max(e.start + 1))
            .min_by_key(|(_, e)| e.end - e.start)
            .map(|(i, _)| i)
    }

    /// The element children of an element, holes left out.
    pub fn element_children(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        self.elements[i].children.iter().filter_map(|c| match c {
            Child::Element(k) => Some(*k),

            _ => None,
        })
    }
}

struct Reader<'a> {
    src: &'a str,
    out: &'a mut Markup,
}

/// What stood before a `<`: it opens markup only where an expression can
/// start.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Prev {
    Start,
    Operator,
    Value,
}

/// Words after which an expression starts, so a `<` opens markup.
const OPENERS: &[&str] = &[
    "return", "and", "or", "not", "then", "else", "elseif", "do", "in", "until", "if", "while",
    "with", "case", "default", "yield", "await",
];

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_name(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-')
}

fn is_attr_name(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-' | b'@')
}

impl Reader<'_> {
    fn bytes(&self) -> &[u8] {
        self.src.as_bytes()
    }

    /// Reads Luau in `from..to` and each markup region in it.
    fn scan(&mut self, from: usize, to: usize, owner: Option<usize>) {
        let mut i = from;
        let mut prev = Prev::Start;

        while i < to {
            let b = self.bytes()[i];

            match b {
                _ if b.is_ascii_whitespace() => i += 1,

                b'-' if self.bytes().get(i + 1) == Some(&b'-') => i = skip_comment(self.src, i),

                b'"' | b'\'' => {
                    i = skip_quoted(self.src, i);
                    prev = Prev::Value;
                }

                b'`' => {
                    i = skip_backtick(self.src, i);
                    prev = Prev::Value;
                }

                b'[' if long_open(self.src, i).is_some() => {
                    i = skip_long(self.src, i);
                    prev = Prev::Value;
                }

                _ if b.is_ascii_alphabetic() || b == b'_' => {
                    let s = i;

                    while i < to && is_word(self.bytes()[i]) {
                        i += 1;
                    }

                    prev = match OPENERS.contains(&&self.src[s..i]) {
                        true => Prev::Operator,

                        false => Prev::Value,
                    };
                }

                _ if b.is_ascii_digit() => {
                    while i < to && (is_word(self.bytes()[i]) || self.bytes()[i] == b'.') {
                        i += 1;
                    }

                    prev = Prev::Value;
                }

                b'<' if prev != Prev::Value && self.opens_tag(i) => {
                    let mark = self.out.elements.len();

                    match self.element(i, None, owner, false) {
                        Some(k) => {
                            if owner.is_none() {
                                self.out.roots.push(k);
                            }

                            i = self.out.elements[k].end;
                            prev = Prev::Value;
                        }

                        None => {
                            self.out.elements.truncate(mark);
                            i += 1;
                            prev = Prev::Operator;
                        }
                    }
                }

                b')' | b']' | b'}' => {
                    i += 1;
                    prev = Prev::Value;
                }

                b'.' if self.src[i..].starts_with("...") => {
                    i += 3;
                    prev = Prev::Value;
                }

                _ => {
                    i += 1;
                    prev = Prev::Operator;
                }
            }
        }
    }

    /// Whether the `<` at `i` starts a tag: a letter or `>` follows.
    fn opens_tag(&self, i: usize) -> bool {
        self.bytes()
            .get(i + 1)
            .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'>' || *b == b'_')
    }

    fn skip_space(&self, mut i: usize) -> usize {
        while i < self.src.len() && self.bytes()[i].is_ascii_whitespace() {
            i += 1;
        }

        i
    }

    /// Reads the element whose `<` is at `at`, and returns its index.
    fn element(
        &mut self,
        at: usize,
        parent: Option<usize>,
        owner: Option<usize>,
        in_children: bool,
    ) -> Option<usize> {
        let len = self.src.len();
        let mut i = at + 1;
        let name_start = i;

        while i < len && is_name(self.bytes()[i]) {
            i += 1;
        }

        let name = self.src[name_start..i].to_string();
        let index = self.out.elements.len();
        self.out.elements.push(Element {
            name: name.clone(),
            name_span: (name_start, i),
            start: at,
            open_end: 0,
            self_close: None,
            close: None,
            end: 0,
            attrs: Vec::new(),
            spreads: Vec::new(),
            children: Vec::new(),
            parent,
            hole_owner: owner,
            in_children,
        });

        // The attributes, up to `>` or `/>`.
        loop {
            i = self.skip_space(i);

            match self.bytes().get(i)? {
                b'/' if self.bytes().get(i + 1) == Some(&b'>') => {
                    let e = &mut self.out.elements[index];
                    e.self_close = Some((i, i + 2));
                    e.open_end = i + 2;
                    e.end = i + 2;

                    return Some(index);
                }

                b'>' => {
                    i += 1;

                    break;
                }

                b'{' => {
                    let end = skip_hole(self.src, i)?;
                    self.out.elements[index].spreads.push((i, end));
                    i = end;
                }

                b'=' if self.bytes().get(i + 1) == Some(&b'{') => {
                    let end = skip_hole(self.src, i + 1)?;
                    self.out.elements[index].spreads.push((i, end));
                    i = end;
                }

                b if is_attr_name(*b) => {
                    let s = i;

                    while i < len && is_attr_name(self.bytes()[i]) {
                        i += 1;
                    }

                    let name_span = (s, i);
                    let after = self.skip_space(i);
                    let value = match self.bytes().get(after) {
                        Some(b'=') => {
                            let v = self.skip_space(after + 1);

                            match self.bytes().get(v)? {
                                q @ (b'"' | b'\'') => {
                                    let end = skip_string(self.src, v, *q)?;
                                    i = end;

                                    Value::Str(v + 1, end - 1)
                                }

                                b'{' => {
                                    let end = skip_hole(self.src, v)?;
                                    i = end;

                                    Value::Expr(v + 1, end - 1)
                                }

                                _ => return None,
                            }
                        }

                        _ => Value::Bare,
                    };

                    self.out.elements[index].attrs.push(Attr {
                        name: self.src[s..name_span.1].to_string(),
                        name_span,
                        value,
                        span: (s, i),
                    });
                }

                _ => return None,
            }
        }

        self.out.elements[index].open_end = i;

        // A `<style>` holds CSS, which is text up to `</style>` as HTML
        // reads it: `--name` is no Luau comment there, and `{` opens no
        // hole.
        if name == "style" {
            let close = i + self.src[i..].find("</style")?;
            let end = close + self.src[close..].find('>')? + 1;
            let e = &mut self.out.elements[index];
            e.children.push(Child::Text(i, close));
            e.close = Some((close, end));
            e.end = end;

            return Some(index);
        }

        // The children, up to `</`.
        loop {
            match self.bytes().get(i)? {
                b'<' if self.src[i..].starts_with("</") => break,

                b'<' if self.src[i..].starts_with("<!--") => {
                    let end = i + self.src[i..].find("-->")? + 3;
                    self.out.elements[index]
                        .children
                        .push(Child::Comment(i, end));
                    i = end;
                }

                b'<' => {
                    let child = self.element(i, Some(index), owner, true)?;
                    self.out.elements[index]
                        .children
                        .push(Child::Element(child));
                    i = self.out.elements[child].end;
                }

                b'{' => {
                    let end = skip_hole(self.src, i)?;
                    let inner = self.src[i + 1..end - 1].trim();
                    let comment = inner.starts_with("--") && no_code(inner);
                    let child = match comment {
                        true => Child::Comment(i, end),

                        false => Child::Hole(i, end),
                    };
                    self.out.elements[index].children.push(child);

                    if !comment {
                        self.scan(i + 1, end - 1, Some(index));
                    }

                    i = end;
                }

                _ => {
                    let s = i;

                    while i < len {
                        match self.bytes()[i] {
                            b'<' | b'{' => break,

                            b'\\' => i = (i + 2).min(len),

                            _ => i += 1,
                        }
                    }

                    self.out.elements[index].children.push(Child::Text(s, i));
                }
            }
        }

        // `</name>` or `</>`.
        let close_start = i;
        i += 2;
        let n = i;

        while i < len && is_name(self.bytes()[i]) {
            i += 1;
        }

        if self.src[n..i] != name {
            return None;
        }

        i = self.skip_space(i);

        if self.bytes().get(i) != Some(&b'>') {
            return None;
        }

        let e = &mut self.out.elements[index];
        e.close = Some((close_start, i + 1));
        e.end = i + 1;

        Some(index)
    }
}

/// Whether a hole's text is comments alone.
fn no_code(inner: &str) -> bool {
    let mut i = 0;
    let b = inner.as_bytes();

    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;

            continue;
        }

        if inner[i..].starts_with("--") {
            i = skip_comment(inner, i);

            continue;
        }

        return false;
    }

    true
}

/// Whether `at` sits in code: in no comment and no string. It reads the
/// file as Luau, so a quote in markup text hides the rest of its line.
pub fn in_code(src: &str, at: usize) -> bool {
    let b = src.as_bytes();
    let mut i = 0;

    while i < at {
        let end = match b[i] {
            b'-' if b.get(i + 1) == Some(&b'-') => skip_comment(src, i),

            b'"' | b'\'' => skip_quoted(src, i),

            b'`' => skip_backtick(src, i),

            b'[' if long_open(src, i).is_some() => skip_long(src, i),

            _ => i + 1,
        };

        if end > at {
            return false;
        }

        i = end;
    }

    true
}

/// The spans of the long comments of a file, `--[[ ]]` and `--[==[ ]==]`.
/// It reads no strings, so markup text with a quote in it does not hide
/// the comments after it.
pub fn block_comments(src: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut from = 0;

    while let Some(n) = src[from..].find("--[") {
        let at = from + n;

        from = match long_open(src, at + 2) {
            Some(_) => {
                let end = skip_long(src, at + 2);
                out.push((at, end));

                end
            }

            None => at + 3,
        };
    }

    out
}

/// The end of a `--` comment at `i`, line or long.
pub fn skip_comment(src: &str, i: usize) -> usize {
    let after = i + 2;

    if long_open(src, after).is_some() {
        return skip_long(src, after);
    }

    src[after..].find('\n').map_or(src.len(), |n| after + n)
}

/// The level of a long bracket `[[` or `[==[` at `i`.
fn long_open(src: &str, i: usize) -> Option<usize> {
    let b = src.as_bytes();

    if b.get(i) != Some(&b'[') {
        return None;
    }

    let mut j = i + 1;

    while b.get(j) == Some(&b'=') {
        j += 1;
    }

    (b.get(j) == Some(&b'[')).then_some(j - i - 1)
}

fn skip_long(src: &str, i: usize) -> usize {
    let Some(level) = long_open(src, i) else {
        return i + 1;
    };
    let close = format!("]{}]", "=".repeat(level));
    let from = i + level + 2;

    src[from..]
        .find(&close)
        .map_or(src.len(), |n| from + n + close.len())
}

/// The end of a quoted Luau string at `i`, or the end of the line when
/// it does not close.
fn skip_quoted(src: &str, i: usize) -> usize {
    skip_string(src, i, src.as_bytes()[i])
        .unwrap_or_else(|| src[i..].find('\n').map_or(src.len(), |n| i + n))
}

/// The end of a string that opens with `q` at `i`, past the close.
fn skip_string(src: &str, i: usize, q: u8) -> Option<usize> {
    let b = src.as_bytes();
    let mut j = i + 1;

    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,

            c if c == q => return Some(j + 1),

            b'\n' if q != b'`' => return None,

            _ => j += 1,
        }
    }

    None
}

fn skip_backtick(src: &str, i: usize) -> usize {
    let b = src.as_bytes();
    let mut j = i + 1;

    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,

            b'`' => return j + 1,

            b'{' => j = skip_hole(src, j).unwrap_or(b.len()),

            _ => j += 1,
        }
    }

    b.len()
}

/// The end of a `{ }` at `i`, past the `}`. Strings and comments inside
/// do not count their braces.
pub fn skip_hole(src: &str, i: usize) -> Option<usize> {
    let b = src.as_bytes();
    let mut depth = 0usize;
    let mut j = i;

    while j < b.len() {
        match b[j] {
            b'{' => {
                depth += 1;
                j += 1;
            }

            b'}' => {
                depth -= 1;
                j += 1;

                if depth == 0 {
                    return Some(j);
                }
            }

            b'"' | b'\'' => j = skip_quoted(src, j),

            b'`' => j = skip_backtick(src, j),

            b'[' if long_open(src, j).is_some() => j = skip_long(src, j),

            b'-' if b.get(j + 1) == Some(&b'-') => j = skip_comment(src, j),

            _ => j += 1,
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_elements_attributes_and_children() {
        let src =
            "return <div id=\"a\" hidden onClick={go}>Hi {name}<br /><img src='x.png' /></div>\n";
        let m = Markup::read(src);
        let div = &m.elements[m.roots[0]];

        assert_eq!(div.name, "div");
        assert_eq!(div.text(src, "id"), Some("a"));
        assert_eq!(div.attr("hidden").unwrap().value, Value::Bare);
        assert!(matches!(
            div.attr("onClick").unwrap().value,
            Value::Expr(..)
        ));
        assert_eq!(div.children.len(), 4);
        assert!(matches!(div.children[1], Child::Hole(..)));

        let br = &m.elements[m.element_children(m.roots[0]).next().unwrap()];
        assert_eq!(br.name, "br");
        assert!(br.self_close.is_some());
        assert_eq!(&src[div.close.unwrap().0..div.close.unwrap().1], "</div>");
    }

    /// React has no unclosed void tag, so `<br>` reads as an open tag
    /// with no close, and the region is not markup Silk reads.
    #[test]
    fn a_void_tag_must_close_itself() {
        assert!(Markup::read("return <p>a<br>b</p>").elements.is_empty());
    }

    #[test]
    fn a_style_holds_css_as_text() {
        let src = "return <div><style>\n:root { --a: #fff; }\n.b { color: var(--a) }\n</style><p>x</p></div>";
        let m = Markup::read(src);
        let style = m.elements.iter().find(|e| e.name == "style").unwrap();
        let (s, t) = style.css(src).unwrap();

        assert!(src[s..t].contains(".b { color"));
        assert!(m.elements.iter().any(|e| e.name == "p"));

        let react = "return <style>{[[\n.b { color: red }\n]]}</style>";
        let m = Markup::read(react);
        let (s, t) = m.elements[0].css(react).unwrap();
        assert_eq!(&react[s..t], "\n.b { color: red }\n");
    }

    #[test]
    fn a_tag_inside_a_hole_is_an_element() {
        let src = "local x = <ul>{items:map(function(i) return <li>{i}</li> end)}</ul>";
        let m = Markup::read(src);
        let li = m.elements.iter().find(|e| e.name == "li").unwrap();

        assert_eq!(li.hole_owner, Some(m.roots[0]));
        assert!(!li.in_children);
    }

    #[test]
    fn a_comparison_is_not_markup() {
        let m = Markup::read("if a < b and c <d then end\nlocal t: Map<string, number> = {}\n");

        assert!(m.elements.is_empty());
    }

    #[test]
    fn a_fragment_and_a_comment_read() {
        let src = "return <>\n  <!-- note -->\n  <p>x</p>\n</>";
        let m = Markup::read(src);
        let f = &m.elements[m.roots[0]];

        assert_eq!(f.name, "");
        assert!(f.children.iter().any(|c| matches!(c, Child::Comment(..))));
        assert_eq!(m.at(src.find("x<").unwrap()), Some(1));
    }
}
