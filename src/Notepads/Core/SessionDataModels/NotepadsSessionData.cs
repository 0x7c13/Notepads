namespace Notepads.Core.SessionDataModels
{
    using System;
    using System.Collections.Generic;

    internal class NotepadsSessionDataV1
    {
        public int Version { get; set; } = 1;

        public Guid SelectedTextEditor { get; set; }

        public double TabScrollViewerHorizontalOffset { get; set; } = 0;

        public List<TextEditorSessionDataV1> TextEditors { get; set; } = new List<TextEditorSessionDataV1>();
    }
}