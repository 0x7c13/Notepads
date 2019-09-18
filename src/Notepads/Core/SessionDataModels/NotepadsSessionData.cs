namespace Notepads.Core.SessionDataModels
{
    using System;
    using System.Collections.Generic;

    internal class NotepadsSessionDataV1
    {
        public NotepadsSessionDataV1()
        {
            TextEditors = new List<TextEditorSessionDataV1>();
        }

        public int Version { get; set; } = 1;

        public Guid SelectedTextEditor { get; set; }

        public double TabScrollViewerHorizontalOffset { get; set; }

        public List<TextEditorSessionDataV1> TextEditors { get; }
    }
}