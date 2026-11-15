// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Infrastructure.Settings;

namespace Notepads.Features.Preferences;

public enum MarkdownFrontMatterStyle { Table = 0, CodeBlock = 1, Hide = 2 }

public static partial class ApplicationPreferences
{
    internal const string MarkdownFrontMatterStyleKey = "MarkdownFrontMatterStyle";
    private static MarkdownFrontMatterStyle _markdownFrontMatterView = MarkdownFrontMatterStyle.Table;
    public static event EventHandler MarkdownFrontMatterStyleChanged;

    public static MarkdownFrontMatterStyle MarkdownFrontMatterView
    {
        get => _markdownFrontMatterView;
        set
        {
            if (value is not (MarkdownFrontMatterStyle.Table or MarkdownFrontMatterStyle.CodeBlock or MarkdownFrontMatterStyle.Hide))
                throw new ArgumentOutOfRangeException(nameof(value));
            if (_markdownFrontMatterView == value) return;
            ApplicationSettingsStore.Write(MarkdownFrontMatterStyleKey, (int)value);
            _markdownFrontMatterView = value;
            MarkdownFrontMatterStyleChanged?.Invoke(null, EventArgs.Empty);
        }
    }

    // Activation reconciles other window processes without rewriting their setting.
    internal static void RefreshMarkdownFrontMatterStyle()
    {
        var next = ApplicationSettingsStore.Read(MarkdownFrontMatterStyleKey) is int value && value is >= 0 and <= 2
            ? (MarkdownFrontMatterStyle)value : MarkdownFrontMatterStyle.Table;
        if (_markdownFrontMatterView == next) return;
        _markdownFrontMatterView = next;
        MarkdownFrontMatterStyleChanged?.Invoke(null, EventArgs.Empty);
    }
}
