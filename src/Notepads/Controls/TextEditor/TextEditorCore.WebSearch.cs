﻿// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Controls.TextEditor
{
    using System;
    using Windows.System;
    using Notepads.Services;
    using Notepads.Utilities;
    using System.Threading.Tasks;

    public partial class TextEditorCore
    {
        public async Task SearchInWebAsync()
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

                var searchUri = new Uri(string.Format(SearchEngineUtility.GetSearchUrlBySearchEngine(AppSettingsService.EditorDefaultSearchEngine)
                    , string.Join("+", searchString.Split(null))));
                await Launcher.LaunchUriAsync(searchUri);
            }
            catch (Exception ex)
            {
                LoggingService.LogError($"[{nameof(TextEditorCore)}] Failed to open search link: {ex.Message}");
            }
        }
    }
}