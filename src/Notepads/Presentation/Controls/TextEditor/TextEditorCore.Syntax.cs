// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Features.Documents.FileTypes;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Presentation.Theming;
using Windows.UI.Core;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditorCore
{
    private SyntaxLanguageProfile _syntaxProfile;
    private string _syntaxProfileKey;
    private bool _languageDetectionQueued;
    private EditorSyntaxPauseReason _syntaxPauseReason;
    internal event EventHandler LanguageDetectionRequested;
    internal event EventHandler SyntaxStatusChanged;
    internal EditorSyntaxPauseReason SyntaxPauseReason => _syntaxPauseReason;

    internal void SetSyntaxLanguage(DocumentLanguage language, string fileName)
    {
        var profileKey = language.Id + (fileName?.EndsWith(".jsonc", StringComparison.OrdinalIgnoreCase) == true ? ":jsonc" : string.Empty);
        if (_syntaxProfileKey == profileKey) return;
        _syntaxProfileKey = profileKey;
        _syntaxProfile = SyntaxLanguageProfile.Create(language.Id, fileName);
        if (!_settingText) InstallSyntaxProfile();
    }

    internal string GetLanguageDetectionSample()
    {
        var end = Math.Min(Native.Length, DocumentLanguages.DetectionSampleBytes);
        while (end > 0 && end < Native.Length && (Native.GetCharAt(end) & 0xc0) == 0x80) end--;
        return ReadRange(0, end);
    }

    private void InstallSyntaxProfile()
    {
        if (_disposed || _syntaxProfile == null) return;
        Native.SetLexerLanguage(_syntaxProfile.Lexer, _syntaxProfile.Keywords,
            _syntaxProfile.PropertyNames, _syntaxProfile.PropertyValues);
        ApplySyntaxColors();
        UpdateSyntaxStatus();
    }

    internal void RetrySyntaxHighlighting()
    {
        if (_disposed || _settingText) return;
        // A pause during idle styling may not have raised a notification yet.
        UpdateSyntaxStatus();
        if (_syntaxPauseReason == EditorSyntaxPauseReason.Memory) InstallSyntaxProfile();
    }

    private void ApplySyntaxColors()
    {
        if (_syntaxProfile == null) return;
        var palette = _accessibility.HighContrast ? null : SyntaxColorPalette.ForTheme(ThemeSettingsService.ThemeMode);
        var foreground = palette != null && _syntaxProfile.Lexer.Length != 0
            ? ToScintillaColor(palette[SyntaxColorRole.Default])
            : Native.StyleGetFore((int)StylesCommon.Default);
        for (var style = 0; style < _syntaxProfile.Tokens.Length; style++)
        {
            // Scintilla reserves 32..39 for default, margins and UI styles.
            if (style is >= (int)StylesCommon.Default and <= (int)StylesCommon.LastPredefined) continue;
            var token = _syntaxProfile.Tokens[style];
            Native.StyleSetFore(style, palette == null || token == SyntaxColorRole.Default
                ? foreground : ToScintillaColor(palette[token]));
        }
    }

    private void UpdateSyntaxStatus()
    {
        var reason = Native.SyntaxHighlightingPauseReason;
        if (reason == _syntaxPauseReason) return;
        _syntaxPauseReason = reason;
        SyntaxStatusChanged?.Invoke(this, EventArgs.Empty);
    }

    private async void QueueLanguageDetection()
    {
        if (_languageDetectionQueued || _disposed) return;
        _languageDetectionQueued = true;
        try
        {
            await Dispatcher.RunAsync(CoreDispatcherPriority.Low, () =>
            {
                _languageDetectionQueued = false;
                if (!_disposed && !_settingText) LanguageDetectionRequested?.Invoke(this, EventArgs.Empty);
            });
        }
        catch (Exception exception)
        {
            _languageDetectionQueued = false;
            if (!_disposed) LoggingService.LogException(exception);
        }
    }
}
