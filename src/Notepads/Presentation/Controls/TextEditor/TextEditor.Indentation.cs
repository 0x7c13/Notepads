// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    public event EventHandler IndentationChanged;
    public event EventHandler IndentationOverrideChanged;
    public int IndentationOverride => TextEditorCore.IndentationOverride;
    public int EffectiveIndentation => TextEditorCore.EffectiveIndentation;
    public bool CanChangeIndentation => CanChangeLanguage && Mode == TextEditorMode.Editing;
    public bool CanConvertIndentation => CanChangeIndentation && TextEditorCore.CanConvertIndentation;

    public void SetIndentationOverride(int value)
    {
        if (!CanChangeIndentation || value == -1 && DocumentLanguage.Id == "yaml") return;
        RestoreIndentationOverride(value);
    }

    private void RestoreIndentationOverride(int value)
    {
        if (IndentationOverride == value) return;
        TextEditorCore.SetIndentationOverride(value);
        IndentationOverrideChanged?.Invoke(this, EventArgs.Empty);
    }

    public Task ConvertIndentationAsync(bool useTabs) => RunDocumentOperationAsync(async cancellation =>
    {
        if (!CanConvertIndentation || useTabs && DocumentLanguage.Id == "yaml") return;
        var effective = EffectiveIndentation;
        var spaces = effective > 0 ? effective : 4;
        await TextEditorCore.ConvertIndentationAsync(useTabs, DocumentOwnerId, cancellation);
        // Keep automatic detection for a 3/5/6/7-space document rather than
        // silently replacing its unit with one of the menu's explicit presets.
        SetIndentationOverride(useTabs ? -1 : spaces is 2 or 4 or 8 ? spaces : 0);
    });

    private void OnIndentationChanged(object sender, EventArgs args) => IndentationChanged?.Invoke(this, EventArgs.Empty);
}
