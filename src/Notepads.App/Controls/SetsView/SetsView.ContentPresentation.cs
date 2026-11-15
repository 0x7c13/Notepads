// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

using System;
using System.Collections.Generic;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace Notepads.Controls;

public partial class SetsView
{
    private const string SetsContentHostName = "SetsContentHost";
    private const int MaximumCachedContents = 8;
    private Grid _setsContentHost;
    private ContentPresenter _setsContentPresenter;
    private SetsViewItem _presentedSet;
    private readonly List<(SetsViewItem Item, ContentPresenter Presenter)> _cachedContents = [];

    internal Func<SetsViewItem, bool> ShouldCacheContent { get; set; }
    internal event Action<SetsViewItem, bool> CachedContentVisibilityChanged;
    private void PresentContent(SetsViewItem selected, bool notifyWhenLoaded)
    {
        ContentPresenter restored = null;
        if (!ReferenceEquals(_presentedSet, selected))
        {
            var cachedIndex = _cachedContents.FindIndex(entry => ReferenceEquals(entry.Item, selected));
            if (cachedIndex >= 0)
            {
                restored = _cachedContents[cachedIndex].Presenter;
                _cachedContents.RemoveAt(cachedIndex);
            }

            var previous = _presentedSet;
            _presentedSet = selected;
            if (_setsContentPresenter is { } current)
            {
                current.Loaded -= SetsContentPresenter_Loaded;
                if (previous != null && Items.Contains(previous) && ShouldCacheContent?.Invoke(previous) == true)
                {
                    current.Visibility = Visibility.Collapsed;
                    _cachedContents.Add((previous, current));
                    _setsContentPresenter = null;
                    CachedContentVisibilityChanged?.Invoke(previous, false);
                    if (_cachedContents.Count > MaximumCachedContents) RemoveCachedContentAt(0);
                }
                else if (restored != null)
                {
                    RemoveContentPresenter(current);
                }
            }

            if (restored != null)
            {
                _setsContentPresenter = restored;
                restored.Visibility = Visibility.Visible;
            }
        }

        var presenter = _setsContentPresenter;
        if (selected == null)
        {
            if (presenter != null)
            {
                presenter.Content = null;
                presenter.ContentTemplate = null;
            }
            return;
        }

        bool attach = presenter == null;
        presenter = _setsContentPresenter ??= new ContentPresenter();
        bool contentChanged = !ReferenceEquals(presenter.Content, selected.Content);
        // Reassigning the same Content can unload the editor on the next layout.
        if (contentChanged) presenter.Content = selected.Content;
        if (!ReferenceEquals(presenter.ContentTemplate, selected.ContentTemplate))
            presenter.ContentTemplate = selected.ContentTemplate;
        if (notifyWhenLoaded && !presenter.IsLoaded)
        {
            presenter.Loaded -= SetsContentPresenter_Loaded;
            presenter.Loaded += SetsContentPresenter_Loaded;
        }
        if (attach) _setsContentHost.Children.Add(presenter);
        // An evicted editor may reattach before XAML delivers Unloaded. Report
        // selection explicitly so its old cached-hidden state cannot survive.
        if (restored != null || contentChanged) CachedContentVisibilityChanged?.Invoke(selected, true);
    }

    internal void ClearCachedContent()
    {
        while (_cachedContents.Count > 0) RemoveCachedContentAt(_cachedContents.Count - 1);
    }

    private void RemoveCachedContentAt(int index)
    {
        var presenter = _cachedContents[index].Presenter;
        _cachedContents.RemoveAt(index);
        RemoveContentPresenter(presenter);
    }

    private void RemoveContentPresenter(ContentPresenter presenter)
    {
        presenter.Loaded -= SetsContentPresenter_Loaded;
        presenter.Content = null;
        _setsContentHost.Children.Remove(presenter);
    }

    private void SetsContentPresenter_Loaded(object sender, RoutedEventArgs e)
    {
        var presenter = (ContentPresenter)sender;
        presenter.Loaded -= SetsContentPresenter_Loaded;
        if (!ReferenceEquals(presenter, _setsContentPresenter)) return;
        var args = new SetSelectedEventArgs(((SetsViewItem)ContainerFromItem(SelectedItem))?.Content, (SetsViewItem)ContainerFromItem(SelectedItem));
        SetSelected?.Invoke(this, args);
    }

}
