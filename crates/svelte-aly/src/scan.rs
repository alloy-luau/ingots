//! A reader for Alloy code and `.alx` markup. It splits a file into
//! regions of code tokens: the code of the file or of a `<script>`, and
//! each `{ }` hole of the markup. It also records the block tags of the
//! markup, `{#if}` to `{/each}`, and each `<style>` element.
//!
//! The reader does not validate. Text it cannot read as markup stays
//! code, and the markup compiler reports the problem.

/// The kind of a code token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Name,
    Num,
    Str,
    Punct,
    /// `$state`, `$derived.by`: a `$` name, with its `.name` parts.
    Rune,
    /// A whole markup element inside code.
    Markup,
    /// The `{` or `}` of a hole in a backtick string.
    Interp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tok {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

/// Where a region of code stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// The code of a file, or the body of a `<script>`.
    Code,
    /// `{expr}` as the value of an attribute of the tag with this index.
    Attr(usize),
    /// `{expr}` among the children of an element.
    Child,
    /// `{expr}` in attribute position: a spread.
    Spread,
    /// The expression of a block tag: `{#if expr}`, `{#each expr as x}`.
    Head,
}

#[derive(Debug, Clone)]
pub struct Region {
    pub place: Place,
    /// The first byte of the code.
    pub start: usize,
    /// One past the last byte: the `}` of a hole.
    pub end: usize,
    pub toks: Vec<Tok>,
}

/// The kind of a block tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tag {
    If,
    ElseIf,
    Else,
    EndIf,
    Each,
    EndEach,
    /// A block tag that svelte-aly does not support, such as `{#await}`.
    Other(String),
}

#[derive(Debug, Clone)]
pub struct Block {
    pub tag: Tag,
    /// The `{`.
    pub start: usize,
    /// One past the `}`.
    pub end: usize,
    /// The region of the expression, for `If`, `ElseIf`, and `Each`.
    pub head: Option<usize>,
}

/// An element: its name as written, and the span of the name.
#[derive(Debug, Clone)]
pub struct Element {
    pub name: String,
    pub span: (usize, usize),
}

#[derive(Debug, Default)]
pub struct Scan {
    pub regions: Vec<Region>,
    pub blocks: Vec<Block>,
    pub elements: Vec<Element>,
    /// Each `<style>` element, from its `<` to one past its `>`.
    pub styles: Vec<(usize, usize)>,
    /// The spans of `<script>` and `</script>`, when the file starts with
    /// a script. Region 0 is then the body of the script.
    pub script: Option<((usize, usize), (usize, usize))>,
}

/// The words after which an expression starts, so a `<` opens markup.
const OPENERS: &[&str] = &[
    "return", "and", "or", "not", "then", "else", "elseif", "do", "in", "until", "if", "while",
    "with", "case", "default", "yield", "await",
];

/// The punctuation of more than one byte, longest first.
const PUNCT: &[&str] = &[
    "//=", "..=", "??=", "...", "::", "==", "~=", "<=", ">=", "->", "=>", "+=", "-=", "*=", "/=",
    "%=", "^=", "//", "..", "??", "?.", "?:", "?[",
];

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_tag_name(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-')
}

fn is_attr_name(b: u8) -> bool {
    is_tag_name(b) || b == b'@'
}

/// Reads a file. `markup` is true for `.alx`, where a `<` can open a tag.
pub fn scan(src: &str, markup: bool) -> Scan {
    let mut r = Reader {
        src,
        bytes: src.as_bytes(),
        markup,
        out: Scan::default(),
    };

    let lead = r.skip_trivia(0);

    if markup && src[lead..].starts_with("<script>") {
        let open = (lead, lead + "<script>".len());
        let close_at = src[open.1..].find("</script>").map(|n| open.1 + n);
        let body_end = close_at.unwrap_or(src.len());

        r.region(Place::Code, open.1, |r, k| {
            r.code(open.1, Some(body_end), k)
        });

        if let Some(at) = close_at {
            let close = (at, at + "</script>".len());
            r.out.script = Some((open, close));
            r.children(close.1, None);
        }
    } else {
        r.region(Place::Code, 0, |r, k| r.code(0, Some(src.len()), k));
    }

    r.out
}

struct Reader<'a> {
    src: &'a str,
    bytes: &'a [u8],
    markup: bool,
    out: Scan,
}

