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
            // Exactly 4 KiB including CR: ordinary page caching should still apply.
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

            // Grow an already cached line beyond the boundary through a real edit.
            native.EmptyUndoBuffer();
            await EditorTestLayout.AfterPaintAsync(native, () => native.InsertText(4095, "x"));
            Check(native.LayoutCache == WinUIEditor.LineCache.Caret &&
                core.GetText() == new string('x', 4096) + "\rshort 中文😀",
                "Editing a line past 4 KiB falls back before retaining page-sized long-line layouts.");
            await EditorTestLayout.AfterPaintAsync(native, native.Undo);
            Check(core.GetText() == text && native.LayoutCache == WinUIEditor.LineCache.Caret,
                "Undo preserves text and does not repeatedly switch cache policies.");

            // Loading multiple long lines must use the same fallback, including away from the caret.
            using var loaded = new TextEditorCore { Width = 600, Height = 320, TextWrapping = TextWrapping.NoWrap };
            var loadedNative = ((WinUIEditor.EditorBaseControl)((Grid)loaded.Content).Children[0]).Editor;
            var longText = "short\r" + string.Concat(Enumerable.Repeat(new string('x', 65536) + "\r", 40));
            await loaded.LoadTextAsync(longText);
            await EditorTestLayout.AfterPaintAsync(loadedNative, () => Window.Current.Content = loaded);
            Check(loadedNative.LayoutCache == WinUIEditor.LineCache.Caret && loaded.GetText() == longText,
                "Long lines outside the caret trigger single-line caching without changing document contents.");
            await EditorTestLayout.AfterPaintAsync(loadedNative, () => loadedNative.FirstVisibleLine = 20);
            Check(loadedNative.FirstVisibleLine == 20 && loadedNative.CurrentPos == 0,
                "Scrolling long lines retains an independently positioned caret.");
        }
        finally { Window.Current.Content = previousContent; }
        log.AppendLine("PASS: page layouts refresh on zoom; long-line load/edit falls back to one cached line.");
        log.AppendLine("PASS: cache fallback preserves Unicode text, undo and independent viewport/caret positions.");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
