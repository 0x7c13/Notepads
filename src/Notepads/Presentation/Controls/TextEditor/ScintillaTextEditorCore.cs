// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.IO;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Features.Documents.Contracts;
using Notepads.Features.Documents.FileTypes;
using Notepads.Features.Documents.Storage;
using Notepads.Features.Documents.Text;
using Notepads.Features.Preferences;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Infrastructure.Threading;
using Notepads.Presentation.Helpers;
using Notepads.Presentation.Input;
using Notepads.Presentation.Theming;
using Windows.ApplicationModel.DataTransfer;
using Windows.Foundation;
using Windows.System;
using Windows.UI;
using Windows.UI.Text;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

/// <summary>
/// Scintilla owns text, line indices and undo history. Full text is only materialized
/// for explicit snapshots/exports; selection and line operations use native ranges.
/// The external selection contract remains UTF-16 with one CR per line ending so
/// existing session files remain readable. Internally positions are UTF-8 bytes.
/// </summary>
public sealed partial class TextEditorCore : UserControl, IDisposable
{
    private readonly EditorBaseControl _view;
    private readonly Grid _surface;
    private readonly Border _lineNumberReveal;
    private readonly LinearGradientBrush _lineNumberGlow;
    private readonly GradientStop _lineNumberGlowStart;
    private readonly GradientStop _lineNumberGlowCenter;
    private readonly GradientStop _lineNumberGlowEnd;
    private EditorColorPalette _palette;
    private double _dpiScale = 1;
    private readonly AccessibilitySettings _accessibility = new();
    private Editor Native => _view.Editor;
    private readonly KeyboardCommandHandler _keyboardCommandHandler;
    private bool _disposed;
    private bool _settingText;
    private bool _displayLineNumbers;
    private bool _displayLineHighlighter;
    private TextWrapping _textWrapping;
    private double _fontSize;
    private double _fontZoomFactor = 100;
    private const double _minimumZoomFactor = 10;
    private const double _maximumZoomFactor = 500;
    private const ModificationFlags TextModifications = ModificationFlags.InsertText | ModificationFlags.DeleteText;
    private double _horizontalOffset;
    private double _verticalOffset;
    private bool _restoreScroll;
    private int _lineNumberDigits;

    public event EventHandler TextChanging;
    public event RoutedEventHandler TextChanged;
    public event RoutedEventHandler SelectionChanged;
    public event EventHandler ModificationStateChanged;
    public event EventHandler CopyTextToWindowsClipboardRequested;
    public event EventHandler CutSelectedTextToWindowsClipboardRequested;
    public event EventHandler<TextWrapping> TextWrappingChanged;
    public event EventHandler<double> FontSizeChanged;
    public event EventHandler<double> FontZoomFactorChanged;

    private bool CanEdit => !_disposed && IsEnabled && !Native.ReadOnly;

    public bool HasSelection => !Native.SelectionEmpty;
    public bool IsDocumentEmpty => Native.Length == 0;
    public long DocumentLength => Native.Length;
    public bool IsDocumentModified => Native.Modify;
    public bool CanUndo => Native.CanUndo();
    public bool CanRedo => Native.CanRedo();
    public long ContentVersion { get; private set; }
    internal ulong DocumentSequence => Native.DocumentSequence;

    public TextWrapping TextWrapping
    {
        get => _textWrapping;
        set
        {
            if (_diffPreview) value = TextWrapping.NoWrap;
            _textWrapping = value;
            Native.WrapMode = value == TextWrapping.NoWrap ? Wrap.None : Wrap.Word;
            TextWrappingChanged?.Invoke(this, value);
        }
    }

