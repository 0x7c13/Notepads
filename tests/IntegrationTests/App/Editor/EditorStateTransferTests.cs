// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Windows.Storage;

namespace NotepadsEditorTests;

internal static partial class EditorStateTransferTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var sourceView = new WinUIEditor.EditorBaseControl();
        var targetView = new WinUIEditor.EditorBaseControl();
        var source = sourceView.Editor;
        var target = targetView.Editor;
        source.CodePage = 65001;
        source.SetText("abc אבג\r中文");
        source.EmptyUndoBuffer();
        source.BeginUndoAction();
        source.SetSel(0, 3); source.PasteText("XYZ");
        source.EndUndoAction(); source.SetSavePoint();
        source.BeginUndoAction(); source.InsertText(3, "!"); source.EndUndoAction();
        source.Undo();
        source.SetSelection(4, 1); source.AddSelection(10, 8); source.MainSelection = 1;
        source.TabWidth = 8; source.Indent = 2; source.UseTabs = true; source.Overtype = true;
        source.AdditionalSelectionTyping = true;
        byte[] bytes;
        using (var reader = source.AcquireStateReader())
        {
            Check(source.ReadOnly && reader.Sequence == source.DocumentSequence, "capture holds the source read lease");
            using var output = new MemoryStream();
            while (true)
            {
                var chunk = await Task.Run(async () => (await reader.ReadAsync()).ToArray());
                if (chunk.Length == 0) break;
                Check(chunk.Length <= 65536, "worker reads marshal bounded chunks to the owning UI");
                output.Write(chunk);
            }
            bytes = output.ToArray();
            Check((ulong)bytes.Length == reader.Length, "capture emits its exact advertised length");
        }
        Check(!source.ReadOnly, "capture release restores source editing");
        var text = source.GetText(source.Length + 1);
        using (var input = new MemoryStream(Encoding.UTF8.GetBytes(text)))
            await target.LoadUtf8Async(input.AsInputStream(), (ulong)input.Length, false);
        // Transfer restores a journal prefix at the source sequence before live state.
        var journal = Path.Combine(ApplicationData.Current.TemporaryFolder.Path, Guid.NewGuid() + ".journal");
        using (File.Create(journal)) { }
        target.StartJournal(journal, source.DocumentSequence);
        try
        {
            using (var input = new MemoryStream(bytes))
                await target.RestoreStateAsync(input.AsInputStream(), (ulong)bytes.Length, Convert.ToHexStringLower(SHA256.HashData(bytes)));
            Check(target.Selections == 2 && target.MainSelection == 1 && target.GetSelectionNCaret(0) == 4 &&
                target.GetSelectionNAnchor(0) == 1 && target.GetSelectionNCaret(1) == 10 && target.GetSelectionNAnchor(1) == 8,
                "complete selections survive installation");
            Check(target.TabWidth == 8 && target.Indent == 2 && target.UseTabs && target.Overtype &&
                target.AdditionalSelectionTyping && !target.ReadOnly, "live editing modes survive installation");
            Check(target.UndoActions == source.UndoActions && target.UndoCurrent == source.UndoCurrent &&
                target.DocumentSequence == source.DocumentSequence, "installation creates no text/journal operation");

            foreach (var failure in new[] { "hash", "format", "truncated" })
            {
                var invalid = failure == "truncated" ? bytes[..^1] : (byte[])bytes.Clone();
                if (failure == "format") invalid[0] ^= 1;
                using var input = new MemoryStream(invalid);
                bool refused = false;
                try { await target.RestoreStateAsync(input.AsInputStream(), (ulong)bytes.Length,
                    failure == "hash" ? new string('0', 64) : Convert.ToHexStringLower(SHA256.HashData(invalid))); }
                catch (Exception) { refused = true; }
                Check(refused && target.GetText(target.Length + 1) == text && target.Selections == 2 &&
                    target.UndoActions == source.UndoActions && !target.ReadOnly,
                    "invalid hash, parser input or early EOF leaves target text, selection, undo and read-only unchanged");
            }
            target.Undo(); Check(target.GetText(target.Length + 1) == "abc אבג\r中文", "moved grouped edit undoes in one step");
            target.Redo(); Check(target.GetText(target.Length + 1) == text, "moved grouped edit redoes in one step");
            target.Redo(); Check(target.GetText(target.Length + 1) == "XYZ! אבג\r中文", "redo tail survives the move");

            target.SetEmptySelection(0); target.Tab();
            Check(target.GetLineIndentation(0) == 2, "native Tab uses restored derived indentation");

            // A cancelled outer operation must drain the uncancelled owner cleanup,
            // not merely schedule eventual destruction of its read lease.
            using var blocked = new BlockedReadStream(bytes);
            var operation = target.RestoreStateAsync(blocked.AsInputStream(), (ulong)bytes.Length,
                Convert.ToHexStringLower(SHA256.HashData(bytes)));
            var completion = operation.AsTask();
            try
            {
                await blocked.Entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
                operation.Cancel();
                Check(!completion.IsCompleted && target.ReadOnly, "cancellation retains ownership until the blocked read drains");
            }
            finally { blocked.Release.TrySetResult(); }
            bool canceled = false;
            try { await completion; }
            catch (OperationCanceledException) { canceled = true; }
            Check(canceled && !target.ReadOnly, "completed cancellation has already released the native read lease");
        }
        finally { await target.StopJournalAsync(); File.Delete(journal); }
        log.AppendLine("PASS: live native state streams across apartments, validates before publication and retains undo/redo and selections.");
        GC.KeepAlive(sourceView);
        GC.KeepAlive(targetView);
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private sealed partial class BlockedReadStream(byte[] bytes) : MemoryStream(bytes)
    {
        internal readonly TaskCompletionSource Entered = new(TaskCreationOptions.RunContinuationsAsynchronously);
        internal readonly TaskCompletionSource Release = new(TaskCreationOptions.RunContinuationsAsynchronously);
        private int _entered;

        private async Task WaitAsync()
        {
            if (Interlocked.Exchange(ref _entered, 1) != 0) return;
            Entered.TrySetResult();
            await Release.Task.ConfigureAwait(false);
        }

        public override async Task<int> ReadAsync(byte[] buffer, int offset, int count, CancellationToken cancellationToken)
        {
            await WaitAsync().ConfigureAwait(false);
            return base.Read(buffer, offset, count);
        }

        public override async ValueTask<int> ReadAsync(Memory<byte> buffer, CancellationToken cancellationToken = default)
        {
            await WaitAsync().ConfigureAwait(false);
            return base.Read(buffer.Span);
        }
    }
}
