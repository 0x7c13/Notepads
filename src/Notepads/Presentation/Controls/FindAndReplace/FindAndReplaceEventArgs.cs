// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;

namespace Notepads.Presentation.Controls.FindAndReplace;

public enum FindAndReplaceMode
{
    FindOnly,
    Replace,
    ReplaceAll
}

public enum SearchDirection
{
    Previous,
    Next
}

public sealed class FindAndReplaceEventArgs : EventArgs
{
    public FindAndReplaceEventArgs(
        SearchContext searchContext,
        string replaceText,
        FindAndReplaceMode findAndReplaceMode,
        SearchDirection searchDirection = SearchDirection.Next)
    {
        SearchContext = searchContext;
        ReplaceText = replaceText;
        FindAndReplaceMode = findAndReplaceMode;
        SearchDirection = searchDirection;
    }

    public SearchContext SearchContext { get; }

    public string ReplaceText { get; }

    public FindAndReplaceMode FindAndReplaceMode { get; }

    public SearchDirection SearchDirection { get; }
}
