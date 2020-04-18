namespace Notepads.Controls.TextEditor
{
    using System;
    using Notepads.Services;

    public partial class TextEditorCore
    {
        private void IncreaseFontSize(double delta)
        {
            if (_fontZoomFactor < _maximumZoomFactor)
            {
                if (_fontZoomFactor % 10 > 0)
                {
                    SetFontZoomFactor(Math.Ceiling(_fontZoomFactor / 10) * 10);
                }
                else
                {
                    FontSize += delta * EditorSettingsService.EditorFontSize;
                }
            }
        }

        private void DecreaseFontSize(double delta)
        {
            if (_fontZoomFactor > _minimumZoomFactor)
            {
                if (_fontZoomFactor % 10 > 0)
                {
                    SetFontZoomFactor(Math.Floor(_fontZoomFactor / 10) * 10);
                }
                else
                {
                    FontSize -= delta * EditorSettingsService.EditorFontSize;
                }
            }
        }

        private void ResetFontSizeToDefault()
        {
            FontSize = EditorSettingsService.EditorFontSize;
        }
    }
}