    public new double FontSize
    {
        get => _fontSize;
        set
        {
            if (double.IsNaN(value) || double.IsInfinity(value)) return;
            var fontSize = Math.Max(_minimumZoomFactor * ApplicationPreferences.EditorFontSize / 100,
                Math.Min(_maximumZoomFactor * ApplicationPreferences.EditorFontSize / 100, value));
            if (fontSize != _fontSize)
            {
                _fontSize = fontSize;
                base.FontSize = fontSize;
                ApplyFont();
            }
            FontSizeChanged?.Invoke(this, _fontSize);
            var zoom = Math.Round(_fontSize * 100 / ApplicationPreferences.EditorFontSize);
            if (Math.Abs(zoom - _fontZoomFactor) >= 1)
            {
                _fontZoomFactor = zoom;
                FontZoomFactorChanged?.Invoke(this, zoom);
            }
        }
    }

    public bool DisplayLineNumbers
    {
        get => _displayLineNumbers;
        set { _displayLineNumbers = _diffPreview || value; UpdateLineNumberMargin(); }
    }

    public bool DisplayLineHighlighter
    {
        get => _displayLineHighlighter;
        set { _displayLineHighlighter = !_diffPreview && value; ApplyCaretLineAppearance(); }
    }

    public TextEditorCore()
    {
        // XAML RTL mirrors the native bitmap, including glyphs and line numbers.
        // Native BiDi handles text direction while chrome stays unmirrored.
        FlowDirection = FlowDirection.LeftToRight;
        _view = new EditorBaseControl();
        _view.MouseWheelZoomEnabled = false;
        _lineNumberGlowStart = new GradientStop { Color = Colors.Transparent };
        _lineNumberGlowCenter = new GradientStop { Color = Colors.Transparent };
        _lineNumberGlowEnd = new GradientStop { Color = Colors.Transparent };
        _lineNumberGlow = new LinearGradientBrush
        {
            StartPoint = new Point(0, 0),
            EndPoint = new Point(0, 1),
            GradientStops = { _lineNumberGlowStart, _lineNumberGlowCenter, _lineNumberGlowEnd }
        };
        _lineNumberReveal = new Border
        {
            HorizontalAlignment = HorizontalAlignment.Left,
            BorderThickness = new Thickness(0),
            BorderBrush = _lineNumberGlow,
            IsHitTestVisible = false
        };
        _surface = new Grid();
        _surface.Children.Add(_view);
        _surface.Children.Add(_lineNumberReveal);
        Content = _surface;
        RegisterPropertyChangedCallback(PaddingProperty, (_, __) => ApplyEditorPadding());
        // The native image target handles context requests before they reach this
        // UserControl. Pass the app flyout to the native control explicitly.
        RegisterPropertyChangedCallback(ContextFlyoutProperty, (_, __) => _view.ContextFlyout = ContextFlyout);
        _view.DpiChanged += OnNativeDpiChanged;
        Native.CodePage = (int)EditorConstants.ScCpUtf8;
        // DirectWrite supports mixed RTL/LTR text with an LTR paragraph base.
        // This does not request the unimplemented default-RTL paragraph mode.
        Native.Bidirectional = Bidirectional.L2r;
        Native.SelectionLayer = Layer.UnderText;
        // One CR is also the old RichEdit/session representation. Import/export
        // converts CRLF/LF at the boundary, never on the typing path.
        Native.EOLMode = EndOfLine.Cr;
        Native.ModEventMask = TextModifications;
        Native.TabWidth = 4;
        Native.MultiPaste = MultiPaste.Each;
        // Scintilla defaults to a 2000-pixel document width, which displays a
        // horizontal scrollbar even for an empty document.
        Native.ScrollWidth = 1;
        Native.ScrollWidthTracking = true;
        Native.SetMarginTypeN(0, MarginType.Number);
        Native.SetMarginWidthN(1, 0);
        Native.SetMarginWidthN(2, 0);
        FontFamily = new FontFamily(ApplicationPreferences.EditorFontFamily);
        FontStyle = ApplicationPreferences.EditorFontStyle.ToFontStyle();
        FontWeight = ApplicationPreferences.EditorFontWeight.ToFontWeight();
        // Sets the font and the theme palette; must precede DisplayLineHighlighter.
        FontSize = ApplicationPreferences.EditorFontSize;
        TextWrapping = ApplicationPreferences.EditorDefaultWordWrap.ToTextWrapping();
        DisplayLineNumbers = ApplicationPreferences.EditorDisplayLineNumbers;
        DisplayLineHighlighter = ApplicationPreferences.EditorDisplayLineHighlighter;
        Native.Modified += OnNativeModified;
        Native.CharAdded += OnNativeCharAdded;
        Native.UpdateUI += OnNativeUpdateUI;
        Native.SavePointReached += OnSavePointReached;
        Native.SavePointLeft += OnSavePointLeft;
        _keyboardCommandHandler = GetKeyboardCommandHandler();
        // Handle shortcuts before the native control consumes them.
        _view.PreviewKeyDown += OnEditorPreviewKeyDown;
        _view.PointerWheelChanged += OnPointerWheelChanged;
        AddHandler(PointerMovedEvent, new PointerEventHandler(OnEditorPointerMoved), true);
        AddHandler(PointerExitedEvent, new PointerEventHandler(OnEditorPointerExited), true);
        Loaded += OnLoaded;
        ActualThemeChanged += OnActualThemeChanged;
        ThemeSettingsService.OnThemeChanged += OnAppThemeChanged;
        _accessibility.HighContrastChanged += OnHighContrastChanged;
        HookExternalEvents();
    }

