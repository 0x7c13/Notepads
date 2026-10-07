// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Threading.Tasks;
using Notepads.Features.Documents.Storage;
using Notepads.Presentation.Controls.TextEditor;

namespace NotepadsEditorTests;

// Test-only text loading through the production baseline path, which
// normalizes CRLF and LF to CR.
internal static class TextLoadExtensions
{
    // Owns these temporary baselines; each is deleted when its last lease closes.
    private static readonly Guid BaselineOwner = Guid.NewGuid();

    internal static async Task LoadTextAsync(this TextEditorCore core, string text)
    {
        using var baseline = text.Length == 0 ? DocumentBaseline.CreateEmpty(BaselineOwner) : await DocumentBaseline.FromTextAsync(text, BaselineOwner);
        await core.LoadBaselineAsync(baseline, preserveUndo: false);
    }
}
