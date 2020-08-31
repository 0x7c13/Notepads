namespace Notepads.Core.SessionDataModels
{
    using System;
    using Notepads.Controls.TextEditor;

    internal class TextEditorSessionDataV1
    {
        public Guid Id { get; set; }

        public string LastSavedBackupFilePath { get; set; }

        public string PendingBackupFilePath { get; set; }

        public string EditingFileFutureAccessToken { get; set; }

        public string EditingFileName { get; set; }

        public string EditingFilePath { get; set; }

        public TextEditorStateMetaData StateMetaData { get; set; }
    }
}