    // Loaded runs on every tab switch. Font and theme apply when they change;
    // restyling here would re-wrap the whole document each time.
    private void OnLoaded(object sender, RoutedEventArgs args)
    {
        ApplyEditorPadding();
        if (_restoreScroll) RestoreScroll();
    }

    private void ApplyEditorPadding()
    {
        // Keep the old top text inset. The native template gives the horizontal
        // scrollbar its own row, so no space is needed below the control.
        _surface.Margin = new Thickness(0, Padding.Top, 0, Padding.Bottom);
    }

    private void OnActualThemeChanged(FrameworkElement sender, object args) => ApplyTheme();

    private async void OnAppThemeChanged(object sender, ElementTheme theme)
    {
        await Dispatcher.CallOnUIThreadAsync(() =>
        {
            if (!_disposed) ApplyTheme();
        });
    }

    private void OnNativeDpiChanged(object sender, double scale)
    {
        _dpiScale = scale;
        UpdateLineNumberMargin();
    }

    private async void OnHighContrastChanged(AccessibilitySettings sender, object args) =>
        await Dispatcher.CallOnUIThreadAsync(() => ApplyTheme());

    private static int ToScintillaColor(Color color) => color.R | color.G << 8 | color.B << 16;

    private void ApplyTheme(bool force = false)
    {
        var palette = EditorColorPalette.ForTheme(ThemeSettingsService.ThemeMode, _accessibility.HighContrast);
        // Restyling re-wraps the whole document. Light and dark palettes are shared,
        // so a repeated theme notification skips it; high contrast always reapplies.
        if (!force && palette == _palette) return;
        _palette = palette;
        var foreground = ToScintillaColor(palette[EditorColorRole.Foreground]);
        var background = palette[EditorColorRole.Background];
        Native.StyleSetFore((int)StylesCommon.Default, foreground);
        // The main page already owns the acrylic/solid backdrop brush.
        // Keep the native text surface and margins transparent above it.
        _view.SetBackgroundColor(background);
        Native.StyleClearAll();
        // StyleClearAll assigns the platform chrome color to line numbers.
        // Restore alpha after copying the default style.
        _view.SetBackgroundColor(background);
        ApplySyntaxColors();
        Native.StyleSetFore((int)StylesCommon.LineNumber, ToScintillaColor(palette[EditorColorRole.LineNumber]));
        Native.FontQuality = WinUIEditor.FontQuality.QualityAntialiased;
        Native.CaretFore = foreground;
        ApplyCaretLineAppearance();
        HideLineNumberGlow();
        _lineNumberReveal.BorderBrush = _accessibility.HighContrast ? null : _lineNumberGlow;
        ApplySelectionAppearance();
        UpdateLineNumberMargin();
        AppearanceChanged?.Invoke(this, EventArgs.Empty);
    }

