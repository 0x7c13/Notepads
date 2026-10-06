// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;

namespace Notepads.Features.Documents.FileTypes;

public static class DocumentLanguages
{
    public const int DetectionSampleBytes = 1024;
    public static DocumentLanguage PlainText { get; } = new("plaintext", "Plain Text");
    private static readonly DocumentLanguage[] _languages =
    [
        PlainText,
        new("bash", "Shell", ".sh", ".bash", ".zsh"),
        new("c", "C", ".c"),
        new("cpp", "C++", ".cpp", ".cc", ".cxx", ".h", ".hpp", ".hxx", ".hh"),
        new("csharp", "C#", ".cs", ".csx"),
        new("css", "CSS", ".css"),
        new("html", "HTML", ".html", ".htm", ".xhtml"),
        new("java", "Java", ".java"),
        new("javascript", "JavaScript", ".js", ".mjs", ".cjs", ".jsx"),
        new("json", "JSON", ".json", ".jsonc", ".jsonl"),
        new("lua", "Lua", ".lua"),
        new("markdown", "Markdown", ".md", ".markdown"),
        new("powershell", "PowerShell", ".ps1", ".psm1", ".psd1"),
        new("python", "Python", ".py", ".pyw", ".pyi"),
        new("rust", "Rust", ".rs"),
        new("sql", "SQL", ".sql"),
        new("toml", "TOML", ".toml"),
        new("typescript", "TypeScript", ".ts", ".mts", ".cts", ".tsx"),
        new("xml", "XML", ".xml", ".xaml", ".csproj", ".props", ".targets", ".resx", ".svg", ".config"),
        new("yaml", "YAML", ".yaml", ".yml")
    ];

    public static IReadOnlyList<DocumentLanguage> All { get; } = new ReadOnlyCollection<DocumentLanguage>(_languages);

    public static DocumentLanguage Find(string id)
    {
        foreach (var language in _languages)
            if (string.Equals(language.Id, id, StringComparison.Ordinal)) return language;
        return null;
    }

    public static string NormalizeOverride(string id) => Find(id)?.Id;

    public static DocumentLanguage Detect(string fileName, string firstLineSample)
    {
        fileName = Path.GetFileName(fileName ?? string.Empty);
        if (string.Equals(fileName, "Cargo.lock", StringComparison.OrdinalIgnoreCase)) return Find("toml");
        var extension = Path.GetExtension(fileName);
        foreach (var language in _languages)
            foreach (var candidate in language.Extensions)
                if (string.Equals(extension, candidate, StringComparison.OrdinalIgnoreCase)) return language;
        return DetectShebang(firstLineSample) ?? PlainText;
    }

    private static DocumentLanguage DetectShebang(string sample)
    {
        if (string.IsNullOrEmpty(sample) || !sample.StartsWith("#!", StringComparison.Ordinal)) return null;
        sample = sample[..Math.Min(sample.Length, DetectionSampleBytes)];
        var end = sample.IndexOfAny(['\r', '\n']);
        if (end >= 0) sample = sample[..end];
        var words = sample[2..].Split([' ', '\t'], StringSplitOptions.RemoveEmptyEntries);
        if (words.Length == 0) return null;
        var interpreter = Path.GetFileName(words[0]);
        if (interpreter == "env")
        {
            interpreter = null;
            for (var i = 1; i < words.Length; i++)
            {
                if (words[i] == "-u" || words[i] == "--unset") { i++; continue; }
                if (words[i].StartsWith('-') || words[i].Contains('=')) continue;
                interpreter = Path.GetFileName(words[i].Trim('\'', '"'));
                break;
            }
        }
        if (interpreter == null) return null;
        if (interpreter is "sh" or "bash" or "zsh" or "dash" or "ksh") return Find("bash");
        if (interpreter is "node" or "nodejs") return Find("javascript");
        if (interpreter is "pwsh" or "powershell") return Find("powershell");
        if (HasVersionSuffix(interpreter, "python")) return Find("python");
        if (HasVersionSuffix(interpreter, "lua")) return Find("lua");
        return null;
    }

    private static bool HasVersionSuffix(string interpreter, string name)
    {
        if (!interpreter.StartsWith(name, StringComparison.Ordinal)) return false;
        if (interpreter.Length > name.Length && !char.IsAsciiDigit(interpreter[name.Length])) return false;
        foreach (var character in interpreter.AsSpan(name.Length))
            if (character != '.' && (character < '0' || character > '9')) return false;
        return true;
    }
}
