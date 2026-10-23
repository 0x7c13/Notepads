// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.TextEditor;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace NotepadsEditorTests;

internal static class LineLayoutCacheTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var previousContent = Window.Current.Content;
        using var core = new TextEditorCore { Width = 600, Height = 320, TextWrapping = TextWrapping.NoWrap };
        var native = ((WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0]).Editor;
        try
        {
            // Exercise cache reuse, then grow the same line through a real edit.
            var text = new string('x', 4095) + "\rshort 中文😀";
            await core.LoadTextAsync(text);
            await EditorTestLayout.AfterPaintAsync(native, () =>
            {
                Window.Current.Content = core;
                Window.Current.Activate();
            });
            Check(native.LayoutCache == WinUIEditor.LineCache.Page, "Ordinary lines retain page caching.");
            var width = native.PointXFromPosition(10) - native.PointXFromPosition(0);
            await EditorTestLayout.AfterPaintAsync(native, () => core.SetFontZoomFactor(200));
            var zoomedWidth = native.PointXFromPosition(10) - native.PointXFromPosition(0);
            Check(zoomedWidth > width * 1.8, "Font changes invalidate cached character positions.");

            native.EmptyUndoBuffer();
            await EditorTestLayout.AfterPaintAsync(native, () => native.InsertText(4095, "x"));
            Check(native.LayoutCache == WinUIEditor.LineCache.Page &&
                core.GetText() == new string('x', 4096) + "\rshort 中文😀",
                "Growing a cached line preserves the configured page cache and edited document.");
            await EditorTestLayout.AfterPaintAsync(native, native.Undo);
            Check(core.GetText() == text && native.LayoutCache == WinUIEditor.LineCache.Page,
                "Undo preserves text and the page-cache policy.");

            // Total visible source exceeds the native retained-capacity budget.
            // Eviction must remain transparent to loading and scrolling.
            using var loaded = new TextEditorCore { Width = 600, Height = 320, TextWrapping = TextWrapping.NoWrap };
            var loadedNative = ((WinUIEditor.EditorBaseControl)((Grid)loaded.Content).Children[0]).Editor;
            var longText = "short\r" + string.Concat(Enumerable.Repeat(new string('x', 65536) + "\r", 40));
            await loaded.LoadTextAsync(longText);
            await EditorTestLayout.AfterPaintAsync(loadedNative, () => Window.Current.Content = loaded);
            Check(loadedNative.LayoutCache == WinUIEditor.LineCache.Page && loaded.GetText() == longText,
                "Long lines outside the caret retain page mode without changing document contents.");
            await EditorTestLayout.AfterPaintAsync(loadedNative, () => loadedNative.FirstVisibleLine = 20);
            Check(loadedNative.FirstVisibleLine == 20 && loadedNative.CurrentPos == 0,
                "Scrolling long lines retains an independently positioned caret.");
        }
        finally { Window.Current.Content = previousContent; }
        log.AppendLine("PASS: page layouts refresh on zoom and remain enabled after long-line load/edit.");
        log.AppendLine("PASS: bounded cache eviction preserves Unicode text, undo and independent viewport/caret positions.");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