    private void ApplySelectionAppearance()
    {
        var color = _accessibility.HighContrast
            ? (Color)Application.Current.Resources["SystemColorHighlightColor"]
            : ThemeSettingsService.AppAccentColor;
        var background = unchecked((int)(0xff000000u | (uint)ToScintillaColor(color)));
        var foreground = unchecked((int)(0xff000000u | (uint)ToScintillaColor(_palette[EditorColorRole.SelectionText])));
        // Rectangular selection uses an additional range for each other row.
        // Keep all ranges consistent, including when Find holds focus.
        Native.SetElementColour(Element.SelectionBack, background);
        Native.SetElementColour(Element.SelectionAdditionalBack, background);
        Native.SetElementColour(Element.SelectionSecondaryBack, background);
        Native.SetElementColour(Element.SelectionInactiveBack, background);
        Native.SetElementColour(Element.SelectionInactiveAdditionalBack, background);
        Native.SetElementColour(Element.SelectionText, foreground);
        Native.SetElementColour(Element.SelectionAdditionalText, foreground);
        Native.SetElementColour(Element.SelectionSecondaryText, foreground);
        Native.SetElementColour(Element.SelectionInactiveText, foreground);
        Native.SetElementColour(Element.SelectionInactiveAdditionalText, foreground);
    }

    private void ApplyCaretLineAppearance()
    {
        // Scintilla's Base layer forces caret-line colors to full opacity.
        Native.CaretLineLayer = Layer.UnderText;
        var color = _palette[EditorColorRole.CaretLine];
        Native.SetElementColour(Element.CaretLineBack, ToScintillaColor(color) | color.A << 24);
        Native.CaretLineVisible = _displayLineHighlighter && !_accessibility.HighContrast;
    }

    private void ApplyFont()
    {
        if (_view == null) return;
        Native.StyleSetFont((int)StylesCommon.Default, FontFamily.Source);
        Native.StyleSetSizeFractional((int)StylesCommon.Default, (int)Math.Round(_fontSize * 75)); // XAML DIPs -> points
        Native.StyleSetItalic((int)StylesCommon.Default, FontStyle != FontStyle.Normal);
        Native.StyleSetWeight((int)StylesCommon.Default, (WinUIEditor.FontWeight)FontWeight.Weight);
        // StyleClearAll copies the new default font to every style.
        ApplyTheme(force: true);
    }

    private void UpdateLineNumberMargin()
    {
        _lineNumberDigits = LineNumberDigits();
        var width = _displayLineNumbers ? Native.TextWidth((int)StylesCommon.LineNumber, new string('9', _lineNumberDigits)) + (int)Math.Round(12 * _dpiScale) : 0;
        Native.SetMarginWidthN(0, width);
        Native.MarginLeft = (int)Math.Round(6 * _dpiScale);
        _lineNumberReveal.Width = width / _dpiScale;
        _lineNumberReveal.Visibility = _displayLineNumbers ? Visibility.Visible : Visibility.Collapsed;
        if (!_displayLineNumbers) HideLineNumberGlow();
    }

    private int LineNumberDigits() => Math.Max(2, Native.LineCount.ToString().Length);

    private void OnEditorPointerMoved(object sender, PointerRoutedEventArgs args)
    {
        var point = args.GetCurrentPoint(_view).Position;
        UpdateLineNumberGlow(point.X, point.Y);
    }

    private void OnEditorPointerExited(object sender, PointerRoutedEventArgs args) => HideLineNumberGlow();

