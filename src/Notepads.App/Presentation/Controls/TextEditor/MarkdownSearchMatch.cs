// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Presentation.Controls.TextEditor;

internal readonly record struct MarkdownSearchMatch(long Serial, long Start, long End, ulong Attachment,
    ulong NativeRevision, ulong SelectionRevision, long ContentVersion);
