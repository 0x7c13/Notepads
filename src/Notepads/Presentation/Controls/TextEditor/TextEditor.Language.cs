// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Features.Documents.FileTypes;
using WinUIEditor;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    public event EventHandler LanguageChanged;
    public event EventHandler LanguageOverrideChanged;
    public DocumentLanguage DocumentLanguage { get; private set; } = DocumentLanguages.PlainText;
    public DocumentLanguage DetectedLanguage => DocumentLanguages.Detect(EditingFileName ?? FileNamePlaceholder,
        TextEditorCore.GetLanguageDetectionSample());
    public string LanguageOverride { get; private set; }
    public EditorSyntaxPauseReason SyntaxHighlightingPauseReason => TextEditorCore.SyntaxPauseReason;
    public bool CanChangeLanguage => !_disposed && !_recoveryTransition && !_transferCommitInProgress && TextEditorCore.IsEnabled;

    public void SetLanguageOverride(string id)
    {
        if (!CanChangeLanguage) return;
        var previousLanguage = DocumentLanguage;
        RestoreLanguageOverride(id);
        if (ReferenceEquals(previousLanguage, DocumentLanguage)) TextEditorCore.RetrySyntaxHighlighting();
    }

    private void RestoreLanguageOverride(string id)
    {
        var normalized = DocumentLanguages.NormalizeOverride(id);
        if (LanguageOverride == normalized) return;
        LanguageOverride = normalized;
        RefreshDocumentLanguage(notify: false);
        LanguageOverrideChanged?.Invoke(this, EventArgs.Empty);
        LanguageChanged?.Invoke(this, EventArgs.Empty);
    }

    private void RefreshDocumentLanguage(bool notify = true)
    {
        if (_disposed || TextEditorCore == null) return;
        var name = EditingFileName ?? FileNamePlaceholder;
        var language = DocumentLanguages.Find(LanguageOverride) ?? DetectedLanguage;
        var changed = !ReferenceEquals(language, DocumentLanguage);
        DocumentLanguage = language;
        TextEditorCore.SetSyntaxLanguage(language, name);
        if (changed && notify) LanguageChanged?.Invoke(this, EventArgs.Empty);
    }

    private void OnLanguageDetectionRequested(object sender, EventArgs args)
    {
        if (LanguageOverride == null) RefreshDocumentLanguage();
    }

    private void OnSyntaxStatusChanged(object sender, EventArgs args) => LanguageChanged?.Invoke(this, EventArgs.Empty);
}