    private void UpdateLineNumberGlow(double x, double y)
    {
        const double radius = 80;
        var height = _view.ActualHeight;
        var distance = Math.Abs(x - _lineNumberReveal.Width);
        if (!_displayLineNumbers || _accessibility.HighContrast || x < 0 || y < 0 || y >= height || distance >= radius)
        {
            HideLineNumberGlow();
            return;
        }

        // A short vertical gradient follows the pointer near the gutter edge.
        // A constant reveal border would draw a bright line across the whole editor.
        var center = y / height;
        var spread = radius / height;
        _lineNumberGlowStart.Offset = Math.Max(0, center - spread);
        _lineNumberGlowCenter.Offset = center;
        _lineNumberGlowEnd.Offset = Math.Min(1, center + spread);
        var alpha = (byte)Math.Round(150 * (1 - distance / radius));
        var glow = _palette[EditorColorRole.Foreground];
        _lineNumberGlowCenter.Color = Color.FromArgb(alpha, glow.R, glow.G, glow.B);
        if (_lineNumberReveal.BorderThickness.Right == 0)
            _lineNumberReveal.BorderThickness = new Thickness(0, 0, 1, 0);
    }

    private void HideLineNumberGlow() => _lineNumberReveal.BorderThickness = new Thickness(0);

    private void OnNativeModified(Editor sender, ModifiedEventArgs args)
    {
        if ((args.ModificationType & (int)TextModifications) == 0) return;
        ContentVersion++;
        if (_settingText) return;
        UpdateSyntaxStatus();
        if (args.Position <= DocumentLanguages.DetectionSampleBytes) QueueLanguageDetection();
        if (args.LinesAdded != 0 && LineNumberDigits() != _lineNumberDigits)
            UpdateLineNumberMargin();
        TextChanging?.Invoke(this, EventArgs.Empty);
        TextChanged?.Invoke(this, new RoutedEventArgs());
    }

    private void OnNativeUpdateUI(Editor sender, UpdateUIEventArgs args)
    {
        UpdateSyntaxStatus();
        // Scintilla reports a content update without a separate selection flag
        // when typing replaces a selection. The status bar still needs the
        // final caret/selection after that edit.
        if ((args.Updated & (int)(Update.Content | Update.Selection)) != 0) SelectionChanged?.Invoke(this, new RoutedEventArgs());
        if ((args.Updated & (int)(Update.VScroll | Update.HScroll)) != 0) ViewportChanged?.Invoke(this, EventArgs.Empty);
    }

    private void OnSavePointReached(Editor sender, SavePointReachedEventArgs args) => ModificationStateChanged?.Invoke(this, EventArgs.Empty);
    private void OnSavePointLeft(Editor sender, SavePointLeftEventArgs args) => ModificationStateChanged?.Invoke(this, EventArgs.Empty);

    public void SetText(string text)
    {
        _settingText = true;
        try
        {
            // AddText is counted, unlike SetText, and preserves embedded NULs.
            var normalized = LineEndingUtility.ApplyLineEnding(text ?? string.Empty, LineEnding.Cr);
            Native.UndoCollection = false;
            Native.ClearAll();
            Native.Allocate(Encoding.UTF8.GetByteCount(normalized));
            Native.AddText(Encoding.UTF8.GetByteCount(normalized), normalized);
            Native.SetSel(0, 0);
            Native.EmptyUndoBuffer();
            Native.SetSavePoint();
            ResetDetectedIndentation();
            UpdateLineNumberMargin();
        }
        finally { Native.UndoCollection = true; _settingText = false; }
        TextChanged?.Invoke(this, new RoutedEventArgs());
    }

    internal Task LoadBaselineAsync(DocumentBaseline baseline, bool preserveUndo,
        CancellationToken cancellationToken = default)
    {
        if (baseline == null) throw new ArgumentNullException(nameof(baseline));
        return LoadNativeDocumentAsync(async () =>
        {
            using (var stream = await baseline.OpenReadStreamAsync(cancellationToken))
            using (var input = stream.AsInputStream())
            {
                await Native.LoadUtf8Async(input, checked((ulong)baseline.ByteLength), preserveUndo)
                    .AsCompletionTask(cancellationToken);
            }
        });
    }

