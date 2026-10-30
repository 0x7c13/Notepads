// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Windows.Graphics.Display;
using Windows.Storage;
using Windows.UI;
using Windows.UI.Core;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Media;

namespace NotepadsEditorTests;

// The provider is exercised by a separate desktop UIA client. This host only
// supplies deterministic document/layout setup and independent native state.
internal static class InteractiveEditorHost
{
    private static TextEditorCore _editor;
    private static WinUIEditor.EditorBaseControl _view;
    private static string _channel;
    private static DpiChangeFixture _dpiFixture;
    private static StackPanel _panel;
    private static bool _disposed;
    private static string _case;
    private static int _dropStarted;
    private static int _dragStarted;
    private static int _dragCompleted;
    private static int _dragResult;
    private static int _sequence;
    private static int _scrollStarts;
    private static DateTime _lastCommand;
    private static DocumentBaseline _preparedReplacement;
    private static string _preparedReplacementCase;
    private static string _preparedReplacementText;
    private static (long Anchor, long Caret)? _selectionOnKey;
    private static RichEditBox _imeReference;
    private static byte[] _imeBaseline;
    private static NotepadsIntegrationTests.FeatureTestHost _imeWorkspace;
    private static NotepadsIntegrationTests.EditorToolsKeyboardFixture _editorTools;

    private static WinUIEditor.Editor Native => _view.Editor;

    internal static async Task StartAsync(string arguments)
    {
        var parts = arguments.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        if (parts.Length != 2 || parts[0] != "--interactive-test" || !Guid.TryParseExact(parts[1], "N", out var run))
            throw new ArgumentException("Interactive tests require a unique run GUID.");

        _channel = Path.Combine(ApplicationData.Current.LocalFolder.Path, "InteractiveTests", run.ToString("N"));
        Directory.CreateDirectory(_channel);
        try
        {
            _panel = new StackPanel { Padding = new Thickness(24), Spacing = 12 };
            _panel.Children.Add(new TextBlock { Text = "Notepads isolated editor automation test" });
            CreateEditor();
            Window.Current.Content = new Grid
            {
                Background = new SolidColorBrush(Colors.White),
                Children = { _panel }
            };
            Window.Current.Activate();
            ApplicationView.GetForCurrentView().Title = "Notepads isolated editor automation test";
            ApplicationView.GetForCurrentView().TryResizeView(new Windows.Foundation.Size(800, 600));
            await ResetAsync("", false, 520, 340, 18);
            _lastCommand = DateTime.UtcNow;
            WriteAtomic("ready.txt", "READY\n" + Snapshot());
            var dispatcher = Window.Current.Dispatcher;
            _ = Task.Run(() => RunCommandsAsync(dispatcher));
        }
        catch (Exception ex)
        {
            WriteAtomic("fatal.txt", ex.ToString());
            throw;
        }
    }

    private static async Task RunCommandsAsync(CoreDispatcher dispatcher)
    {
        var exitCode = 1;
        try
        {
            while (true)
            {
                if (_sequence >= 1000 || DateTime.UtcNow - _lastCommand > TimeSpan.FromMinutes(10))
                    throw new TimeoutException("Interactive test exceeded its command or inactivity limit.");

                // XAML timer ticks can be starved by a nested OLE drag loop.
                // Observe files off-thread and enqueue only an actual command;
                // the awaited completion keeps all UI operations serialized.
                var path = Path.Combine(_channel, $"command-{_sequence + 1:D4}.txt");
                if (!File.Exists(path))
                {
                    await Task.Delay(25).ConfigureAwait(false);
                    continue;
                }
                if (await DispatchCommandAsync(dispatcher).ConfigureAwait(false) is { } status)
                {
                    exitCode = status;
                    break;
                }
            }
        }
        catch (Exception ex)
        {
            // The external runner retains an owned process handle and validates
            // this exit even when a closed dispatcher cannot accept an ACK.
            try { WriteAtomic("fatal.txt", ex.ToString()); }
            catch (IOException) { }
            catch (UnauthorizedAccessException) { }
        }
        // The UI command has returned, including all readers and finally blocks.
        // Terminate off-thread so CLR/COM teardown can still use the dispatcher.
        Environment.Exit(exitCode);
    }

    private static async Task<int?> DispatchCommandAsync(CoreDispatcher dispatcher)
    {
        var completed = new TaskCompletionSource<int?>(TaskCreationOptions.RunContinuationsAsynchronously);
        if (!await dispatcher.TryRunAsync(CoreDispatcherPriority.Normal, async () =>
        {
            try { completed.TrySetResult(await ProcessCommandAsync()); }
            catch (Exception ex) { completed.TrySetException(ex); }
        }))
        {
            throw new InvalidOperationException("The interactive editor dispatcher has closed.");
        }
        // TryRunAsync acknowledges dispatch, not the async handler's completion.
        return await completed.Task.ConfigureAwait(false);
    }

