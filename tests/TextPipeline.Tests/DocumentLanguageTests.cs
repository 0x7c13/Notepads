// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Globalization;
using System.Text.Json;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Sessions.Contracts;

namespace NotepadsEditorTests;

internal static class DocumentLanguageTests
{
    internal static void Run()
    {
        Check("csharp", "Code.CS");
        Check("cpp", "header.H");
        Check("xml", "Notepads.csproj");
        Check("json", "settings.jsonc");
        Check("markdown", "README.mdown");
        Check("toml", "Cargo.lock");
        Check("python", "Untitled.txt", "#!/usr/bin/env -S python3.12 -u\rprint('hello')");
        Check("bash", "no-extension", "#!/bin/bash\r# comment");
        Check("javascript", "script", "#!/usr/bin/env NODE_ENV=production node\r");
        Check("lua", "script", "#!/usr/bin/lua5.4\r");
        Check("plaintext", "Untitled.txt", "#! /usr/bin/pythonista\r");
        Check("plaintext", "Untitled.txt", "comment\r#!/usr/bin/python\r");
        Check("csharp", "program.cs", "#!/bin/bash\r");
        Check("plaintext", "Unknown.bin", new string(' ', 1024) + "#!/bin/bash");
        var culture = CultureInfo.CurrentCulture;
        try { CultureInfo.CurrentCulture = new CultureInfo("tr-TR"); Check("python", "SCRIPT.PY"); }
        finally { CultureInfo.CurrentCulture = culture; }
        if (DocumentLanguages.NormalizeOverride("unknown") != null || DocumentLanguages.NormalizeOverride(null) != null)
            throw new Exception("Unknown/V1 language overrides must become Automatic.");

        var data = new TextEditorSessionDataV2 { StateMetaData = new DocumentMetadata() };
        Check("plaintext", "script", "#!/usr/bin/python...");
        var previousHash = data.ComputeSha256();
        var automatic = JsonSerializer.Serialize(data, SessionJsonContext.Default.TextEditorSessionDataV2);
        if (automatic.Contains("LanguageOverride", StringComparison.Ordinal)) throw new Exception("Automatic altered previous JSON bytes.");
        data.StateMetaData.LanguageOverride = "plaintext";
        var clonedMetadata = data.StateMetaData.Clone();
        clonedMetadata.LanguageOverride = "csharp";
        if (data.StateMetaData.LanguageOverride != "plaintext") throw new Exception("Metadata clone shares mutable state.");
        var json = JsonSerializer.Serialize(data, SessionJsonContext.Default.TextEditorSessionDataV2);
        var restored = JsonSerializer.Deserialize(json, SessionJsonContext.Default.TextEditorSessionDataV2);
        if (restored.StateMetaData.LanguageOverride != "plaintext" || data.ComputeSha256() == previousHash)
            throw new Exception("Manual Plain Text is not preserved and covered by descriptor identity.");
        data.StateMetaData.LanguageOverride = null;
        if (data.ComputeSha256() != previousHash) throw new Exception("Returning to Automatic did not restore descriptor identity.");
        Console.WriteLine("PASS: bounded language recognition, filename precedence, ordinal matching, manual language metadata and unchanged legacy JSON bytes.");
    }

    private static void Check(string expected, string name, string sample = "")
    {
        var actual = DocumentLanguages.Detect(name, sample).Id;
        if (actual != expected) throw new Exception($"Expected {expected}, got {actual} for {name}.");
    }
}
