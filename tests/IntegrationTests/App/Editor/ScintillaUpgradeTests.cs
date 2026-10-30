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
using WinUIEditor;
using static NotepadsEditorTests.EditorTestLayout;

namespace NotepadsEditorTests;

internal static class ScintillaUpgradeTests
{
    internal static async Task RunAsync(StringBuilder log)
    {
        var previous = Window.Current.Content;
        using var core = new TextEditorCore { Width = 620, Height = 400, FontSize = 18, TextWrapping = TextWrapping.NoWrap };
        var view = (EditorBaseControl)((Grid)core.Content).Children[0];
        var editor = view.Editor;
        try
        {
            core.SetText("abc\rdef");
            await AfterPaintAsync(editor, () => Window.Current.Content = core);
            await CheckUpdateAsync(editor, () =>
            {
                editor.InsertText(5, "X");
                editor.InsertText(1, "\r");
            }, Update.Text | Update.LineCount, 1);
            await CheckUpdateAsync(editor, () => editor.SetSelection(3, 3), Update.Selection, -1);
            await CheckReentrantUpdateAsync(editor);
            await CheckDocumentRepresentationResetAsync(editor);
            await CheckEmptyRepresentationsAsync(editor);

            // Compare the same control glyph in a short line (ordinary segment
            // measurement) and a long line (the new single-byte lookup).
            foreach (var symbol in new[] { 0, (int)'*' })
            {
                editor.ControlCharSymbol = symbol;
                await AfterPaintAsync(editor, () => core.SetText("\u0001x\r\u0001" + new string('x', 2100)));
                var second = editor.PositionFromLine(1);
                var shortWidth = editor.PointXFromPosition(1) - editor.PointXFromPosition(0);
                var longWidth = editor.PointXFromPosition(second + 1) - editor.PointXFromPosition(second);
                Check(shortWidth > 0 && Math.Abs(shortWidth - longWidth) <= 1,
                    "Control glyph width agrees across the optimization threshold");
            }
            editor.ControlCharSymbol = 0;
            editor.SetRepresentation("\r\n", "END");
            editor.ViewEOL = true;
            var firstEndingWidth = await MeasureEndingWidthAsync(editor, "x");
            var secondEndingWidth = await MeasureEndingWidthAsync(editor, new string('x', 2100));
            Check(firstEndingWidth > 0 && Math.Abs(firstEndingWidth - secondEndingWidth) <= 1,
                $"The long-line lookup preserves combined CRLF representations: short={firstEndingWidth}, long={secondEndingWidth}");
            editor.ViewEOL = false;
            editor.ClearRepresentation("\u0001");
            await AfterPaintAsync(editor, () => core.SetText(new string('x', 2400) + "\u0001"));
            Check(editor.PointXFromPosition(editor.Length) > editor.PointXFromPosition(0),
                "A cleared control representation does not break long-line layout");

            await AfterPaintAsync(editor, () =>
            {
                core.SetText("\tx");
                // Plain-text documents omit per-byte style storage. Hide
                // their actual style instead of assigning an unavailable run.
                editor.StyleSetVisible(0, false);
                // Exercise Scintilla's segment geometry. The separate bidi
                // hit-test layout does not implement invisible styles.
                editor.Bidirectional = Bidirectional.Disabled;
            });
            var hiddenTabWidth = editor.PointXFromPosition(1) - editor.PointXFromPosition(0);
            Check(!editor.StyleGetVisible(0) && hiddenTabWidth == 0,
                $"A hidden tab does not leave a tab-stop-sized gap: width={hiddenTabWidth}");
            editor.StyleSetVisible(0, true);
            editor.Bidirectional = Bidirectional.L2r;

            // Worker threads must see a fully prepared immutable lookup, even
            // when their first lines all exceed the single-byte threshold.
            editor.LayoutThreads = 4;
            var row = string.Concat(Enumerable.Repeat("abc\u0002 ", 480));
            await AfterPaintAsync(editor, () =>
            {
                core.TextWrapping = TextWrapping.Wrap;
                core.SetText(string.Join('\r', Enumerable.Repeat(row, 64)));
            });
            var wrapped = editor.WrapCount(0);
            Check(wrapped > 1, "Parallel wrap fixture actually wraps");
            for (var line = 1; line < 64; ++line)
                Check(editor.WrapCount(line) == wrapped, "Parallel line layouts retain identical geometry");
            await AfterPaintAsync(editor, () => core.SetText(string.Empty));
            Check(editor.PointXFromPosition(0) >= 0, "An empty final document still lays out");
            log.AppendLine("PASS: Scintilla update notifications including callback edits, compact-layout boundaries, control representations and parallel wrapping.");
        }
        finally { Window.Current.Content = previous; }
    }

