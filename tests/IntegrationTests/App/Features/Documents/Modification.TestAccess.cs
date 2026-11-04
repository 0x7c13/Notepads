// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Threading.Tasks;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    internal readonly List<int> TestComparedChunks = new();
    internal readonly List<int> TestComparisonBufferSizes = new();
    internal Func<long, Task> TestComparisonPause;
    internal Task TestCompareContentAsync() => RefreshModificationStateAsync();
    internal Task TestComparisonCompletion => _contentComparisonCompletion;

    partial void OnContentComparisonChunk(long offset, int count, ref Task pause)
    {
        TestComparedChunks.Add(count);
        pause = TestComparisonPause?.Invoke(offset);
    }

    partial void OnContentComparisonBuffersAllocated(int size) => TestComparisonBufferSizes.Add(size);
}