    internal Task RestoreBaselineAsync(DocumentBaseline baseline, EditorJournalCheckpoint checkpoint,
        CancellationToken cancellationToken)
    {
        return LoadNativeDocumentAsync(async () =>
        {
            using (var stream = await baseline.OpenReadStreamAsync(cancellationToken))
            using (var input = stream.AsInputStream())
            {
                await Native.RestoreUtf8Async(input, checked((ulong)baseline.ByteLength), checkpoint)
                    .AsCompletionTask(cancellationToken);
            }
        });
    }

    private async Task LoadNativeDocumentAsync(Func<Task> loadAsync)
    {
        _settingText = true;
        var enabled = IsEnabled;
        IsEnabled = false;
        try
        {
            await loadAsync();
            if (_disposed) return;
            InstallSyntaxProfile();
            Native.SetSel(0, 0);
            ContentVersion++;
            ResetDetectedIndentation();
            UpdateLineNumberMargin();
        }
        finally
        {
            if (!_disposed) IsEnabled = enabled;
            _settingText = false;
        }
        if (!_disposed) TextChanged?.Invoke(this, new RoutedEventArgs());
    }

    internal EditorUtf8Reader AcquireDocumentReader() => Native.AcquireUtf8Reader();

    internal void StartJournal(DocumentJournal journal) => Native.StartJournal(journal.File.Path, Native.DocumentSequence);

    internal EditorJournalCheckpoint AcquireJournalCheckpoint() => Native.AcquireJournalCheckpoint();

    internal Task StopJournalAsync() => Native.StopJournalAsync().AsTask();

    internal bool IsJournalFaulted => Native.JournalFaulted;

    internal Task StartJournalFromCheckpointAsync(DocumentJournal journal, EditorJournalCheckpoint checkpoint,
        CancellationToken cancellationToken) => Native.StartJournalFromCheckpointAsync(journal.File.Path, checkpoint)
            .AsCompletionTask(cancellationToken);

    internal Task RotateJournalAsync(DocumentJournal journal, ulong expectedSequence,
        CancellationToken cancellationToken) => Native.RotateJournalAsync(journal.File.Path, expectedSequence)
            .AsCompletionTask(cancellationToken);

    public string GetText() => Native.GetText(Native.Length + 1);

    private string ReadRange(long start, long end)
    {
        var previousStart = Native.TargetStart;
        var previousEnd = Native.TargetEnd;
        try { Native.SetTargetRange(start, end); return Native.GetTargetText(); }
        finally { Native.SetTargetRange(previousStart, previousEnd); }
    }

    private long ReplaceRange(long start, long end, string text)
    {
        Native.SetTargetRange(start, end);
        return Native.ReplaceTarget(Encoding.UTF8.GetByteCount(text), text);
    }

    public string GetSelectedText() => Native.GetSelText();
    public void SelectAll() => Native.SelectAll();
    public void Undo() { if (CanEdit && Native.CanUndo()) Native.Undo(); }
    public void Redo() { if (CanEdit && Native.CanRedo()) Native.Redo(); }
    public void ClearUndoQueue()
    {
        var modified = Native.Modify;
        Native.EmptyUndoBuffer();
        if (modified) MarkModified();
    }
    public void MarkSaved() => Native.SetSavePoint();
    public void MarkModified() => Native.UndoSavePoint = -1;

    // Scintilla's maintained UTF-16 line index avoids counting from the beginning
    // of a large document on every caret move.
    private long ToUtf16(long position)
    {
        var line = Native.LineFromPosition(position);
        return Native.IndexPositionFromLine(line, LineCharacterIndexType.Utf16)
            + Native.CountCodeUnits(Native.PositionFromLine(line), position);
    }

    private long FromUtf16(long position)
    {
        position = Math.Max(0, Math.Min(position,
            Native.IndexPositionFromLine(Native.LineCount, LineCharacterIndexType.Utf16)));
        var line = Native.LineFromIndexPosition(position, LineCharacterIndexType.Utf16);
        var start = Native.PositionFromLine(line);
        var index = Native.IndexPositionFromLine(line, LineCharacterIndexType.Utf16);
        var result = Native.PositionRelativeCodeUnits(start, position - index);
        return result < 0 ? Native.Length : result;
    }

