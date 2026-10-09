// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using Notepads.Features.Preferences;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Windows.Graphics.Display;
using Windows.Storage;
using Windows.UI;
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
    private static DispatcherTimer _timer;
    private static string _channel;
    private static StackPanel _panel;
    private static bool _disposed;
    private static string _case;
    private static int _dropStarted;
    private static int _dragStarted;
    private static int _dragCompleted;
    private static int _dragResult;
    private static int _sequence;
    private static bool _busy;
    private static DateTime _lastCommand;

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
            ThemeSettingsService.Initialize();
            ApplicationPreferences.Initialize();
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
            _timer = new DispatcherTimer { Interval = TimeSpan.FromMilliseconds(25) };
            _timer.Tick += async (_, _) => await PollAsync();
            _timer.Start();
            WriteAtomic("ready.txt", "READY\n" + Snapshot());
        }
        catch (Exception ex)
        {
            WriteAtomic("fatal.txt", ex.ToString());
            throw;
        }
    }

    private static async Task PollAsync()
    {
        if (_busy) return;
        _busy = true;
        try
        {
            if (_sequence >= 1000 || DateTime.UtcNow - _lastCommand > TimeSpan.FromMinutes(10))
            {
                _timer.Stop();
                Application.Current.Exit();
                return;
            }
            var next = _sequence + 1;
            var path = Path.Combine(_channel, $"command-{next:D4}.txt");
            if (!File.Exists(path)) return;
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
                WriteAtomic($"response-{next:D4}.txt", "OK\n" + Snapshot() + extra);
            }
            catch (Exception ex)
            {
                WriteAtomic($"response-{next:D4}.txt", "ERROR\n" + ex);
            }
        }
        catch (Exception ex)
        {
            WriteAtomic("fatal.txt", ex.ToString());
            _timer.Stop();
        }
        finally { _busy = false; }
    }

    private static async Task<string> ExecuteAsync(string[] command)
    {
        switch (command[0])
        {
            case "reset" when command.Length == 6:
                var text = DecodeText(command[1]);
                await ResetAsync(text, bool.Parse(command[2]), Number(command[3]), Number(command[4]), Number(command[5]));
                break;
            case "begin-case" when command.Length == 2:
                if (!Guid.TryParseExact(command[1], "N", out var caseId)) throw new ArgumentException("Invalid drag case ID.");
                _case = caseId.ToString("N");
                _dropStarted = _dragStarted = _dragCompleted = _dragResult = 0;
                break;
            case "select" when command.Length == 3:
                var anchor = long.Parse(command[1], CultureInfo.InvariantCulture);
                var caret = long.Parse(command[2], CultureInfo.InvariantCulture);
                if (anchor < 0 || caret < 0 || anchor > Native.Length || caret > Native.Length)
                    throw new ArgumentOutOfRangeException(nameof(command));
                Native.SetSelection(caret, anchor);
                break;
            case "geometry" when command.Length == 2:
                var position = long.Parse(command[1], CultureInfo.InvariantCulture);
                if (!_view.IsLoaded || position < 0 || position > Native.Length) throw new ArgumentOutOfRangeException(nameof(position));
                var scale = DisplayInformation.GetForCurrentView().RawPixelsPerViewPixel;
                var point = _view.TransformToVisual(null).TransformPoint(new Windows.Foundation.Point(
                    Native.PointXFromPosition(position) / scale, Native.PointYFromPosition(position) / scale));
                var bounds = Window.Current.CoreWindow.Bounds;
                return "pointX=" + ((point.X + bounds.X) * scale).ToString(CultureInfo.InvariantCulture) +
                    "\npointY=" + ((point.Y + bounds.Y) * scale).ToString(CultureInfo.InvariantCulture) + "\n";
            case "undo" when command.Length == 1:
                Native.Undo();
                break;
            case "insert" when command.Length == 2:
                _editor.TypeText(DecodeText(command[1]));
                break;
            case "set" when command.Length == 2:
                _editor.SetText(DecodeText(command[1]));
                break;
            case "replace" when command.Length == 2:
                await _editor.LoadTextAsync(DecodeText(command[1]));
                break;
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
                _editor.Dispose();
                _disposed = true;
                break;
            case "state" when command.Length == 1:
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

    private static void CreateEditor()
    {
        _editor = new TextEditorCore
        {
            Width = 520, Height = 340, FontSize = 18, DisplayLineNumbers = false,
            HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top,
            RequestedTheme = ElementTheme.Light
        };
        _view = (WinUIEditor.EditorBaseControl)((Grid)_editor.Content).Children[0];
        AutomationProperties.SetAutomationId(_view, "InteractiveEditor");
        AutomationProperties.SetName(_view, "Isolated editor automation test");
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

    private static UIElement FindImageTarget(DependencyObject root)
    {
        if (root is FrameworkElement { Name: "ImageTarget" } element) return element;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); ++i)
            if (FindImageTarget(VisualTreeHelper.GetChild(root, i)) is { } result) return result;
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

    private static async Task ResetAsync(string text, bool wrap, double width, double height, double fontSize)
    {
        if (width < 120 || width > 700 || height < 100 || height > 440 || fontSize < 10 || fontSize > 40)
            throw new ArgumentOutOfRangeException(nameof(width), "Invalid fixture dimensions.");
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
            _editor.Width = width;
            _editor.Height = height;
            _editor.FontSize = fontSize;
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
            "\nlinesOnScreen=" + Native.LinesOnScreen + "\nlineHeight=" + Native.TextHeight(0) +
            "\nenabled=" + _editor.IsEnabled + "\nloaded=" + _view.IsLoaded + "\nreadOnly=" + Native.ReadOnly +
            "\ndragDropEnabled=" + Native.DragDropEnabled + "\ncanUndo=" + Native.CanUndo() +
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
