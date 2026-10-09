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
            _editor = new TextEditorCore
            {
                Width = 520,
                Height = 340,
                FontSize = 18,
                DisplayLineNumbers = false,
                HorizontalAlignment = HorizontalAlignment.Left,
                VerticalAlignment = VerticalAlignment.Top,
                RequestedTheme = ElementTheme.Light
            };
            _view = (WinUIEditor.EditorBaseControl)((Grid)_editor.Content).Children[0];
            AutomationProperties.SetAutomationId(_view, "InteractiveEditor");
            AutomationProperties.SetName(_view, "Isolated editor automation test");
            var panel = new StackPanel { Padding = new Thickness(24), Spacing = 12 };
            panel.Children.Add(new TextBlock { Text = "Notepads isolated editor automation test" });
            panel.Children.Add(_editor);
            Window.Current.Content = new Grid
            {
                Background = new SolidColorBrush(Colors.White),
                Children = { panel }
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
            var command = File.ReadAllText(path).Split('|');
            _sequence = next;
            _lastCommand = DateTime.UtcNow;
            try
            {
                await ExecuteAsync(command);
                WriteAtomic($"response-{next:D4}.txt", "OK\n" + Snapshot());
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

    private static async Task ExecuteAsync(string[] command)
    {
        switch (command[0])
        {
            case "reset" when command.Length == 6:
                var text = Encoding.UTF8.GetString(Convert.FromBase64String(command[1]));
                if (Encoding.UTF8.GetByteCount(text) > 128 * 1024) throw new ArgumentException("Fixture text exceeds 128 KiB.");
                await ResetAsync(text, bool.Parse(command[2]), Number(command[3]), Number(command[4]), Number(command[5]));
                break;
            case "select" when command.Length == 3:
                var anchor = long.Parse(command[1], CultureInfo.InvariantCulture);
                var caret = long.Parse(command[2], CultureInfo.InvariantCulture);
                if (anchor < 0 || caret < 0 || anchor > Native.Length || caret > Native.Length)
                    throw new ArgumentOutOfRangeException(nameof(command));
                Native.SetSelection(caret, anchor);
                break;
            case "state" when command.Length == 1:
                break;
            default:
                throw new ArgumentException("Unknown interactive command: " + command[0]);
        }
    }

    private static double Number(string value) => double.Parse(value, CultureInfo.InvariantCulture);

    private static async Task ResetAsync(string text, bool wrap, double width, double height, double fontSize)
    {
        if (width < 120 || width > 700 || height < 100 || height > 440 || fontSize < 10 || fontSize > 40)
            throw new ArgumentOutOfRangeException(nameof(width), "Invalid fixture dimensions.");
        await EditorTestLayout.AfterPaintAsync(Native, () =>
        {
            Native.ReadOnly = false;
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
            "\npositionY=" + string.Join(',', samples) + "\n";
    }

    private static void WriteAtomic(string name, string value)
    {
        var path = Path.Combine(_channel, name);
        File.WriteAllText(path + ".tmp", value, new UTF8Encoding(false));
        File.Move(path + ".tmp", path, true);
    }
}
