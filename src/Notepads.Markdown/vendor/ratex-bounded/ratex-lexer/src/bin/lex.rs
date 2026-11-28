//! Binary: read LaTeX from stdin, print one token per line (text only).
//! Used by tools/lexer_compare to diff against KaTeX lexer output.

use ratex_lexer::Lexer;
use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let mut input = String::new();
    for line in stdin.lock().lines().map_while(Result::ok) {
        input.push_str(&line);
        input.push('\n');
    }
    // Trim trailing newline so "echo -n 'x^2'" and "echo 'x^2'" behave similarly
    if input.ends_with('\n') {
        input.pop();
    }

    let mut lexer = Lexer::new(&input);
    let tokens = lexer.lex_all();
    let mut out = io::stdout().lock();
    for t in tokens {
        // Escape newlines in token text so one token = one line
        let line = if t.text == "EOF" {
            "EOF".to_string()
        } else {
            t.text.replace('\n', "\\n")
        };
        writeln!(out, "{}", line).expect("write");
    }
}
