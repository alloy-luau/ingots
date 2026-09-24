//! Prints what Silk writes for a `.alx` file, and its findings:
//! `cargo run -p silk --example transform -- page.alx`.

use silk::emit;

fn main() {
    let path = std::env::args().nth(1).expect("a .alx file");
    let source = std::fs::read_to_string(&path).expect("a readable file");
    let out = emit::run(&source, &path, &emit::Options::default());
    let mut text = String::new();
    let mut cursor = 0usize;

    for e in &out.edits {
        text.push_str(&source[cursor..e.0 as usize]);
        text.push_str(&e.2);
        cursor = e.1 as usize;
    }

    text.push_str(&source[cursor..]);
    print!("{text}");

    for f in &out.findings {
        eprintln!("{}..{} {}: {}", f.span.0, f.span.1, f.lint, f.message);
    }
}