    private static async Task<int?> ProcessCommandAsync()
    {
        try
        {
            var next = _sequence + 1;
            var path = Path.Combine(_channel, $"command-{next:D4}.txt");
            if (!File.Exists(path)) return null;
            // The file becomes visible before the publisher's rename handle
            // necessarily closes. Share deletion so this atomic handoff is safe.
            using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read | FileShare.Delete);
            using var reader = new StreamReader(stream, Encoding.UTF8);
            var command = reader.ReadToEnd().Split('|');
            _sequence = next;
            _lastCommand = DateTime.UtcNow;
            try
            {
                var extra = await ExecuteAsync(command);
                if (command[0] == "exit")
                {
                    Window.Current.Content = null;
                    NotepadsTestIsolation.TestApplicationData.ValidatePreservedFileGrants();
                    await LoggingService.FlushAsync();
                    // Do not query the disposed editor to build a final snapshot.
                    WriteAtomic($"response-{next:D4}.txt", "OK\n");
                    return 0;
                }
                else WriteAtomic($"response-{next:D4}.txt", "OK\n" +
                    (command[0].StartsWith("dpi-", StringComparison.Ordinal) ? string.Empty : Snapshot()) + extra);
            }
            catch (Exception ex)
            {
                WriteAtomic($"response-{next:D4}.txt", "ERROR\n" + ex);
                if (command[0] == "exit") return 1;
            }
        }
        catch (Exception ex)
        {
            WriteAtomic("fatal.txt", ex.ToString());
            return 1;
        }
        return null;
    }

    private static async Task<string> ExecuteAsync(string[] command)
    {
        switch (command[0])
        {
            case "font-workspace-geometry" when command.Length == 1:
                return await WorkspaceFontSurfaceGeometryAsync();
            case "font-rendering" when command.Length == 6:
                return await PrepareFontRenderingAsync(int.Parse(command[1], CultureInfo.InvariantCulture),
                    bool.Parse(command[2]), bool.Parse(command[3]), Number(command[4]), Number(command[5]));
            case "font-preference" when command.Length is 3 or 4:
                return await PrepareFontPreferenceAsync(bool.Parse(command[1]), bool.Parse(command[2]),
                    command.Length == 3 || bool.Parse(command[3]));
            case "tabs-start" when command.Length == 3:
                return await NotepadsIntegrationTests.TabDragTestHost.StartAsync(int.Parse(command[1]), command[2]);
            case "tabs-frame" when command.Length == 1:
                await NotepadsIntegrationTests.TabDragTestHost.FrameAsync();
                return NotepadsIntegrationTests.TabDragTestHost.State();
            case "tabs-state" when command.Length == 1:
                return NotepadsIntegrationTests.TabDragTestHost.State();
            case "tabs-focus-action" when command.Length == 1:
                return NotepadsIntegrationTests.TabDragTestHost.FocusStatusAction();
            case "tabs-hold-drop" when command.Length == 2:
                return NotepadsIntegrationTests.TabDragTestHost.SetDropHold(bool.Parse(command[1]));
            case "tabs-close" when command.Length == 2:
                return NotepadsIntegrationTests.TabDragTestHost.Close(int.Parse(command[1]));
            case "dpi-prepare" when command.Length == 4:
                _dpiFixture?.Dispose();
                _dpiFixture = null;
                _dpiFixture = await DpiChangeFixture.CreateAsync(bool.Parse(command[1]), bool.Parse(command[2]), bool.Parse(command[3]));
                return _dpiFixture.State();
            case "dpi-state" when command.Length == 1:
                return (_dpiFixture ?? throw new InvalidOperationException("No DPI fixture.")).State();
            case "dpi-validate" when command.Length == 1:
                return (_dpiFixture ?? throw new InvalidOperationException("No DPI fixture.")).Validate();
            case "reset" when command.Length == 6:
                var text = DecodeText(command[1]);
                await ResetAsync(text, bool.Parse(command[2]), Number(command[3]), Number(command[4]), Number(command[5]));
                break;
            case "begin-case" when command.Length == 2:
                if (!Guid.TryParseExact(command[1], "N", out var caseId)) throw new ArgumentException("Invalid drag case ID.");
                await DiscardPreparedReplacementAsync();
                _case = caseId.ToString("N");
                _dropStarted = _dragStarted = _dragCompleted = _dragResult = 0;
                break;
            case "select" when command.Length == 3:
            case "select-on-key" when command.Length == 3:
                var anchor = long.Parse(command[1], CultureInfo.InvariantCulture);
                var caret = long.Parse(command[2], CultureInfo.InvariantCulture);
                if (anchor < 0 || caret < 0 || anchor > Native.Length || caret > Native.Length)
                    throw new ArgumentOutOfRangeException(nameof(command));
                if (command[0] == "select-on-key") _selectionOnKey = (anchor, caret);
                else Native.SetSelection(caret, anchor);
                break;
            case "geometry" when command.Length == 2:
                var position = long.Parse(command[1], CultureInfo.InvariantCulture);
                if (!_view.IsLoaded || position < 0 || position > Native.Length) throw new ArgumentOutOfRangeException(nameof(position));
                var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
                var geometrySurface = FindImageTarget(_view) ??
                    throw new InvalidOperationException("The native text surface is not available.");
                var point = geometrySurface.TransformToVisual(null).TransformPoint(new Windows.Foundation.Point(
                    Native.PointXFromPosition(position) / scale, Native.PointYFromPosition(position) / scale));
                var bounds = Window.Current.CoreWindow.Bounds;
                return "pointX=" + ((point.X + bounds.X) * scale).ToString(CultureInfo.InvariantCulture) +
                    "\npointY=" + ((point.Y + bounds.Y) * scale).ToString(CultureInfo.InvariantCulture) + "\n";
            case "undo" when command.Length == 1:
                Native.Undo();
                break;
            case "rectangle-step" when command.Length == 2:
                switch (command[1])
                {
                    case "Left": Native.CharLeftRectExtend(); break;
                    case "Right": Native.CharRightRectExtend(); break;
                    case "Down": Native.LineDownRectExtend(); break;
                    default: throw new ArgumentException("Invalid rectangle navigation direction.");
                }
                break;
            case "scroll-caret" when command.Length == 1:
                Native.ScrollCaret();
                break;
            case "vertical-viewport" when command.Length == 3:
                _view.SetVerticalViewport(long.Parse(command[1], CultureInfo.InvariantCulture),
                    double.Parse(command[2], CultureInfo.InvariantCulture));
                break;
            case "caret-visible" when command.Length == 2:
                await EditorTestLayout.AfterPaintAsync(Native, () =>
                {
                    Native.CaretStyle = bool.Parse(command[1]) ? WinUIEditor.CaretStyle.Line : WinUIEditor.CaretStyle.Invisible;
                    Native.CaretFore = Native.AdditionalCaretFore = 0xff00ff;
                    Native.CaretPeriod = 0;
                });
                break;
            case "direction" when command.Length == 2:
                var rightToLeft = bool.Parse(command[1]);
                if (_editor.RightToLeftReadingOrder != rightToLeft)
                    await EditorTestLayout.AfterPaintAsync(Native, () => _editor.RightToLeftReadingOrder = rightToLeft);
                break;
            case "line-numbers" when command.Length == 2:
                await EditorTestLayout.AfterPaintAsync(Native, () => _editor.DisplayLineNumbers = bool.Parse(command[1]));
                return GutterState();
            case "gutter-state" when command.Length == 1:
                return GutterState();
            case "buffered-draw" when command.Length == 2:
                await EditorTestLayout.AfterPaintAsync(Native, () =>
                {
                    Native.BufferedDraw = bool.Parse(command[1]);
                    // Changing the buffer mode alone does not invalidate pixels.
                    Native.StyleSetBack((int)WinUIEditor.StylesCommon.LineNumber,
                        Native.StyleGetBack((int)WinUIEditor.StylesCommon.LineNumber));
                });
                break;
            case "x-offset" when command.Length == 2:
                var offset = int.Parse(command[1], CultureInfo.InvariantCulture);
                if (offset < 0 || offset > 10000) throw new ArgumentOutOfRangeException(nameof(offset));
                await EditorTestLayout.AfterPaintAsync(Native, () => Native.XOffset = offset);
                break;
            case "raster" when command.Length == 2 || command.Length == 3 && command[2] == "Caret":
                await EditorTestLayout.AfterPaintAsync(Native, () =>
                {
                    Native.CaretStyle = command.Length == 3 ? WinUIEditor.CaretStyle.Line : WinUIEditor.CaretStyle.Invisible;
                    // The app test preferences are Dark while this isolated
                    // pixel fixture has a white background. Set all observed
                    // ink explicitly rather than inheriting the white caret.
                    Native.CaretFore = 0;
                    Native.CaretPeriod = 0;
                    _editor.DisplayLineHighlighter = false;
                    Native.StyleSetFore(0, 0);
                    Native.StyleSetFore(1, 0x0000ff);
                    Native.StyleSetFore(2, 0xff0000);
                    Native.StyleSetFore((int)WinUIEditor.StylesCommon.LineNumber, 0);
                    Native.StyleSetBack((int)WinUIEditor.StylesCommon.LineNumber, 0xffffff);
                    // Fixed contrasting selection colors force a multicolor
                    // foreground paint, independently of the system accent.
                    Native.SetElementColour(WinUIEditor.Element.SelectionText, unchecked((int)0xffffffff));
                    Native.SetElementColour(WinUIEditor.Element.SelectionInactiveText, unchecked((int)0xffffffff));
                    Native.SetElementColour(WinUIEditor.Element.SelectionBack, unchecked((int)0xffff0000));
                    Native.SetElementColour(WinUIEditor.Element.SelectionInactiveBack, unchecked((int)0xffff0000));
                    Native.StartStyling(0, 0);
                    var styled = bool.Parse(command[1]);
                    var index = 0;
                    for (long position = 0; position < Native.Length;)
                    {
                        var next = Native.PositionAfter(position);
                        Native.SetStyling(next - position, styled ? ++index % 2 + 1 : 0);
                        position = next;
                    }
                    Native.SetEmptySelection(0);
                });
                break;
            case "insert" when command.Length == 2:
                _editor.TypeText(DecodeText(command[1]));
                break;
            case "set" when command.Length == 2:
                _editor.SetText(DecodeText(command[1]));
                break;
            case "prepare-replace" when command.Length == 2:
                await PrepareReplacementAsync(DecodeText(command[1]));
                return "replacementCase=" + _preparedReplacementCase + "\nreplacementText64=" +
                    Convert.ToBase64String(Encoding.UTF8.GetBytes(_preparedReplacementText)) + "\n";
            case "replace" when command.Length is 2 or 4:
                (long Anchor, long Caret)? replacementSelection = command.Length == 4
                    ? (long.Parse(command[2], CultureInfo.InvariantCulture), long.Parse(command[3], CultureInfo.InvariantCulture))
                    : null;
                await ApplyPreparedReplacementAsync(DecodeText(command[1]), replacementSelection);
                return "documentReplaced=True\n";
            case "flags" when command.Length == 4:
                Native.ReadOnly = bool.Parse(command[1]);
                _editor.IsEnabled = bool.Parse(command[2]);
                Native.DragDropEnabled = bool.Parse(command[3]);
                break;
            case "detach" when command.Length == 1:
                await DetachAsync();
                break;
            case "attach" when command.Length == 1:
                await AttachAsync();
                break;
            case "dispose" when command.Length == 1:
                await DiscardPreparedReplacementAsync();
                _editor.Dispose();
                _disposed = true;
                break;
            case "exit" when command.Length == 1:
                KeyboardLayoutTestScope.Restore();
                if (_editorTools != null) await _editorTools.DisposeAsync();
                _editorTools = null;
                if (_imeWorkspace != null) await _imeWorkspace.DisposeAsync();
                _imeWorkspace = null;
                await NotepadsIntegrationTests.TabDragTestHost.DisposeAsync();
                _dpiFixture?.Dispose();
                _dpiFixture = null;
                await DiscardPreparedReplacementAsync();
                if (!_disposed) _editor.Dispose();
                _disposed = true;
                break;
            case "state" when command.Length == 1:
                break;
            case "editor-tools-start" when command.Length == 1:
                if (_editorTools != null) throw new InvalidOperationException("An editor tools fixture already exists.");
                _editorTools = await NotepadsIntegrationTests.EditorToolsKeyboardFixture.CreateAsync();
                return _editorTools.Snapshot();
            case "editor-tools-state" when command.Length == 1:
                return _editorTools.Snapshot();
            case "editor-tools-stop" when command.Length == 1:
                var tools = _editorTools;
                _editorTools = null;
                if (tools != null) await tools.DisposeAsync();
                break;
            case "ime-state" when command.Length == 1:
                // Only IME probes need the OS input profile; ordinary editor
                // and OLE state snapshots use local control observations.
                return "inputLanguage=" + Windows.Globalization.Language.CurrentInputMethodLanguageTag + "\n";
            case "ime-workspace-start" when command.Length == 1:
                if (_imeWorkspace != null) throw new InvalidOperationException("An IME workspace already exists.");
                ApplicationPreferences.IsSessionSnapshotEnabled = true;
                _imeWorkspace = await NotepadsIntegrationTests.FeatureTestHost.CreateAsync();
                return ImeWorkspaceState();
            case "keyboard-layout-polish" when command.Length == 1:
                return KeyboardLayoutTestScope.Begin();
            case "keyboard-layout-restore" when command.Length == 1:
                KeyboardLayoutTestScope.Restore();
                return "keyboardLayoutRestored=True\n";
            case "ime-workspace-new" when command.Length == 2:
                var workspaceEditor = (TextEditor)await _imeWorkspace.Core.CreateNewTextEditorAsync(DecodeText(command[1]));
                _imeWorkspace.Core.OpenTextEditor(workspaceEditor);
                await NotepadsIntegrationTests.TabDragTestHost.FrameAsync();
                var workspaceView = (WinUIEditor.EditorBaseControl)((Grid)workspaceEditor.TestNativeCore.Content).Children[0];
                AutomationProperties.SetAutomationId(workspaceView, "WorkspaceImeEditor");
                workspaceEditor.Focus();
                return ImeWorkspaceState();
            case "ime-workspace-state" when command.Length == 1:
                return ImeWorkspaceState();
            case "ime-reference" when command.Length == 1:
                // A real system RichEdit control provides an independent IME
                // policy oracle; input still comes from the external keyboard.
                if (_imeReference == null)
                {
                    _imeReference = new RichEditBox { Width = 520, Height = 80 };
                    AutomationProperties.SetAutomationId(_imeReference, "ImeReference");
                    _panel.Children.Add(_imeReference);
                }
                _imeReference.Document.SetText(Windows.UI.Text.TextSetOptions.None, string.Empty);
                _imeReference.Focus(FocusState.Programmatic);
                break;
            case "ime-reference-state" when command.Length == 1:
                if (_imeReference == null) throw new InvalidOperationException("No IME reference control.");
                _imeReference.Document.GetText(Windows.UI.Text.TextGetOptions.None, out var referenceText);
                return "referenceText64=" + Convert.ToBase64String(Encoding.UTF8.GetBytes(referenceText)) + "\n";
            case "ime-journal-start" when command.Length == 1:
                if (Native.Length > 128 * 1024 || _view.IsComposing)
                    throw new InvalidOperationException("IME journal fixture requires a bounded, non-composing baseline.");
                _imeBaseline = Encoding.UTF8.GetBytes(_editor.GetText());
                var journalPath = Path.Combine(_channel, "ime-" + Guid.NewGuid().ToString("N") + ".npj");
                using (File.Create(journalPath)) { }
                Native.StartJournal(journalPath, Native.DocumentSequence);
                break;
            case "ime-journal-check" when command.Length == 1:
                return await CheckImeJournalAsync();
            case "ime-journal-stop" when command.Length == 1:
                await Native.StopJournalAsync();
                _imeBaseline = null;
                break;
            default:
                throw new ArgumentException("Unknown interactive command: " + command[0]);
        }
        return string.Empty;
    }

    private static string DecodeText(string encoded)
    {
        var text = Encoding.UTF8.GetString(Convert.FromBase64String(encoded));
        if (Encoding.UTF8.GetByteCount(text) > 128 * 1024) throw new ArgumentException("Fixture text exceeds 128 KiB.");
        return text;
    }

    private static string ImeWorkspaceState()
    {
        var editor = (TextEditor)_imeWorkspace.Core.GetSelectedReadyEditor();
        var core = editor.TestNativeCore;
        var view = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
        return "text64=" + Convert.ToBase64String(Encoding.UTF8.GetBytes(editor.GetText())) +
            "\nundoDepth=" + view.Editor.UndoSequence + "\nisComposing=" + view.IsComposing +
            "\ncanUndo=" + view.Editor.CanUndo() + "\ncanRedo=" + view.Editor.CanRedo() +
            "\ncaret=" + view.Editor.CurrentPos + "\nlength=" + view.Editor.Length +
            "\ntitle64=" + Convert.ToBase64String(Encoding.UTF8.GetBytes(editor.DisplayTitle)) +
            "\ntabCount=" + _imeWorkspace.Core.GetTabs().Length + "\n";
    }

    private static async Task<string> CheckImeJournalAsync()
    {
        if (_imeBaseline == null) throw new InvalidOperationException("No IME journal baseline.");
        using var checkpoint = Native.AcquireJournalCheckpoint();
        await checkpoint.FlushAsync();
        using var baseline = new Windows.Storage.Streams.InMemoryRandomAccessStream();
        using var result = new Windows.Storage.Streams.InMemoryRandomAccessStream();
        using (var writer = new Windows.Storage.Streams.DataWriter(baseline.GetOutputStreamAt(0)))
        {
            writer.WriteBytes(_imeBaseline);
            await writer.StoreAsync();
        }
        using (var input = baseline.GetInputStreamAt(0))
        using (var output = result.GetOutputStreamAt(0))
            await checkpoint.WriteBaselineAsync(input, (ulong)_imeBaseline.Length, output, 128 * 1024);
        using var read = result.GetInputStreamAt(0);
        using var reader = new Windows.Storage.Streams.DataReader(read);
        var count = await reader.LoadAsync(checked((uint)result.Size));
        if (count != result.Size || count != checkpoint.CommittedDocumentByteLength)
            throw new InvalidOperationException("IME journal replay did not restore the committed byte length.");
        var bytes = new byte[count];
        reader.ReadBytes(bytes);
        return "journalText64=" + Convert.ToBase64String(bytes) + "\n";
    }

    private static string GutterState()
    {
        var surface = FindImageTarget(_view) as FrameworkElement ??
            throw new InvalidOperationException("The native text surface is not available.");
        var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        var origin = surface.TransformToVisual(null).TransformPoint(new Windows.Foundation.Point());
        var vertical = FindElement(_view, "VerticalScrollBar") ??
            throw new InvalidOperationException("The native vertical scrollbar is not available.");
        var scrollbarOrigin = vertical.TransformToVisual(null).TransformPoint(new Windows.Foundation.Point());
        var bounds = Window.Current.CoreWindow.Bounds;
        var reveal = (Border)((Grid)_editor.Content).Children[1];
        return "surfaceLeft=" + ((origin.X + bounds.X) * scale).ToString(CultureInfo.InvariantCulture) +
            "\nsurfaceTop=" + ((origin.Y + bounds.Y) * scale).ToString(CultureInfo.InvariantCulture) +
            "\nsurfaceWidth=" + (surface.ActualWidth * scale).ToString(CultureInfo.InvariantCulture) +
            "\nsurfaceHeight=" + (surface.ActualHeight * scale).ToString(CultureInfo.InvariantCulture) +
            "\nscrollbarLeft=" + ((scrollbarOrigin.X + bounds.X) * scale).ToString(CultureInfo.InvariantCulture) +
            "\nscrollbarTop=" + ((scrollbarOrigin.Y + bounds.Y) * scale).ToString(CultureInfo.InvariantCulture) +
            "\nscrollbarWidth=" + (vertical.ActualWidth * scale).ToString(CultureInfo.InvariantCulture) +
            "\nscrollbarHeight=" + (vertical.ActualHeight * scale).ToString(CultureInfo.InvariantCulture) +
            "\ngutterWidth=" + Native.GetMarginWidthN(0) +
            "\nglowLeft=" + reveal.BorderThickness.Left.ToString(CultureInfo.InvariantCulture) +
            "\nglowRight=" + reveal.BorderThickness.Right.ToString(CultureInfo.InvariantCulture) + "\n";
    }

    private static async Task DiscardPreparedReplacementAsync()
    {
        var baseline = _preparedReplacement;
        _preparedReplacement = null;
        _preparedReplacementCase = _preparedReplacementText = null;
        if (baseline != null) await baseline.DisposeAsync();
    }

    private static async Task PrepareReplacementAsync(string text)
    {
        if (_case == null) throw new InvalidOperationException("Begin a drag case before preparing its replacement.");
        await DiscardPreparedReplacementAsync();
        // Production loading receives an already constructed baseline. Creating
        // that fixture can await brokered storage, so do it before holding OLE data.
        RecordDragEvent("editor-replace-baseline-start", "threadId=" + Environment.CurrentManagedThreadId);
        var preparing = text.Length == 0
            ? Task.FromResult(DocumentBaseline.CreateEmpty(Guid.ParseExact(_case, "N")))
            : DocumentBaseline.FromTextAsync(text, Guid.ParseExact(_case, "N"));
        RecordDragEvent("editor-replace-baseline-pending", "threadId=" + Environment.CurrentManagedThreadId);
        _preparedReplacement = await preparing;
        _preparedReplacementCase = _case;
        _preparedReplacementText = text;
        RecordDragEvent("editor-replace-baseline-ready", "threadId=" + Environment.CurrentManagedThreadId);
    }

    private static async Task ApplyPreparedReplacementAsync(string text, (long Anchor, long Caret)? selection)
    {
        if (_preparedReplacement == null || _preparedReplacementCase != _case ||
            !string.Equals(_preparedReplacementText, text, StringComparison.Ordinal))
            throw new InvalidOperationException("Replacement must consume this case's exact prepared text.");
        if (selection is { } range && (range.Anchor < 0 || range.Caret < 0 ||
            range.Anchor > _preparedReplacement.ByteLength || range.Caret > _preparedReplacement.ByteLength))
            throw new ArgumentOutOfRangeException(nameof(selection));
        // Consume once. The local lease is disposed on success or failure, while
        // the production loader owns any reader it needs until loading completes.
        using var baseline = _preparedReplacement;
        _preparedReplacement = null;
        _preparedReplacementCase = _preparedReplacementText = null;
        var previousDocument = Native.DocPointer;
        RecordDragEvent("editor-replace-native-load-start", "threadId=" + Environment.CurrentManagedThreadId);
        var load = _editor.LoadBaselineAsync(baseline, preserveUndo: false);
        RecordDragEvent("editor-replace-native-load-pending", "threadId=" + Environment.CurrentManagedThreadId);
        await load;
        RecordDragEvent("editor-replace-native-load-completed", "threadId=" + Environment.CurrentManagedThreadId);
        if (Native.DocPointer == previousDocument)
            throw new InvalidOperationException("Replacement did not publish a distinct native document.");
        // Keep this fixture's publication and selection restoration in one UI
        // operation. A subsequent dispatch can wait behind XAML's synchronous
        // COM release while the external receiver intentionally holds Drop.
        if (selection is { } restored) Native.SetSelection(restored.Caret, restored.Anchor);
    }

    private static void CreateEditor()
    {
        _editor = new TextEditorCore
        {
            Width = 520, Height = 340, FontSize = 18, DisplayLineNumbers = false,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top,
            RequestedTheme = ElementTheme.Light
        };
        _view = (WinUIEditor.EditorBaseControl)((Grid)_editor.Content).Children[0];
        _scrollStarts = 0;
        _view.ScrollStarted += (_, _) => ++_scrollStarts;
        AutomationProperties.SetAutomationId(_view, "InteractiveEditor");
        AutomationProperties.SetName(_view, "Isolated editor automation test");
        _view.PreviewKeyDown += (_, args) =>
        {
            // Consume a selection shortcut immediately before a queued text
            // key. Changing selection during that text key would cancel it.
            if (args.Key != Windows.System.VirtualKey.F15 || _selectionOnKey is not { } selection) return;
            _selectionOnKey = null;
            Native.SetSelection(selection.Caret, selection.Anchor);
            args.Handled = true;
        };
        _panel.Children.Add(_editor);
        _disposed = false;
    }

    private static UIElement _dragImage;

    private static void ObserveDragSource()
    {
        var image = FindImageTarget(_view) ?? throw new InvalidOperationException("Editor drag surface was not found.");
        if (image == _dragImage) return;
        _dragImage = image;
        var observedView = _view;
        image.AddHandler(UIElement.DropEvent, new DragEventHandler((_, _) =>
        {
            if (_view != observedView) return;
            ++_dropStarted;
            RecordDragEvent("editor-drop-entered", "dropStarted=" + _dropStarted);
        }), true);
        image.DragStarting += (_, _) =>
        {
            if (_view != observedView) return;
            ++_dragStarted;
            RecordDragEvent("editor-drag-started", "dragStarted=" + _dragStarted);
        };
        image.DropCompleted += (_, args) =>
        {
            if (_view != observedView) return;
            ++_dragCompleted;
            _dragResult = (int)args.DropResult;
            RecordDragEvent("editor-drag-completed", "dragResult=" + _dragResult);
        };
    }

    private static UIElement FindImageTarget(DependencyObject root) => FindElement(root, "ImageTarget");

    private static FrameworkElement FindElement(DependencyObject root, string name)
    {
        if (root is FrameworkElement element && element.Name == name) return element;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); ++i)
            if (FindElement(VisualTreeHelper.GetChild(root, i), name) is { } result) return result;
        return null;
    }

    private static void RecordDragEvent(string name, string value)
    {
        if (_case != null)
            WriteAtomic(_case + "." + name, "ticks=" + DateTime.UtcNow.Ticks + "\n" + value + "\n");
    }

    private static async Task DetachAsync()
    {
        if (!_panel.Children.Contains(_editor)) return;
        var unloaded = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        void OnUnloaded(object sender, RoutedEventArgs args) => unloaded.TrySetResult(true);
        _view.Unloaded += OnUnloaded;
        try
        {
            _panel.Children.Remove(_editor);
            _panel.UpdateLayout();
            await Task.WhenAny(unloaded.Task, Task.Delay(5000));
            if (!unloaded.Task.IsCompleted || _view.IsLoaded) throw new InvalidOperationException("Editor did not unload.");
        }
        finally { _view.Unloaded -= OnUnloaded; }
    }

    private static async Task AttachAsync()
    {
        if (_panel.Children.Contains(_editor)) return;
        var loaded = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        void OnLoaded(object sender, RoutedEventArgs args) => loaded.TrySetResult(true);
        _view.Loaded += OnLoaded;
        try
        {
            _panel.Children.Add(_editor);
            _panel.UpdateLayout();
            await Task.WhenAny(loaded.Task, Task.Delay(5000));
            if (!loaded.Task.IsCompleted || !_view.IsLoaded) throw new InvalidOperationException("Editor did not reload.");
        }
        finally { _view.Loaded -= OnLoaded; }
    }

    private static double Number(string value) => double.Parse(value, CultureInfo.InvariantCulture);

    private static async Task<string> WorkspaceFontSurfaceGeometryAsync()
    {
        var previous = Window.Current.Content;
        try
        {
            await using var host = await NotepadsIntegrationTests.FeatureTestHost.CreateAsync();
            var core = ((TextEditor)host.Core.GetSelectedReadyEditor()).TestNativeCore;
            var view = (WinUIEditor.EditorBaseControl)((Grid)core.Content).Children[0];
            await EditorTestLayout.AfterPaintAsync(view.Editor, () =>
                view.Editor.StyleSetFore(0, view.Editor.StyleGetFore(0)));
            return FontSurfaceGeometry(view);
        }
        finally { Window.Current.Content = previous; }
    }

    private const string FontRenderingText = "Il1| mnwMW 0123456789\rThe quick brown fox jumps over the lazy dog.\r中文字体清晰度测试：你好，世界。\rABC אבג العربية e\u0301 Ω 😀";

    private static async Task<string> PrepareFontPreferenceAsync(bool clearType, bool dark, bool nativeOnly)
    {
        await ResetAsync(FontRenderingText, false, 660, 240, 15);
        await EditorTestLayout.AfterPaintAsync(Native, () =>
        {
            ThemeSettingsService.SetTheme(dark ? ElementTheme.Dark : ElementTheme.Light);
            ApplicationPreferences.EditorFontRendering = clearType ? FontRenderingMode.ClearType : FontRenderingMode.Grayscale;
            _editor.FontFamily = new FontFamily("Consolas");
            // Force production font/theme application even after a manual-quality fixture.
            _editor.FontSize = 14;
            _editor.Padding = new Thickness(6, 6, 10, 6);
            _editor.DisplayLineHighlighter = false;
            Native.CaretStyle = WinUIEditor.CaretStyle.Invisible;
            _panel.Padding = new Thickness(24);
            ((Grid)Window.Current.Content).Background = new SolidColorBrush(Color.FromArgb(255, 26, 88, 55));
            // Remove only the XAML underlay: the independent pixel oracle must
            // prove that the production preference paints native opaque pixels.
            if (nativeOnly) ((Grid)_editor.Content).Background = null;
            _editor.UpdateLayout();
        });
        var origin = _editor.TransformToVisual(null).TransformPoint(default);
        var window = Window.Current.CoreWindow.Bounds;
        var target = FindImageTarget(_view) ?? throw new InvalidOperationException("Missing text surface.");
        var inset = target.TransformToVisual(_editor).TransformPoint(default);
        var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        // Frame capture uses physical screen coordinates; the inset is a local
        // distance, independent of the app window's position on the desktop.
        return FontSurfaceGeometry(_view) + FormattableString.Invariant(
            $"renderFrameLeft={(origin.X + window.X) * scale}\nrenderFrameTop={(origin.Y + window.Y) * scale}\nrenderFrameWidth={_editor.ActualWidth * scale}\nrenderFrameHeight={_editor.ActualHeight * scale}\nrenderFrameInsetTop={inset.Y * scale}\n");
    }

    private static async Task<string> PrepareFontRenderingAsync(int quality, bool dark, bool opaque, double placement, double size)
    {
        if (quality is < 0 or > 3 || !double.IsFinite(placement) || placement is < 0 or > 1)
            throw new ArgumentOutOfRangeException(nameof(quality));
        await ResetAsync(FontRenderingText, false, 660, 240, size);
        await EditorTestLayout.AfterPaintAsync(Native, () =>
        {
            _panel.Padding = new Thickness(24 + placement);
            ((Grid)Window.Current.Content).Background = new SolidColorBrush(dark ? Color.FromArgb(255, 32, 32, 32) : Colors.White);
            _editor.FontFamily = new FontFamily("Consolas");
            _editor.DisplayLineHighlighter = false;
            _view.SetBackgroundColor(dark ? Color.FromArgb(opaque ? (byte)255 : (byte)0, 32, 32, 32) :
                Color.FromArgb(opaque ? (byte)255 : (byte)0, 255, 255, 255));
            Native.StyleSetFore((int)WinUIEditor.StylesCommon.Default, dark ? 0xe0e0e0 : 0);
            Native.StyleClearAll();
            Native.FontQuality = (WinUIEditor.FontQuality)quality;
            Native.CaretStyle = WinUIEditor.CaretStyle.Invisible;
            _editor.UpdateLayout();
        });
        return FontSurfaceGeometry(_view);
    }

    private static string FontSurfaceGeometry(WinUIEditor.EditorBaseControl view)
    {
        var target = FindImageTarget(view) as FrameworkElement ?? throw new InvalidOperationException("Missing text surface.");
        var origin = target.TransformToVisual(null).TransformPoint(default);
        double scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
        return FormattableString.Invariant($"renderOriginX={origin.X * scale}\nrenderOriginY={origin.Y * scale}\nrenderWidth={target.ActualWidth * scale}\nrenderHeight={target.ActualHeight * scale}\nrenderQuality={(int)view.Editor.FontQuality}\nrenderLayoutRounding={target.UseLayoutRounding}\n");
    }

    private static async Task ResetAsync(string text, bool wrap, double width, double height, double fontSize)
    {
        if (width < 120 || width > 700 || height < 100 || height > 440 || fontSize < 10 || fontSize > 40)
            throw new ArgumentOutOfRangeException(nameof(width), "Invalid fixture dimensions.");
        await DiscardPreparedReplacementAsync();
        _selectionOnKey = null;
        if (_disposed)
        {
            _panel.Children.Remove(_editor);
            CreateEditor();
        }
        await EditorTestLayout.AfterPaintAsync(Native, () =>
        {
            if (!_panel.Children.Contains(_editor)) _panel.Children.Add(_editor);
            _editor.IsEnabled = true;
            Native.ReadOnly = false;
            Native.DragDropEnabled = true;
            Native.BufferedDraw = false;
            _editor.Width = width;
            _editor.Height = height;
            _editor.Padding = new Thickness(0);
            _editor.FontSize = fontSize;
            _editor.RightToLeftReadingOrder = false;
            _editor.DisplayLineNumbers = false;
            _editor.DisplayLineHighlighter = true;
            Native.CaretStyle = WinUIEditor.CaretStyle.Line;
            Native.CaretPeriod = 500;
            Native.Overtype = false;
            Native.SelectionMode = WinUIEditor.SelectionMode.Stream;
            Native.MoveExtendsSelection = false;
            _editor.SetText(text);
            Native.WrapMode = wrap ? WinUIEditor.Wrap.Word : WinUIEditor.Wrap.None;
            Native.SetEmptySelection(0);
            Native.EmptyUndoBuffer();
            Native.FirstVisibleLine = 0;
            Native.XOffset = 0;
            _editor.UpdateLayout();
            _editor.Focus(FocusState.Programmatic);
        });
        ObserveDragSource();
    }

    private static string Snapshot()
    {
        // The native pixel Y positions are independent of TextRangeProvider.
        // The external client derives visual-line boundaries from these samples.
        var samples = new List<string>();
        for (long position = 0; ; position = Native.PositionAfter(position))
        {
            samples.Add(position.ToString(CultureInfo.InvariantCulture) + ":" +
                Native.PointYFromPosition(position).ToString(CultureInfo.InvariantCulture));
            if (position == Native.Length) break;
        }
        return "text64=" + Convert.ToBase64String(Encoding.UTF8.GetBytes(_editor.GetText())) +
            "\nanchor=" + Native.Anchor + "\ncaret=" + Native.CurrentPos +
            "\nlength=" + Native.Length + "\nfirstVisible=" + Native.FirstVisibleLine +
            "\nverticalOffset=" + _view.DisplayVerticalOffset.ToString(CultureInfo.InvariantCulture) +
            "\nscrollAnimating=" + _view.IsScrollAnimating +
            "\nscrollStarts=" + _scrollStarts +
            "\nscale=" + DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel.ToString(CultureInfo.InvariantCulture) +
            "\nlinesOnScreen=" + Native.LinesOnScreen + "\nlineHeight=" + Native.TextHeight(0) +
            "\ndisplayRows=" + Native.VisibleFromDocLine(Native.LineCount) + "\ndocumentLines=" + Native.LineCount +
            "\nfirstLineRows=" + Native.WrapCount(0) +
            "\nenabled=" + _editor.IsEnabled + "\nloaded=" + _view.IsLoaded + "\nreadOnly=" + Native.ReadOnly +
            "\ndragDropEnabled=" + Native.DragDropEnabled + "\ncanUndo=" + Native.CanUndo() +
            "\ncanRedo=" + Native.CanRedo() + "\nundoDepth=" + Native.UndoSequence +
            "\nrtl=" + _editor.RightToLeftReadingOrder + "\nrevision=" + Native.DocumentRevision +
            "\novertype=" + Native.Overtype + "\nnativeFocus=" + Native.Focus +
            "\nopenPopups=" + VisualTreeHelper.GetOpenPopups(Window.Current).Count +
            "\nisComposing=" + _view.IsComposing +
            "\nrectangleCaret=" + Native.RectangularSelectionCaret + "\nxOffset=" + Native.XOffset +
            "\ndropStarted=" + _dropStarted + "\ndragStarted=" + _dragStarted +
            "\ndragCompleted=" + _dragCompleted + "\ndragResult=" + _dragResult +
            "\npositionY=" + string.Join(',', samples) + "\n";
    }

    private static void WriteAtomic(string name, string value)
    {
        var path = Path.Combine(_channel, name);
        File.WriteAllText(path + ".tmp", value, new UTF8Encoding(false));
        File.Move(path + ".tmp", path, true);
    }
}