    private static async Task CheckEmptyRepresentationsAsync(Editor editor)
    {
        var bidirectional = editor.Bidirectional;
        var tabDrawMode = editor.TabDrawMode;
        var controlSymbol = editor.ControlCharSymbol;
        try
        {
            editor.Bidirectional = Bidirectional.Disabled;
            editor.ControlCharSymbol = 0;
            foreach (var character in new[] { "\u0003", "中", "\t" })
            {
                var original = editor.GetRepresentation(character);
                var appearance = editor.GetRepresentationAppearance(character);
                try
                {
                    editor.SetRepresentation(character, string.Empty);
                    editor.TabDrawMode = TabDrawMode.ControlChar;
                    foreach (var suffix in new[] { string.Empty, new string('x', 2100) })
                    {
                        var bytes = Encoding.UTF8.GetByteCount(character);
                        async Task<int> WidthAsync(RepresentationAppearance mode)
                        {
                            editor.SetRepresentationAppearance(character, mode);
                            // Change the final text as well: rewriting identical
                            // text can retain the existing per-line positions.
                            var tail = mode == RepresentationAppearance.Blob ? "xx" : "x";
                            await AfterPaintAsync(editor, () => editor.SetText(character + suffix + tail));
                            return editor.PointXFromPosition(bytes) - editor.PointXFromPosition(0);
                        }
                        var plainWidth = await WidthAsync(RepresentationAppearance.Plain);
                        Check(plainWidth == 0,
                            $"Empty plain representation U+{(int)character[0]:X4}, suffix={suffix.Length}: expected=0, actual={plainWidth}");
                        var blobWidth = await WidthAsync(RepresentationAppearance.Blob);
                        Check(blobWidth > 0,
                            $"Empty blob representation U+{(int)character[0]:X4}, suffix={suffix.Length} retains padding: actual={blobWidth}");
                    }
                    if (character == "\t")
                    {
                        editor.TabDrawMode = TabDrawMode.LongArrow;
                        editor.SetRepresentationAppearance(character, RepresentationAppearance.Plain);
                        await AfterPaintAsync(editor, () => editor.SetText("\tx"));
                        Check(editor.PointXFromPosition(1) > editor.PointXFromPosition(0),
                            "An ordinary tab retains its tab stop even when its representation is empty");
                    }
                }
                finally
                {
                    if (original.Length == 0) editor.ClearRepresentation(character);
                    else
                    {
                        editor.SetRepresentation(character, original);
                        editor.SetRepresentationAppearance(character, appearance);
                    }
                }
            }
        }
        finally
        {
            editor.ControlCharSymbol = controlSymbol;
            editor.TabDrawMode = tabDrawMode;
            editor.Bidirectional = bidirectional;
        }
    }

    private static async Task<int> MeasureEndingWidthAsync(Editor editor, string body)
    {
        await AfterPaintAsync(editor, () =>
        {
            editor.SetText(body);
            // Tracking deliberately retains the old canvas until its widen
            // tick. Start each measurement at the same minimal extent so the
            // next paint publishes this fixture's width, not an earlier row's.
            editor.ScrollWidth = 1;
        });
        var bodyWidth = editor.ScrollWidth;
        // The managed convenience setter normalizes newlines to CR. Use the
        // native API, and measure painted extent instead of hit-testing inside
        // a CRLF pair (bidi hit testing clamps those positions to the body).
        await AfterPaintAsync(editor, () =>
        {
            editor.SetText(body + "\r\nz");
            editor.ScrollWidth = 1;
        });
        Check(editor.GetCharAt(body.Length) == '\r' && editor.GetCharAt(body.Length + 1) == '\n',
            "The CRLF fixture retains both bytes");
        return editor.ScrollWidth - bodyWidth;
    }

