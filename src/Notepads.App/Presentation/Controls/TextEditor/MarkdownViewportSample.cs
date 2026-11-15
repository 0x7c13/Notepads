// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Presentation.Controls.TextEditor;

internal readonly record struct MarkdownViewportSample(
    long StartByte,
    long EndByte,
    long DisplayRow,
    double Fraction,
    ulong NativeRevision,
    long Length,
    long ContentVersion,
    double BottomBlend = 0,
    double TopBlend = 0);
