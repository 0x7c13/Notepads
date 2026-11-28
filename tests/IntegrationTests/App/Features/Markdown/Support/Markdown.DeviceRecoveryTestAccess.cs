// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using Notepads.Markdown;

namespace Notepads.Presentation.Controls.Markdown;

internal sealed partial class MarkdownSourceSession
{
    internal DocumentUpdateResult TestLastSubmissionReceipt { get; private set; }

    partial void ObserveSubmissionReceipt(DocumentUpdateResult receipt) => TestLastSubmissionReceipt = receipt;
}
