// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using Notepads.Infrastructure.Settings;

namespace Notepads.Features.Preferences;

[Flags]
public enum ReadingAids { None = 0, Whitespace = 1, IndentGuides = 2, Braces = 4, Occurrences = 8 }

public static partial class ApplicationPreferences
{
    private const string ReadingAidsKey = "EditorReadingAids";
    private const ReadingAids AllReadingAids = ReadingAids.Whitespace | ReadingAids.IndentGuides | ReadingAids.Braces | ReadingAids.Occurrences;
    private static ReadingAids _readingAids = ReadingAids.Braces | ReadingAids.Occurrences;
    public static event EventHandler ReadingAidsChanged;

    public static ReadingAids EditorReadingAids
    {
        get => _readingAids;
        set
        {
            if ((value & ~AllReadingAids) != 0) throw new ArgumentOutOfRangeException(nameof(value));
            if (_readingAids == value) return;
            ApplicationSettingsStore.Write(ReadingAidsKey, (int)value);
            _readingAids = value;
            ReadingAidsChanged?.Invoke(null, EventArgs.Empty);
        }
    }

    internal static void RefreshReadingAids()
    {
        var next = ApplicationSettingsStore.Read(ReadingAidsKey) is int value && (value & ~(int)AllReadingAids) == 0
            ? (ReadingAids)value : ReadingAids.Braces | ReadingAids.Occurrences;
        if (_readingAids == next) return;
        _readingAids = next;
        ReadingAidsChanged?.Invoke(null, EventArgs.Empty);
    }
}