    public void GetTextSelectionPosition(out int startPosition, out int endPosition)
    {
        startPosition = checked((int)ToUtf16(Native.SelectionStart));
        endPosition = checked((int)ToUtf16(Native.SelectionEnd));
    }

    public void SetTextSelectionPosition(int startPosition, int endPosition) => Native.SetSel(FromUtf16(startPosition), FromUtf16(endPosition));

    public void GetLineColumnSelection(out int startLineIndex, out int endLineIndex,
        out int startColumnIndex, out int endColumnIndex, out int selectedCount,
        out int lineCount, LineEnding lineEnding = LineEnding.Crlf)
    {
        var start = Native.SelectionStart;
        var end = Native.SelectionEnd;
        var startLine = Native.LineFromPosition(start);
        var endLine = Native.LineFromPosition(end);
        startLineIndex = checked((int)startLine + 1);
        endLineIndex = checked((int)endLine + 1);
        startColumnIndex = checked((int)Native.CountCodeUnits(Native.PositionFromLine(startLine), start) + 1);
        endColumnIndex = checked((int)Native.CountCodeUnits(Native.PositionFromLine(endLine), end) + 1);
        long selectedLength = 0;
        for (var selection = 0; selection < Native.Selections; selection++)
        {
            var rangeStart = Native.GetSelectionNStart(selection);
            var rangeEnd = Native.GetSelectionNEnd(selection);
            selectedLength += ToUtf16(rangeEnd) - ToUtf16(rangeStart);
            if (lineEnding == LineEnding.Crlf)
            {
                selectedLength += Native.LineFromPosition(rangeEnd) - Native.LineFromPosition(rangeStart);
            }
        }
        selectedCount = checked((int)selectedLength);
        lineCount = checked((int)Native.LineCount);
        if (end > start && endColumnIndex == 1)
        {
            endLineIndex--;
            endColumnIndex = checked((int)Native.CountCodeUnits(Native.PositionFromLine(endLine - 1), Native.GetLineEndPosition(endLine - 1)) + 1);
        }
    }

    public bool GoTo(int line)
    {
        if (line < 1 || line > Native.LineCount) return false;
        Native.GotoLine(line - 1);
        return true;
    }

    public double GetSingleLineHeight() => Native.TextHeight(0);
    public double GetFontZoomFactor() => _fontZoomFactor;
    public void SetFontZoomFactor(double value)
    {
        if (double.IsNaN(value) || double.IsInfinity(value)) return;
        value = Math.Max(_minimumZoomFactor, Math.Min(_maximumZoomFactor, Math.Round(value)));
        FontSize = value * ApplicationPreferences.EditorFontSize / 100;
    }

    public void GetScrollViewerPosition(out double horizontalOffset, out double verticalOffset)
    {
        // A tab that was never shown keeps the offsets it was restored with.
        if (_restoreScroll)
        {
            horizontalOffset = _horizontalOffset;
            verticalOffset = _verticalOffset;
            return;
        }
        horizontalOffset = Native.XOffset / _dpiScale;
        verticalOffset = Native.FirstVisibleLine * GetSingleLineHeight() / _dpiScale;
    }

    public void SetScrollViewerInitPosition(double horizontalOffset, double verticalOffset)
    {
        _horizontalOffset = horizontalOffset; _verticalOffset = verticalOffset;
        _restoreScroll = true;
    }

    private void RestoreScroll()
    {
        Native.XOffset = (int)Math.Round(_horizontalOffset * _dpiScale);
        Native.FirstVisibleLine = (long)(_verticalOffset * _dpiScale / Math.Max(1, GetSingleLineHeight()));
        _restoreScroll = false;
    }

