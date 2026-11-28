// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Text;
using System.Threading.Tasks;
using Notepads.Infrastructure.Storage;
using Windows.Storage;

namespace NotepadsEditorTests;

internal static class FileLogSinkTests
{
    [UnsafeAccessor(UnsafeAccessorKind.Method, Name = "set_File")]
    private static extern void SetFile(FileLogSink sink, StorageFile file);

    internal static async Task RunAsync(StorageFolder folder, StringBuilder log)
    {
        var first = new FileLogSink();
        var second = new FileLogSink();
        try
        {
            await Task.WhenAll(first.InitializeAsync(), second.InitializeAsync());
            if (first.File.Path == second.File.Path)
                throw new InvalidOperationException("Independent log writers must never share or replace the same log file.");
            await Task.WhenAll(first.AppendAsync(new[] { "first writer" }), second.AppendAsync(new[] { "second writer" }));
            var firstLines = await FileIO.ReadLinesAsync(first.File);
            var secondLines = await FileIO.ReadLinesAsync(second.File);
            if (!firstLines.SequenceEqual(new[] { "first writer" }) || !secondLines.SequenceEqual(new[] { "second writer" }))
                throw new InvalidOperationException("Independent log writers must retain their own messages without truncation or interleaving.");
            var retained = first.File;
            await first.InitializeAsync();
            if (!ReferenceEquals(retained, first.File) || !(await FileIO.ReadLinesAsync(first.File)).SequenceEqual(new[] { "first writer" }))
                throw new InvalidOperationException("An initialized log writer must retain its existing file.");
        }
        finally
        {
            var sameFile = first.File != null && second.File != null && first.File.Path == second.File.Path;
            try { if (first.File != null) await first.File.DeleteAsync(); }
            finally { if (second.File != null && !sameFile) await second.File.DeleteAsync(); }
        }
        log.AppendLine("PASS: independently initialized log sinks retain distinct files and messages; repeated initialization keeps the existing file.");

        var file = await folder.CreateFileAsync("shared-log.log");
        var sink = new FileLogSink();
        SetFile(sink, file);
        var appends = Enumerable.Range(0, 128).Select(index => sink.AppendAsync(new[] { $"line-{index}" })).ToArray();
        var observations = Enumerable.Range(0, 128).Select(_ => FileStorage.GetDateModifiedAsync(file)).ToArray();
        await Task.WhenAll(appends.Concat<Task>(observations)).WaitAsync(TimeSpan.FromSeconds(20));
        var lines = await FileIO.ReadLinesAsync(file);
        if (lines.Count != appends.Length || lines.Distinct().Count() != appends.Length)
            throw new InvalidOperationException("Shared-file log appends and metadata observations must preserve every line.");
        log.AppendLine("PASS: shared log StorageFile serializes 128 appends and metadata observations without losing lines.");
    }
}
