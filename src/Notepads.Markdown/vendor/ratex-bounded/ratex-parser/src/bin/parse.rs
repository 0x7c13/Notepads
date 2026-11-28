use ratex_parser::parser::parse;
use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();

    for line in stdin.lock().lines().map_while(Result::ok) {
        let expr = line.trim().to_string();
        if expr.is_empty() {
            continue;
        }

        match parse(&expr) {
            Ok(ast) => {
                let json = serde_json::to_string(&ast).unwrap_or_else(|e| {
                    format!(r#"{{"error":true,"message":"{}","input":"{}"}}"#, e, expr)
                });
                writeln!(out, "{}", json).expect("write");
            }
            Err(e) => {
                let err = serde_json::json!({
                    "error": true,
                    "message": e.to_string(),
                    "input": expr,
                });
                writeln!(out, "{}", err).expect("write");
            }
        }
    }
}
