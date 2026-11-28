// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

use std::{env, fs, path::Path};

use syntect::{
    dumps,
    parsing::{SyntaxDefinition, SyntaxSet},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let assets = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/markdown-core/assets/code-syntaxes");
    let output = env::args_os()
        .nth(1)
        .map_or_else(|| assets.join("highlight.packdump"), Into::into);
    let mut builder = SyntaxSet::load_defaults_newlines().into_builder();
    for name in [
        "TypeScript",
        "TypeScriptReact",
        "PowerShell",
        "TOML",
        "Dockerfile",
    ] {
        let mut source = fs::read_to_string(assets.join(format!("{name}.sublime-syntax")))?;
        if name == "PowerShell" {
            // Oniguruma repeats the ASCII hex class through a subexpression call.
            // Fancy-regex can use the same class directly.
            let subexpression = r"\g<1>{1,5}";
            assert_eq!(source.matches(subexpression).count(), 1);
            source = source.replace(subexpression, "[0-9a-fA-F]{1,5}");
        }
        builder.add(SyntaxDefinition::load_from_str(&source, true, None)?);
    }
    dumps::dump_to_uncompressed_file(&builder.build(), output)?;
    Ok(())
}
