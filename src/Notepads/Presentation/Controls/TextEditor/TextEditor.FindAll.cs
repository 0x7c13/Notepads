// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Globalization;
using Notepads.Presentation.Controls.FindAndReplace;
using Windows.UI.Core;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Automation;
using Windows.System;

namespace Notepads.Presentation.Controls.TextEditor;

public sealed partial class TextEditor
{
    private DocumentSearchResults _findAllResults;
    private SearchContext _findAllContext;
    private bool _findAllRunning;
    private void FindAllDocumentChanged(object sender, RoutedEventArgs args)
    {
        if (_findAllResults == null && !_findAllRunning) return;
        CancelSearch();
        ClearFindAllResults();
    }

    private void BeginFindAll(SearchContext context)
    {
        FindName(nameof(FindAllPanel));
        FindAllPanel.Visibility = Visibility.Visible;
        _findAllResults = null;
        _findAllContext = context;
        _findAllRunning = true;
        FindAllList.ItemsSource = null;
        UpdateFindAllHeader(_resourceLoader.GetString("EditorTools_FindAllSearchingTitle"), context,
            _resourceLoader.GetString("EditorTools_Searching"));
    }

    private void ShowFindAllResults(DocumentSearchResults results, SearchContext context)
    {
        FindName(nameof(FindAllPanel));
        FindAllPanel.Visibility = Visibility.Visible;
        _findAllResults = results;
        _findAllRunning = false;
        FindAllList.ItemsSource = results.Rows;
        var status = string.Format(_resourceLoader.GetString(results.Limited
            ? "EditorTools_FindAllPartialCount" : results.Rows.Count == 1
                ? "EditorTools_FindAllOne" : "EditorTools_FindAllCount"), results.Rows.Count);
        UpdateFindAllHeader(status, context, results.Limited
            ? string.Format(_resourceLoader.GetString("EditorTools_FindAllLimited"), results.Rows.Count) : null);
    }

    private void InvalidateFindAll()
    {
        if (_findAllResults == null && !_findAllRunning) return;
        var context = _findAllContext;
        _findAllRunning = false;
        _findAllResults = null;
        _findAllContext = null;
        FindAllList.ItemsSource = null;
        UpdateFindAllHeader(_resourceLoader.GetString("EditorTools_FindAllStaleTitle"), context,
            _resourceLoader.GetString("EditorTools_FindAllStale"));
    }

    private void UpdateFindAllHeader(string status, SearchContext context, string message = null)
    {
        FindAllStatus.Text = status;
        var summary = status;
        FindAllQueryText.Text = string.Empty;
        FindAllRulesText.Text = string.Empty;
        if (context != null)
        {
            // Keep control characters visible without changing the actual query.
            var query = context.SearchText.Replace("\r\n", "↵").Replace('\r', '␍').Replace('\n', '␊')
                .Replace('\t', '⇥').Replace('\0', '␀').Replace('\u2028', '↵').Replace('\u2029', '¶');
            var scope = _resourceLoader.GetString("EditorTools_FindAllScope");
            var rules = _resourceLoader.GetString(context.UseRegex ? "EditorTools_FindAllRegex" : "EditorTools_FindAllText") +
                " · " + _resourceLoader.GetString(context.MatchCase ? "EditorTools_FindAllCaseSensitive" : "EditorTools_FindAllCaseInsensitive");
            var fullRules = rules;
            if (!context.UseRegex)
            {
                var wordRule = _resourceLoader.GetString(context.MatchWholeWord ? "EditorTools_FindAllWholeWord" : "EditorTools_FindAllAnyWord");
                fullRules += " · " + wordRule;
                if (context.MatchWholeWord) rules += " · " + wordRule;
            }
            // Bound layout work before XAML's width-dependent ellipsis. Never cut
            // a surrogate pair or combining sequence; the tooltip retains all text.
            var end = 0;
            while (end < query.Length)
            {
                var next = end + StringInfo.GetNextTextElementLength(query.AsSpan(end));
                if (next > 160) break;
                end = next;
            }
            var preview = end < query.Length ? query[..end] + "…" : query;
            var queryFormat = _resourceLoader.GetString("EditorTools_FindAllQuery");
            FindAllQueryText.Text = " · " + string.Format(queryFormat, preview);
            FindAllRulesText.Text = " · " + scope + " · " + rules;
            summary += "\n" + scope + "\n" + string.Format(queryFormat, query) + "\n" + fullRules;
        }
        if (!string.IsNullOrEmpty(message)) summary += "\n" + message;
        FindAllFullSummary.Text = summary;
        AutomationProperties.SetName(FindAllStatus, summary);
    }

    private void FindAllQueryChanged(object sender, SearchContext context)
    {
        if (_disposed || FindAllPanel?.Visibility != Visibility.Visible) return;
        // TextChanged can arrive after a command has already read the new text.
        // That notification must not cancel a search using those same options.
        if (_findAllContext != null && context.SearchText == _findAllContext.SearchText && context.MatchCase == _findAllContext.MatchCase &&
            context.MatchWholeWord == _findAllContext.MatchWholeWord && context.UseRegex == _findAllContext.UseRegex) return;
        CancelSearch();
        ClearFindAllResults();
    }

    private void FindAllClose_Click(object sender, RoutedEventArgs args)
    {
        HideFindAndReplaceControl();
        TextEditorCore.Focus(FocusState.Programmatic);
    }

    private void ClearFindAllResults()
    {
        _findAllRunning = false;
        _findAllResults = null;
        _findAllContext = null;
        if (FindAllList != null) FindAllList.ItemsSource = null;
        if (FindAllPanel != null) FindAllPanel.Visibility = Visibility.Collapsed;
    }

    private void FindAllList_ItemClick(object sender, ItemClickEventArgs args)
    {
        if (!_disposed && _findAllResults != null)
        {
            var index = _findAllResults.Rows.IndexOf((DocumentSearchRow)args.ClickedItem);
            if (TextEditorCore.NavigateSearchResult(_findAllResults, index)) TextEditorCore.Focus(FocusState.Programmatic);
        }
    }

    private void FindAllList_PreviewKeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (_disposed || args.Handled || args.Key != VirtualKey.Enter || _findAllResults == null) return;
        var window = Window.Current.CoreWindow;
        if (window.GetKeyState(VirtualKey.Control).HasFlag(CoreVirtualKeyStates.Down) ||
            window.GetKeyState(VirtualKey.Menu).HasFlag(CoreVirtualKeyStates.Down)) return;

        // ListViewItem consumes Enter to invoke ItemClick. Handle it during
        // preview so keyboard confirmation can also collapse the results.
        args.Handled = true;
        if (TextEditorCore.NavigateSearchResult(_findAllResults, FindAllList.SelectedIndex))
        {
            CancelSearch();
            ClearFindAllResults();
            TextEditorCore.Focus(FocusState.Programmatic);
        }
    }
}
