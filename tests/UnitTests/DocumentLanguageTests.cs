// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System.Globalization;
using System.Text.Json;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Sessions.Contracts;

namespace Notepads.UnitTests;

public sealed class DocumentLanguageTests
{
    [Theory]
    [InlineData("csharp", "Code.CS", "")]
    [InlineData("cpp", "header.H", "")]
    [InlineData("xml", "Notepads.csproj", "")]
    [InlineData("json", "settings.jsonc", "")]
    [InlineData("markdown", "README.mdown", "")]
    [InlineData("toml", "Cargo.lock", "")]
    [InlineData("python", "Untitled.txt", "#!/usr/bin/env -S python3.12 -u\rprint('hello')")]
    [InlineData("bash", "no-extension", "#!/bin/bash\r# comment")]
    [InlineData("javascript", "script", "#!/usr/bin/env NODE_ENV=production node\r")]
    [InlineData("lua", "script", "#!/usr/bin/lua5.4\r")]
    [InlineData("plaintext", "Untitled.txt", "#! /usr/bin/pythonista\r")]
    [InlineData("plaintext", "Untitled.txt", "comment\r#!/usr/bin/python\r")]
    [InlineData("csharp", "program.cs", "#!/bin/bash\r")]
    [InlineData("plaintext", "script", "#!/usr/bin/python...")]
    public void DetectUsesFilenameThenSupportedFirstLineInterpreter(string expected, string name, string sample) =>
        Assert.Equal(expected, DocumentLanguages.Detect(name, sample).Id);

    [Fact]
    public void DetectionDoesNotScanBeyondTheBoundedPrefix() =>
        Assert.Equal("plaintext", DocumentLanguages.Detect("Unknown.bin", new string(' ', 1024) + "#!/bin/bash").Id);

    [Fact]
    public void FilenameMatchingIsOrdinalInTurkishCulture()
    {
        var culture = CultureInfo.CurrentCulture;
        try
        {
            CultureInfo.CurrentCulture = new CultureInfo("tr-TR");
            Assert.Equal("python", DocumentLanguages.Detect("SCRIPT.PY", "").Id);
        }
        finally { CultureInfo.CurrentCulture = culture; }
    }

    [Theory]
    [InlineData(null)]
    [InlineData("unknown")]
    public void LegacyOrUnknownOverrideBecomesAutomatic(string value) =>
        Assert.Null(DocumentLanguages.NormalizeOverride(value));

    [Fact]
    public void ManualPlainTextIsPersistedWhileAutomaticRetainsLegacyDescriptorIdentity()
    {
        var data = new TextEditorSessionDataV2 { StateMetaData = new DocumentMetadata() };
        var previousHash = data.ComputeSha256();
        var automatic = JsonSerializer.Serialize(data, SessionJsonContext.Default.TextEditorSessionDataV2);
        Assert.DoesNotContain("LanguageOverride", automatic, StringComparison.Ordinal);
        data.StateMetaData.LanguageOverride = "plaintext";
        var clonedMetadata = data.StateMetaData.Clone();
        clonedMetadata.LanguageOverride = "csharp";
        Assert.Equal("plaintext", data.StateMetaData.LanguageOverride);
        var json = JsonSerializer.Serialize(data, SessionJsonContext.Default.TextEditorSessionDataV2);
        var restored = JsonSerializer.Deserialize(json, SessionJsonContext.Default.TextEditorSessionDataV2);
        Assert.Equal("plaintext", restored.StateMetaData.LanguageOverride);
        Assert.NotEqual(previousHash, data.ComputeSha256());
        data.StateMetaData.LanguageOverride = null;
        Assert.Equal(previousHash, data.ComputeSha256());
    }
}
