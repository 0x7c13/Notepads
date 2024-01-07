// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Core.SessionDataModels
{
    using System;
    using System.Collections.Generic;

    internal sealed class NotepadsSessionDataV1
    {
        public int Version { get; set; } = 1;

        public Guid SelectedTextEditor { get; set; }

        public double TabScrollViewerHorizontalOffset { get; set; } = 0;

        public List<TextEditorSessionDataV1> TextEditors { get; set; } = new List<TextEditorSessionDataV1>();
    }
}