// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.Text;
using NotepadsEditorTests;

internal static class Program
{
    private static async Task Main()
    {
        foreach (var ending in new[] { LineEnding.Cr, LineEnding.Lf, LineEnding.Crlf })
        {
            var sequence = ending == LineEnding.Cr ? "\r" : ending == LineEnding.Lf ? "\n" : "\r\n";
            foreach (var text in new[] {
                "", "no newline\0中文😀", "\r\n\r\n", "a\rb\nc\r\nd\n\re\r",
                // Cross the V2 writer's chunk boundary with Unicode and CRLF.
                new string('a', 16382) + "😀\r\n" + new string('b', 16382) + "\r\n尾\0\n" })
            {
                var expected = text.Replace("\r\n", "\n").Replace("\r", "\n").Replace("\n", sequence);
                Equal(expected, LineEndingUtility.ApplyLineEnding(text, ending));
                using var stream = new MemoryStream();
                using (var writer = new StreamWriter(stream, new UTF8Encoding(false, true), 1024, true))
                    await LineEndingUtility.WriteAsync(writer, text, ending);
                Equal(expected, new UTF8Encoding(false, true).GetString(stream.ToArray()));
                if (!ReferenceEquals(expected, text) && expected == text)
                {
                    if (!ReferenceEquals(text, LineEndingUtility.ApplyLineEnding(text, ending)))
                        throw new Exception("Already normalized text was copied.");
                }
            }
        }
        using (var writer = new StringWriter())
        {
            const string raw = "\r\n\0😀\n\r";
            await LineEndingUtility.WriteAsync(writer, raw);
            Equal(raw, writer.ToString());
        }
        Console.WriteLine("PASS: V2 newline normalization, streaming UTF-8, embedded NUL, and surrogate/chunk boundaries.");
        await DocumentOperationTests.RunAsync();
        await DocumentTextCodecTests.RunAsync();
        SessionRecoverySchemaTests.Run();
        DocumentLanguageTests.Run();
        IndentationDetectorTests.Run();
    }

    private static void Equal(string expected, string actual)
    {
        if (!string.Equals(expected, actual, StringComparison.Ordinal))
            throw new Exception($"Text mismatch: expected {expected.Length} code units, got {actual.Length}.");
    }
}
