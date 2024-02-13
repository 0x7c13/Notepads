// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.DiffViewer
{
    using Windows.UI.Xaml;

    public interface ISideBySideDiffViewer
    {
        void RenderDiff(string left, string right, ElementTheme theme);
    }
}