    public new bool Focus(FocusState state) => _view.Focus(state);
    public void ResetFocusAndScrollToPreviousPosition() => _view.Focus(FocusState.Programmatic);
    public void TypeText(string text)
    {
        if (!CanEdit) return;
        text = LineEndingUtility.ApplyLineEnding(text, LineEnding.Cr);
        Native.PasteText(text);
    }

    public void DeleteSelection()
    {
        if (CanEdit && HasSelection) Native.Clear();
    }
    public void SelectCurrentLine()
    {
        var line = Native.LineFromPosition(Native.CurrentPos);
        Native.SetSel(Native.PositionFromLine(line), Native.PositionFromLine(line + 1));
    }

    public void SmartlyTrimTextSelection()
    {
        if (Native.SelectionIsRectangle || Native.Selections != 1) return;
        var text = GetSelectedText();
        if (string.IsNullOrWhiteSpace(text)) return;
        var trimmedStart = text.TrimStart(' ', '\t', '\r');
        var leading = text.Substring(0, text.Length - trimmedStart.Length);
        var offset = leading.LastIndexOf('\r') + 1;
        var trailing = trimmedStart.Length - trimmedStart.TrimEnd(' ', '\t', '\r').Length;
        Native.SetSel(Native.SelectionStart + Encoding.UTF8.GetByteCount(text.Substring(0, offset)),
            Native.SelectionEnd - Encoding.UTF8.GetByteCount(text.Substring(text.Length - trailing)));
    }

    public async Task PastePlainTextFromWindowsClipboardAsync(TextControlPasteEventArgs args)
    {
        if (args != null) args.Handled = true;
        if (!CanEdit) return;
        try
        {
            var data = Clipboard.GetContent();
            if (data.Contains(StandardDataFormats.Text)) TypeText(await data.GetTextAsync());
        }
        catch (Exception ex) { LoggingService.LogError($"[{nameof(TextEditorCore)}] Paste failed: {ex.Message}"); }
    }

    private void OnEditorPreviewKeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (!args.Handled && _keyboardCommandHandler.Handle(args)) args.Handled = true;
    }

    private void OnPointerWheelChanged(object sender, PointerRoutedEventArgs args)
    {
        var properties = args.GetCurrentPoint(_view).Properties;
        if (!args.KeyModifiers.HasFlag(VirtualKeyModifiers.Control) ||
            args.KeyModifiers.HasFlag(VirtualKeyModifiers.Shift) || properties.IsHorizontalMouseWheel)
        {
            return;
        }

        var delta = properties.MouseWheelDelta;
        if (delta > 0) IncreaseFontSize(0.1); else if (delta < 0) DecreaseFontSize(0.1);
        args.Handled = true;
    }

    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        IsEnabled = false;
        UnhookExternalEvents();
        Native.Modified -= OnNativeModified;
        Native.CharAdded -= OnNativeCharAdded;
        Native.UpdateUI -= OnNativeUpdateUI;
        Native.SavePointReached -= OnSavePointReached;
        Native.SavePointLeft -= OnSavePointLeft;
        _view.PreviewKeyDown -= OnEditorPreviewKeyDown;
        _view.PointerWheelChanged -= OnPointerWheelChanged;
        RemoveHandler(PointerMovedEvent, new PointerEventHandler(OnEditorPointerMoved));
        RemoveHandler(PointerExitedEvent, new PointerEventHandler(OnEditorPointerExited));
        _view.DpiChanged -= OnNativeDpiChanged;
        Loaded -= OnLoaded;
        ActualThemeChanged -= OnActualThemeChanged;
        ThemeSettingsService.OnThemeChanged -= OnAppThemeChanged;
        _accessibility.HighContrastChanged -= OnHighContrastChanged;
        // Free the text now, not at the next GC. A read lease or journal refuses this.
        try { Native.DetachDocument(); }
        catch (Exception ex) { LoggingService.LogError($"[{nameof(TextEditorCore)}] Document release failed: {ex.Message}"); }
    }
}