impl Reader<'_> {
    fn at(&self, i: usize) -> u8 {
        self.bytes.get(i).copied().unwrap_or(0)
    }

    /// Opens a region, runs `body` to fill it, and closes it at the
    /// offset that `body` returns.
    fn region(
        &mut self,
        place: Place,
        start: usize,
        body: impl FnOnce(&mut Self, usize) -> usize,
    ) -> (usize, usize) {
        let k = self.out.regions.len();
        self.out.regions.push(Region {
            place,
            start,
            end: start,
            toks: Vec::new(),
        });

        let end = body(self, k);
        self.out.regions[k].end = end;

        (k, end)
    }

    fn push(&mut self, region: usize, kind: Kind, start: usize, end: usize) {
        self.out.regions[region].toks.push(Tok { kind, start, end });
    }

    /// Skips whitespace and comments.
    fn skip_trivia(&self, mut i: usize) -> usize {
        loop {
            while i < self.bytes.len() && self.bytes[i].is_ascii_whitespace() {
                i += 1;
            }

            if self.src[i..].starts_with("--") {
                i = self.skip_comment(i);
            } else {
                return i;
            }
        }
    }

    /// Skips a `--` comment, a line or a long one.
    fn skip_comment(&self, i: usize) -> usize {
        if let Some(end) = self.long_end(i + 2) {
            return end;
        }

        self.src[i..].find('\n').map_or(self.src.len(), |n| i + n)
    }

    /// When a long bracket `[[` or `[==[` opens at `i`, one past its close.
    fn long_end(&self, i: usize) -> Option<usize> {
        if self.at(i) != b'[' {
            return None;
        }

        let level = self.bytes[i + 1..]
            .iter()
            .take_while(|b| **b == b'=')
            .count();

        if self.at(i + 1 + level) != b'[' {
            return None;
        }

        let close = format!("]{}]", "=".repeat(level));
        let body = i + level + 2;

        Some(
            self.src[body..]
                .find(&close)
                .map_or(self.src.len(), |n| body + n + close.len()),
        )
    }

    /// One past the close of the quoted string that opens at `i`.
    fn skip_quoted(&self, i: usize) -> usize {
        let quote = self.bytes[i];
        let mut j = i + 1;

        while j < self.bytes.len() {
            match self.bytes[j] {
                b'\\' => j += 2,
                b'\n' => return j,
                b if b == quote => return j + 1,
                _ => j += 1,
            }
        }

        self.bytes.len()
    }

    /// Reads code from `i`. With `stop`, up to that offset; without, up
    /// to the `}` that closes the hole, whose offset it returns.
    fn code(&mut self, mut i: usize, stop: Option<usize>, region: usize) -> usize {
        let end = stop.unwrap_or(self.bytes.len());
        let mut depth = 0usize;
        // Whether the token before is a value, after which `<` compares.
        let mut value = false;

        while i < end {
            let b = self.bytes[i];

            match b {
                _ if b.is_ascii_whitespace() => i += 1,

                b'-' if self.at(i + 1) == b'-' => i = self.skip_comment(i),

                b'"' | b'\'' => {
                    let j = self.skip_quoted(i);
                    self.push(region, Kind::Str, i, j);
                    i = j;
                    value = true;
                }

                b'`' => {
                    let j = self.backtick(i, region);
                    self.push(region, Kind::Str, i, j);
                    i = j;
                    value = true;
                }

                b'[' if self.long_end(i).is_some() => {
                    let j = self.long_end(i).unwrap_or(end);
                    self.push(region, Kind::Str, i, j);
                    i = j;
                    value = true;
                }

                _ if b.is_ascii_alphabetic() || b == b'_' => {
                    let s = i;

                    while i < end && is_word(self.bytes[i]) {
                        i += 1;
                    }

                    self.push(region, Kind::Name, s, i);
                    value = !OPENERS.contains(&&self.src[s..i]);
                }

                _ if b.is_ascii_digit() => {
                    let s = i;

                    while i < end && (is_word(self.bytes[i]) || self.bytes[i] == b'.') {
                        i += 1;
                    }

                    self.push(region, Kind::Num, s, i);
                    value = true;
                }

                b'$' if self.at(i + 1).is_ascii_alphabetic() => {
                    let s = i;
                    i += 1;

                    while i < end
                        && (is_word(self.bytes[i])
                            || (self.bytes[i] == b'.' && self.at(i + 1).is_ascii_alphabetic()))
                    {
                        i += 1;
                    }

                    self.push(region, Kind::Rune, s, i);
                    value = true;
                }

                b'<' if self.markup && !value && self.opens_tag(i) => {
                    let mark = (self.out.regions.len(), self.out.blocks.len());
                    let elements = self.out.elements.len();
                    let styles = self.out.styles.len();

                    match self.element(i) {
                        Some(j) => {
                            self.push(region, Kind::Markup, i, j);
                            i = j;
                            value = true;
                        }

                        None => {
                            self.out.regions.truncate(mark.0);
                            self.out.blocks.truncate(mark.1);
                            self.out.elements.truncate(elements);
                            self.out.styles.truncate(styles);
                            self.push(region, Kind::Punct, i, i + 1);
                            i += 1;
                            value = false;
                        }
                    }
                }

                b'{' | b'(' | b'[' => {
                    if b == b'{' {
                        depth += 1;
                    }

                    self.push(region, Kind::Punct, i, i + 1);
                    i += 1;
                    value = false;
                }

                b'}' if depth == 0 && stop.is_none() => return i,

                b'}' | b')' | b']' => {
                    if b == b'}' {
                        depth = depth.saturating_sub(1);
                    }

                    self.push(region, Kind::Punct, i, i + 1);
                    i += 1;
                    value = true;
                }

                _ => {
                    let len = PUNCT
                        .iter()
                        .find(|p| self.src[i..].starts_with(**p))
                        .map_or_else(|| self.char_len(i), |p| p.len());

                    self.push(region, Kind::Punct, i, i + len);
                    i += len;
                    value = false;
                }
            }
        }

        end
    }

    fn char_len(&self, i: usize) -> usize {
        self.src[i..].chars().next().map_or(1, char::len_utf8)
    }

    /// Reads a backtick string that opens at `i`, with the code of each
    /// `{ }` hole, and returns one past its close.
    fn backtick(&mut self, i: usize, region: usize) -> usize {
        let mut j = i + 1;

        while j < self.bytes.len() {
            match self.bytes[j] {
                b'\\' => j += 2,
                b'`' => return j + 1,

                b'{' => {
                    self.push(region, Kind::Interp, j, j + 1);
                    let close = self.code(j + 1, None, region);

                    if close >= self.bytes.len() {
                        return self.bytes.len();
                    }

                    self.push(region, Kind::Interp, close, close + 1);
                    j = close + 1;
                }

                _ => j += 1,
            }
        }

        self.bytes.len()
    }

    /// Whether the `<` at `i` starts a tag: a letter, `_`, or `>` follows.
    fn opens_tag(&self, i: usize) -> bool {
        let b = self.at(i + 1);
        b.is_ascii_alphabetic() || b == b'_' || b == b'>'
    }

    fn skip_space(&self, mut i: usize) -> usize {
        while i < self.bytes.len() && self.bytes[i].is_ascii_whitespace() {
            i += 1;
        }

        i
    }

    /// Reads the element whose `<` is at `at`, and returns one past its
    /// end. `None` when the text is not an element.
    fn element(&mut self, at: usize) -> Option<usize> {
        let mut i = at + 1;
        let name_start = i;

        while i < self.bytes.len() && is_tag_name(self.bytes[i]) {
            i += 1;
        }

        let name = self.src[name_start..i].to_string();
        let tag = self.out.elements.len();
        self.out.elements.push(Element {
            name: name.clone(),
            span: (name_start, i),
        });

        loop {
            i = self.skip_space(i);

            match self.at(i) {
                0 => return None,

                b'/' if self.at(i + 1) == b'>' => return Some(i + 2),

                b'>' if name == "style" => {
                    let close = format!("</{name}>");
                    let end = self.src[i..].find(&close).map(|n| i + n + close.len())?;
                    self.out.styles.push((at, end));

                    return Some(end);
                }

                b'>' => return Some(self.children(i + 1, Some(&name))),

                b'{' => {
                    let (_, close) =
                        self.region(Place::Spread, i + 1, |r, k| r.code(i + 1, None, k));
                    i = close + 1;
                }

                b if is_attr_name(b) => {
                    while i < self.bytes.len() && is_attr_name(self.bytes[i]) {
                        i += 1;
                    }

                    let after = self.skip_space(i);

                    if self.at(after) != b'=' {
                        continue;
                    }

                    i = self.skip_space(after + 1);

                    match self.at(i) {
                        b'"' | b'\'' => i = self.skip_quoted(i),

                        b'{' => {
                            let (_, close) =
                                self.region(Place::Attr(tag), i + 1, |r, k| r.code(i + 1, None, k));
                            i = close + 1;
                        }

                        _ => return None,
                    }
                }

                _ => return None,
            }
        }
    }

    /// Reads children from `i` up to the close tag of `name`, and returns
    /// one past it. With no name, reads to the end of the file: the markup
    /// after a `<script>`.
    fn children(&mut self, mut i: usize, name: Option<&str>) -> usize {
        while i < self.bytes.len() {
            let rest = &self.src[i..];

            if rest.starts_with("</") && name.is_some() {
                return rest.find('>').map_or(self.src.len(), |n| i + n + 1);
            } else if rest.starts_with("<!--") {
                i = rest.find("-->").map_or(self.src.len(), |n| i + n + 3);
            } else if rest.starts_with('<') && self.opens_tag(i) {
                i = match self.element(i) {
                    Some(end) => end,

                    None => i + 1,
                };
            } else if rest.starts_with('{') {
                i = self.brace(i);
            } else {
                i += self.char_len(i);
            }
        }

        self.src.len()
    }

    /// Reads a `{` among children: a block tag or a hole. Returns one past
    /// its `}`.
    fn brace(&mut self, at: usize) -> usize {
        let rest = &self.src[at..];
        let sigil = rest.as_bytes().get(1).copied();

        if !matches!(sigil, Some(b'#' | b':' | b'/' | b'@')) {
            let (_, close) = self.region(Place::Child, at + 1, |r, k| r.code(at + 1, None, k));
            return close + 1;
        }

        let word_end = at + 2 + rest[2..].bytes().take_while(|b| is_word(*b)).count();
        let word = &self.src[at + 2..word_end];
        let mut head_at = word_end;

        let tag = match (sigil, word) {
            (Some(b'#'), "if") => Tag::If,
            (Some(b'#'), "each") => Tag::Each,
            (Some(b'/'), "if") => Tag::EndIf,
            (Some(b'/'), "each") => Tag::EndEach,

            (Some(b':'), "else") => {
                let after = self.skip_space(word_end);

                if self.src[after..].starts_with("if") && !is_word(self.at(after + 2)) {
                    head_at = after + 2;
                    Tag::ElseIf
                } else {
                    Tag::Else
                }
            }

            _ => Tag::Other(self.src[at + 1..word_end].to_string()),
        };

        let (head, close) = match tag {
            Tag::If | Tag::ElseIf | Tag::Each => {
                let (k, close) = self.region(Place::Head, head_at, |r, k| r.code(head_at, None, k));
                (Some(k), close)
            }

            _ => (
                None,
                self.src[at..].find('}').map_or(self.src.len(), |n| at + n),
            ),
        };

        let end = (close + 1).min(self.src.len());
        self.out.blocks.push(Block {
            tag,
            start: at,
            end,
            head,
        });

        end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts<'a>(src: &'a str, region: &Region) -> Vec<&'a str> {
        region.toks.iter().map(|t| &src[t.start..t.end]).collect()
    }

    #[test]
    fn reads_a_script_and_the_markup_after_it() {
        let src = "<script>\n  let n = $state(0) -- '\n</script>\n<TextLabel Text={n}>That's {n}</TextLabel>\n{#if n > 1}\n<Frame />\n{/if}\n";
        let s = scan(src, true);

        assert!(s.script.is_some());
        assert_eq!(
            texts(src, &s.regions[0]),
            ["let", "n", "=", "$state", "(", "0", ")"]
        );
        assert_eq!(s.regions[1].place, Place::Attr(0));
        assert_eq!(s.regions[2].place, Place::Child);
        assert_eq!(texts(src, &s.regions[3]), ["n", ">", "1"]);
        assert_eq!(
            s.blocks.iter().map(|b| b.tag.clone()).collect::<Vec<_>>(),
            [Tag::If, Tag::EndIf]
        );
    }

    #[test]
    fn reads_markup_in_code_and_holes_in_strings() {
        let src = "local e = <Frame>{`a {b}`}</Frame>\nlocal x = a < b";
        let s = scan(src, true);

        assert_eq!(
            texts(src, &s.regions[0]),
            [
                "local",
                "e",
                "=",
                "<Frame>{`a {b}`}</Frame>",
                "local",
                "x",
                "=",
                "a",
                "<",
                "b"
            ]
        );
        assert_eq!(texts(src, &s.regions[1]), ["{", "b", "}", "`a {b}`"]);
    }

    #[test]
    fn survives_text_that_is_not_closed() {
        for src in [
            "<script>\nlet a = `{",
            "<Frame>{#if",
            "{/if}}}",
            "local t = { a = <",
            "<script>",
        ] {
            let _ = scan(src, true);
        }
    }
}
