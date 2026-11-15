// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.IO;
using System.Threading.Tasks;
using Notepads.Infrastructure.Diagnostics;
using Notepads.Markdown;
using Notepads.Presentation.Views.MainPage;
using Windows.Storage;
using Windows.System;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Media;

namespace Notepads.Presentation.Controls.Markdown;

public sealed partial class MarkdownExtensionView
{
    private async void LinkRequested(MarkdownView sender, MarkdownLinkRequestedEventArgs args)
    {
        if (_disposed || !_active || !_session.IsCurrent || args.DocumentId != _session.DocumentId ||
            args.Revision != _session.AcceptedRevision || args.TextGeneration != _view.GetSelection().TextGeneration) return;
        var heading = args.LocalHeadingNode;
        if (heading != 0)
        {
            QueueDocumentTarget(new(null, _editor.MarkdownNativeRevision, Heading: (heading, args.TextGeneration)));
            return;
        }
        if (args.LocalPreviewPosition is ulong position)
        {
            QueueDocumentTarget(new(null, _editor.MarkdownNativeRevision, (position, args.TextGeneration)));
            return;
        }
        var request = ++_linkRequest;
        _pendingFragment = null;
        try
        {
            var destination = args.Destination;
            if (destination.StartsWith('#'))
            {
                var headingTitle = args.IsWikiLink ? destination[1..].Trim() : null;
                if (!string.IsNullOrEmpty(headingTitle))
                    QueueFragment(headingTitle, wikiHeading: true);
                else
                    QueueFragment(destination);
                return;
            }
            await OpenPreviewDestinationAsync(destination, args.IsWikiLink, _owner.EditingFile?.Path,
                () => !_disposed && _active && request == _linkRequest && _session.IsCurrent &&
                    args.Revision == _session.AcceptedRevision);
        }
        catch (Exception error)
        {
            LoggingService.LogException(error);
            if (!_disposed && request == _linkRequest) ShowStatus("MarkdownPreview_LinkFailed");
        }
    }

    private async Task OpenPreviewDestinationAsync(string destination, bool wiki, string originPath, Func<bool> isCurrent)
    {
        if (!TryResolvePreviewDestination(destination, wiki, originPath, out var uri, out var fullPath, out var fragment))
        {
            if (isCurrent()) ShowStatus("MarkdownPreview_LinkFailed");
            return;
        }
        if (uri != null)
        {
            if (!await Launcher.LaunchUriAsync(uri) && isCurrent()) ShowStatus("MarkdownPreview_LinkFailed");
            return;
        }
        var file = wiki ? await ResolveWikiFileAsync(fullPath) : await StorageFile.GetFileFromPathAsync(fullPath);
        if (!isCurrent()) return;
        DependencyObject element = _owner;
        while (element != null && element is not NotepadsMainPage) element = VisualTreeHelper.GetParent(element);
        if (element is not NotepadsMainPage page || !await page.OpenMarkdownLinkAsync(file, fragment, wiki,
            () => _view.Visibility == Visibility.Visible && isCurrent()))
            if (isCurrent()) ShowStatus("MarkdownPreview_LinkFailed");
    }

    internal static bool TryResolvePreviewDestination(string destination, bool wiki, string originPath,
        out Uri remoteUri, out string fullPath, out string fragment)
    {
        remoteUri = null;
        fullPath = fragment = null;
        try
        {
            if (!MarkdownDestination.TryParse(destination, wiki, allowMailto: true, out var path,
                out remoteUri, out fragment)) return false;
            if (remoteUri != null) return true;
            bool fullyQualified = Path.IsPathFullyQualified(path);
            if (!fullyQualified)
            {
                if (string.IsNullOrEmpty(originPath) || !Path.IsPathFullyQualified(originPath)) return false;
                path = Path.Combine(Path.GetDirectoryName(originPath), path);
            }
            fullPath = Path.GetFullPath(path);
            return true;
        }
        catch (Exception error) when (error is UriFormatException or ArgumentException or
            NotSupportedException or PathTooLongException)
        {
            return false;
        }
    }

    private static async Task<StorageFile> ResolveWikiFileAsync(string fullPath)
    {
        if (Path.HasExtension(fullPath))
        {
            try { return await StorageFile.GetFileFromPathAsync(fullPath); }
            catch (FileNotFoundException) when (!fullPath.EndsWith(".md", StringComparison.OrdinalIgnoreCase)) { }
        }
        return await StorageFile.GetFileFromPathAsync(fullPath + ".md");
    }
}