    private static async Task CheckDocumentRepresentationResetAsync(Editor editor)
    {
        var document = editor.DocPointer;
        var bidirectional = editor.Bidirectional;
        editor.AddRefDocument(document);
        try
        {
            editor.Bidirectional = Bidirectional.Disabled;
            var text = "\u0001" + new string('x', 2100);
            await AfterPaintAsync(editor, () => editor.SetText(text));
            var defaultWidth = editor.PointXFromPosition(1) - editor.PointXFromPosition(0);
            editor.SetRepresentation("\u0001", "wide-control-representation");
            // Change the cached line too: SetRepresentation alone does not
            // invalidate Scintilla's existing per-line positions.
            await AfterPaintAsync(editor, () => editor.SetText(text + "y"));
            var customWidth = editor.PointXFromPosition(1) - editor.PointXFromPosition(0);
            Check(customWidth > defaultWidth + 10,
                $"The document-switch fixture warms a distinct control width: default={defaultWidth}, custom={customWidth}");

            var restoredWidth = -1;
            await AfterPaintAsync(editor, () =>
            {
                // Exercise the native document switch without a font or tab
                // setter or ClearAll/SetText that would incidentally invalidate
                // the width cache. This fixture contains only single-byte text.
                editor.DocPointer = 0;
                editor.AddText(text.Length, text);
                restoredWidth = editor.PointXFromPosition(1) - editor.PointXFromPosition(0);
            });
            Check(editor.GetRepresentation("\u0001") == "SOH" && restoredWidth == defaultWidth,
                $"Document replacement resets cached control widths: expected={defaultWidth}, actual={restoredWidth}");
        }
        finally
        {
            try { editor.DocPointer = document; }
            finally { editor.ReleaseDocument(document); editor.Bidirectional = bidirectional; }
        }
    }

    private static async Task CheckReentrantUpdateAsync(Editor editor)
    {
        var completion = new TaskCompletionSource<int>(TaskCreationOptions.RunContinuationsAsynchronously);
        var firstPosition = -1;
        var injected = false;
        void OnUpdate(Editor sender, UpdateUIEventArgs args)
        {
            if (((Update)args.Updated & Update.Text) == 0) return;
            if (!injected)
            {
                injected = true;
                firstPosition = args.Position;
                editor.InsertText(1, "R");
            }
            else completion.TrySetResult(args.Position);
        }
        editor.UpdateUI += OnUpdate;
        try
        {
            editor.InsertText(5, "Q");
            var nextPosition = await completion.Task.WaitAsync(TimeSpan.FromSeconds(5));
            Check(firstPosition == 5 && nextPosition == 1,
                "An edit inside UpdateUI receives a separate notification with its own earliest byte");
        }
        finally { editor.UpdateUI -= OnUpdate; }
    }

    private static async Task CheckUpdateAsync(Editor editor, Action edit, Update flags, int position)
    {
        var completion = new TaskCompletionSource<int>(TaskCreationOptions.RunContinuationsAsynchronously);
        void OnUpdate(Editor sender, UpdateUIEventArgs args)
        {
            if (((Update)args.Updated & flags) == flags) completion.TrySetResult(args.Position);
        }
        editor.UpdateUI += OnUpdate;
        try
        {
            edit();
            Check(await completion.Task.WaitAsync(TimeSpan.FromSeconds(5)) == position,
                "UpdateUI exposes the earliest changed byte, or -1 for selection-only updates");
        }
        finally { editor.UpdateUI -= OnUpdate; }
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
