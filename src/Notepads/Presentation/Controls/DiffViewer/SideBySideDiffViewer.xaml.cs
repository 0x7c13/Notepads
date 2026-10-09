// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Globalization;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Documents.Text;
using Notepads.Presentation.Controls.TextEditor;
using Notepads.Presentation.Theming;
using Windows.ApplicationModel.Resources;
using Windows.System;
using Windows.UI.Core;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;

namespace Notepads.Presentation.Controls.DiffViewer;

public sealed partial class SideBySideDiffViewer : UserControl, IDisposable
{
    private readonly ResourceLoader _resources = ResourceLoader.GetForCurrentView();
    private long[] _changeRows = [];
    private long[] _changeRowCounts = [];
    private uint _addedLines;
    private uint _deletedLines;
    private bool _coarse;
    private bool _synchronizing;
    private bool _disposed;
    private int _currentHunk = -1;
    internal bool HasChanges => _changeRows.Length != 0;
    public event EventHandler CloseRequested;
    public event EventHandler<double> ZoomChanged;

    public SideBySideDiffViewer()
    {
        InitializeComponent();
        OldEditor.ConfigureDiffPreview();
        NewEditor.ConfigureDiffPreview();
        OldEditor.ViewportChanged += OnViewportChanged;
        NewEditor.ViewportChanged += OnViewportChanged;
        OldEditor.FontZoomFactorChanged += OnZoomChanged;
        NewEditor.FontZoomFactorChanged += OnZoomChanged;
        OldEditor.AppearanceChanged += OnAppearanceChanged;
        NewEditor.AppearanceChanged += OnAppearanceChanged;
        OldEditor.CopyTextToWindowsClipboardRequested += OnCopyRequested;
        NewEditor.CopyTextToWindowsClipboardRequested += OnCopyRequested;
        OldEditor.ContextFlyout = CreateCopyFlyout(OldEditor);
        NewEditor.ContextFlyout = CreateCopyFlyout(NewEditor);
        PreviewKeyDown += OnPreviewKeyDown;
        AutomationProperties.SetName(OldEditor, _resources.GetString("DiffViewer_OldDocument"));
        AutomationProperties.SetName(NewEditor, _resources.GetString("DiffViewer_NewDocument"));
        SetButtonLabel(CloseButton, "DiffViewer_Close");
        SetButtonLabel(PreviousButton, "DiffViewer_Previous");
        SetButtonLabel(NextButton, "DiffViewer_Next");
        SetSummary(_resources.GetString("DiffViewer_Loading"));
    }

    private void SetButtonLabel(Button button, string key)
    {
        var text = _resources.GetString(key);
        AutomationProperties.SetName(button, text);
        ToolTipService.SetToolTip(button, text);
    }

    private MenuFlyout CreateCopyFlyout(TextEditorCore editor)
    {
        var flyout = new MenuFlyout();
        var copy = new MenuFlyoutItem { Text = _resources.GetString("TextEditor_ContextFlyout_CopyButtonDisplayText") };
        copy.Click += (_, _) => editor.CopyDiffSelection();
        var selectAll = new MenuFlyoutItem { Text = _resources.GetString("TextEditor_ContextFlyout_SelectAllButtonDisplayText") };
        selectAll.Click += (_, _) => editor.SelectDiffDocument();
        flyout.Items.Add(copy);
        flyout.Items.Add(selectAll);
        return flyout;
    }

    internal async Task<bool> PrepareAsync(DocumentBaseline baseline, TextEditorCore source,
        DocumentLanguage language, string fileName, CancellationToken cancellationToken)
    {
        long savedLines;
        using (var savedText = await baseline.OpenReadStreamAsync(cancellationToken))
            savedLines = await CanonicalLineCounter.CountAsync(savedText, cancellationToken);
        cancellationToken.ThrowIfCancellationRequested();
        if (!source.CanPrepareDiff(baseline.ByteLength, savedLines)) return false;
        OldEditor.SetSyntaxLanguage(language, fileName);
        NewEditor.SetSyntaxLanguage(language, fileName);
        OldEditor.SetFontZoomFactor(source.GetFontZoomFactor());
        NewEditor.SetFontZoomFactor(source.GetFontZoomFactor());
        await source.LoadDiffSnapshotsAsync(baseline, OldEditor, NewEditor, cancellationToken);
        cancellationToken.ThrowIfCancellationRequested();
        OldEditor.MakeDiffReadOnly();
        NewEditor.MakeDiffReadOnly();
        var result = await OldEditor.CompareAsync(NewEditor, cancellationToken);
        cancellationToken.ThrowIfCancellationRequested();
        if (!result.IsAvailable) return false;
        _changeRows = result.ChangeRows;
        _changeRowCounts = result.ChangeRowCounts;
        _addedLines = result.AddedLines;
        _deletedLines = result.DeletedLines;
        _coarse = result.IsCoarse;
        if (!HasChanges) return true;
        OldEditor.ApplyDiffPresentation(result, oldSide: true);
        NewEditor.ApplyDiffPresentation(result, oldSide: false);
        ApplyPalette();
        OldEditor.IsEnabled = true;
        NewEditor.IsEnabled = true;
        PreviousButton.IsEnabled = NextButton.IsEnabled = true;
        Navigate(1);
        return true;
    }

