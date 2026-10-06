// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;

namespace Notepads.Presentation.Controls.GoTo;

public sealed class GoToEventArgs : EventArgs
{
    public GoToEventArgs(string searchLine)
    {
        SearchLine = searchLine;
    }

    public string SearchLine { get; }
}
