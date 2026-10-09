// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.IO;
using Notepads.Features.Documents.Storage;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Theming;
using Windows.UI.Xaml;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    private bool _diffPreview;
    private long _diffHighlightRow;
    private long _diffHighlightCount;
    private int _diffHighlightColor;
    internal event EventHandler ViewportChanged;
    internal event EventHandler AppearanceChanged;
    internal long FirstVisibleRow => Native.FirstVisibleLine;
    internal long HorizontalScrollOffset => Native.XOffset;

    internal void ConfigureDiffPreview()
    {
        _diffPreview = true;
        TextWrapping = TextWrapping.NoWrap;
        DisplayLineNumbers = true;
        DisplayLineHighlighter = false;
    }

    internal bool CanPrepareDiff(long savedBytes, long savedLines) =>
        Native.CanPrepareDiff(checked((ulong)savedBytes), checked((ulong)savedLines));

    internal Task LoadDiffSourceAsync(Stream source, long byteLength, CancellationToken cancellationToken) =>
        LoadNativeDocumentAsync(async () =>
        {
            using var input = source.AsInputStream();
            await Native.LoadUtf8Async(input, checked((ulong)byteLength), false).AsCompletionTask(cancellationToken);
        });

    internal void MakeDiffReadOnly() => Native.ReadOnly = true;

    internal async Task LoadDiffSnapshotsAsync(DocumentBaseline baseline, TextEditorCore oldPane,
        TextEditorCore currentPane, CancellationToken cancellationToken)
    {
        // Keep reader ownership on UI. Worker stream disposal must not release
        // the original document lease behind preparation/publication.
        using var reader = AcquireDocumentReader();
        using var current = new EditorDocumentStream(reader, ownsReader: false);
        using var loads = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        var oldLoad = oldPane.LoadBaselineAsync(baseline, false, loads.Token);
        var newLoad = currentPane.LoadDiffSourceAsync(current, checked((long)reader.Length), loads.Token);
        var first = await Task.WhenAny(oldLoad, newLoad);
        if (first.IsFaulted || first.IsCanceled) loads.Cancel();
        await Task.WhenAll(oldLoad, newLoad);
    }

    internal async Task<DiffComparison> CompareAsync(TextEditorCore other, CancellationToken cancellationToken) =>
        new(await Native.CompareAsync(other.Native).AsCompletionTask(cancellationToken));

    internal void ApplyDiffPresentation(DiffComparison result, bool oldSide)
    {
        Native.ApplyDiffPresentation(result.NativeResult, oldSide);
        _diffHighlightRow = _diffHighlightCount = 0;
    }

    internal void ApplyDiffPalette(DiffColorPalette palette, bool oldSide)
    {
        Native.SetDiffColours(
            ToScintillaColor(palette[oldSide ? DiffColorRole.DeletedLine : DiffColorRole.AddedLine]) |
            (palette[oldSide ? DiffColorRole.DeletedLine : DiffColorRole.AddedLine].A << 24),
            ToScintillaColor(palette[oldSide ? DiffColorRole.DeletedText : DiffColorRole.AddedText]) |
            (palette[oldSide ? DiffColorRole.DeletedText : DiffColorRole.AddedText].A << 24),
            ToScintillaColor(palette[DiffColorRole.Gap]) | (palette[DiffColorRole.Gap].A << 24),
            ToScintillaColor(palette[DiffColorRole.GapHatch]) | (palette[DiffColorRole.GapHatch].A << 24));
        var border = palette[DiffColorRole.ActiveChangeBorder];
        _diffHighlightColor = ToScintillaColor(border) | (border.A << 24);
        Native.SetDiffHighlight(_diffHighlightRow, _diffHighlightCount, _diffHighlightColor);
    }

    internal void HighlightDiffChange(long row, long count)
    {
        Native.SetDiffHighlight(row, count, _diffHighlightColor);
        _diffHighlightRow = row;
        _diffHighlightCount = count;
    }

    internal void SetDiffViewport(long row, long horizontalOffset)
    {
        Native.FirstVisibleLine = row;
        Native.XOffset = checked((int)horizontalOffset);
    }

    internal void CopyDiffSelection() => Native.Copy();
    internal void SelectDiffDocument() => Native.SelectAll();
}
