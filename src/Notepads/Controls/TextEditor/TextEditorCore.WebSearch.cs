namespace Notepads.Controls.TextEditor
{
    using System;
    using Windows.System;
    using Notepads.Services;
    using Notepads.Utilities;

    public partial class TextEditorCore
    {
        public async void SearchInWeb()
        {
            try
            {
                if (Document.Selection.Length == 0)
                {
                    return;
                }

                var selectedText = Document.Selection.Text.Trim();

                // The maximum length of a URL in the address bar is 2048 characters
                // Let's take 2000 here to make sure we are not exceeding the limit
                // Otherwise we will see "Invalid URI: The uri string is too long" exception
                var searchString = selectedText.Length <= 2000 ? selectedText : selectedText.Substring(0, 2000);

                if (Uri.TryCreate(searchString, UriKind.Absolute, out var webUrl) &&
                    (webUrl.Scheme == Uri.UriSchemeHttp || webUrl.Scheme == Uri.UriSchemeHttps))
                {
                    await Launcher.LaunchUriAsync(webUrl);
                    return;
                }

                var searchUri = new Uri(string.Format(SearchEngineUtility.GetSearchUrlBySearchEngine(EditorSettingsService.EditorDefaultSearchEngine)
                    , string.Join("+", searchString.Split(null))));
                await Launcher.LaunchUriAsync(searchUri);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"Failed to open search link: {ex.Message}");
            }
        }
    }
}