    private void SetSummary(string message, bool showStatistics = false)
    {
        SummaryMessage.Text = message;
        var visible = showStatistics && (_addedLines != 0 || _deletedLines != 0);
        StatisticsOpen.Text = visible ? " (" : string.Empty;
        AddedCount.Text = visible ? "+" + _addedLines.ToString("N0", CultureInfo.CurrentCulture) : string.Empty;
        StatisticsSeparator.Text = visible ? " " : string.Empty;
        DeletedCount.Text = visible ? "-" + _deletedLines.ToString("N0", CultureInfo.CurrentCulture) : string.Empty;
        StatisticsClose.Text = visible ? ")" : string.Empty;
    }

    private void ApplyPalette()
    {
        if (_disposed) return;
        var palette = DiffColorPalette.ForTheme(ThemeSettingsService.ThemeMode, new AccessibilitySettings().HighContrast);
        OldEditor.ApplyDiffPalette(palette, oldSide: true);
        NewEditor.ApplyDiffPalette(palette, oldSide: false);
    }

    private void OnAppearanceChanged(object sender, EventArgs args) => ApplyPalette();

    private void OnViewportChanged(object sender, EventArgs args)
    {
        if (_disposed || _synchronizing) return;
        var source = (TextEditorCore)sender;
        var target = ReferenceEquals(source, OldEditor) ? NewEditor : OldEditor;
        _synchronizing = true;
        try { target.SetDiffViewport(source.FirstVisibleRow, source.HorizontalScrollOffset); }
        finally { _synchronizing = false; }
    }

    private void OnZoomChanged(object sender, double zoom)
    {
        if (_disposed || _synchronizing) return;
        _synchronizing = true;
        try
        {
            OldEditor.SetFontZoomFactor(zoom);
            NewEditor.SetFontZoomFactor(zoom);
            ZoomChanged?.Invoke(this, zoom);
        }
        finally { _synchronizing = false; }
    }

    private static void OnCopyRequested(object sender, EventArgs args) => ((TextEditorCore)sender).CopyDiffSelection();

    private void Navigate(int direction)
    {
        if (_disposed || _changeRows.Length == 0) return;
        _currentHunk = (_currentHunk + direction + _changeRows.Length) % _changeRows.Length;
        var row = _changeRows[_currentHunk];
        _synchronizing = true;
        try
        {
            OldEditor.HighlightDiffChange(row, _changeRowCounts[_currentHunk]);
            NewEditor.HighlightDiffChange(row, _changeRowCounts[_currentHunk]);
            OldEditor.SetDiffViewport(Math.Max(0, row - 2), 0);
            NewEditor.SetDiffViewport(Math.Max(0, row - 2), 0);
        }
        finally { _synchronizing = false; }
        SetSummary(string.Format(CultureInfo.CurrentCulture,
            _resources.GetString(_coarse ? "DiffViewer_CoarseCount" : "DiffViewer_ChangeCount"), _currentHunk + 1, _changeRows.Length), showStatistics: true);
    }

    internal void FocusEditor()
    {
        if (!_disposed && !NewEditor.Focus(FocusState.Programmatic))
            NewEditor.Loaded += OnEditorLoaded;
    }

    private void OnEditorLoaded(object sender, RoutedEventArgs args)
    {
        NewEditor.Loaded -= OnEditorLoaded;
        if (!_disposed) NewEditor.Focus(FocusState.Programmatic);
    }
    private void CloseButton_Click(object sender, RoutedEventArgs args) => CloseRequested?.Invoke(this, EventArgs.Empty);
    private void PreviousButton_Click(object sender, RoutedEventArgs args) => Navigate(-1);
    private void NextButton_Click(object sender, RoutedEventArgs args) => Navigate(1);

    private void OnPreviewKeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (args.Handled) return;
        var window = CoreWindow.GetForCurrentThread();
        var alt = window.GetKeyState(VirtualKey.Menu).HasFlag(CoreVirtualKeyStates.Down);
        var shift = window.GetKeyState(VirtualKey.Shift).HasFlag(CoreVirtualKeyStates.Down);
        if (args.Key == VirtualKey.F7 || (alt && args.Key is VirtualKey.Up or VirtualKey.Down))
        {
            Navigate(args.Key == VirtualKey.Up || (args.Key == VirtualKey.F7 && shift) ? -1 : 1);
            args.Handled = true;
        }
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        NewEditor.Loaded -= OnEditorLoaded;
        PreviewKeyDown -= OnPreviewKeyDown;
        OldEditor.ViewportChanged -= OnViewportChanged;
        NewEditor.ViewportChanged -= OnViewportChanged;
        OldEditor.FontZoomFactorChanged -= OnZoomChanged;
        NewEditor.FontZoomFactorChanged -= OnZoomChanged;
        OldEditor.AppearanceChanged -= OnAppearanceChanged;
        NewEditor.AppearanceChanged -= OnAppearanceChanged;
        OldEditor.CopyTextToWindowsClipboardRequested -= OnCopyRequested;
        NewEditor.CopyTextToWindowsClipboardRequested -= OnCopyRequested;
        OldEditor.ContextFlyout = null;
        NewEditor.ContextFlyout = null;
        try { OldEditor.Dispose(); }
        finally { NewEditor.Dispose(); }
        _changeRows = [];
        _changeRowCounts = [];
    }
}
