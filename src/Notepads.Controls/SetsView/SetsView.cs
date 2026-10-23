// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.


using System;
using System.Linq;
using CommunityToolkit.WinUI;
using Windows.ApplicationModel.DataTransfer;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Data;
using Windows.UI.Xaml.Input;

namespace Notepads.Controls;

/// <summary>
/// SetsView is a control for displaying a set of sets and their content.
/// </summary>
[TemplatePart(Name = SetsContentPresenterName, Type = typeof(ContentPresenter))]
[TemplatePart(Name = SetsViewContainerName, Type = typeof(Grid))]
[TemplatePart(Name = SetsItemsPresenterName, Type = typeof(ItemsPresenter))]
[TemplatePart(Name = SetsScrollViewerName, Type = typeof(ScrollViewer))]
[TemplatePart(Name = SetsScrollBackButtonName, Type = typeof(ButtonBase))]
[TemplatePart(Name = SetsScrollForwardButtonName, Type = typeof(ButtonBase))]
[TemplatePart(Name = SetsItemsScrollViewerLeftSideShadowName, Type = typeof(DropShadowPanel))]
[TemplatePart(Name = SetsItemsScrollViewerRightSideShadowName, Type = typeof(DropShadowPanel))]
public partial class SetsView : ListViewBase
{
    private const int ScrollAmount = 50; // TODO: Should this be based on SetsWidthMode

    private const string SetsContentPresenterName = "SetsContentPresenter";
    private const string SetsViewContainerName = "SetsViewContainer";
    private const string SetsItemsPresenterName = "SetsItemsPresenter";
    private const string SetsScrollViewerName = "ScrollViewer";
    private const string SetsItemsScrollViewerLeftSideShadowName = "SetsItemsScrollViewerLeftSideShadow";
    private const string SetsItemsScrollViewerRightSideShadowName = "SetsItemsScrollViewerRightSideShadow";
    private const string SetsScrollBackButtonName = "SetsScrollBackButton";
    private const string SetsScrollForwardButtonName = "SetsScrollForwardButton";

    private ContentPresenter _setsContentPresenter;
    private Grid _setsViewContainer;
    private ItemsPresenter _setItemsPresenter;
    private ScrollViewer _setsScroller;
    private DropShadowPanel _setsItemsScrollViewerLeftSideShadow;
    private DropShadowPanel _setsItemsScrollViewerRightSideShadow;
    private ButtonBase _setsScrollBackButton;
    private ButtonBase _setsScrollForwardButton;
    private long _setsScrollBackVisibilityToken;
    private long _setsScrollForwardVisibilityToken;

    private bool _hasLoaded;
    private bool _isDragging;
    private bool _scrollerReady;
    private bool _applyingScroll;
    private double? _pendingScrollOffset;
    private SetsViewItem _pendingScrollItem;

    private double _scrollViewerHorizontalOffset = .0f;
    public double ScrollViewerHorizontalOffset => _pendingScrollOffset ?? _scrollViewerHorizontalOffset;

    /// <summary>
    /// Initializes a new instance of the <see cref="SetsView"/> class.
    /// </summary>
    public SetsView()
    {
        DefaultStyleKey = typeof(SetsView);

        // Container Generation Hooks
        RegisterPropertyChangedCallback(ItemsSourceProperty, ItemsSource_PropertyChanged);
        ItemContainerGenerator.ItemsChanged += ItemContainerGenerator_ItemsChanged;

        // Drag and Layout Hooks
        DragItemsStarting += SetsView_DragItemsStarting;
        DragItemsCompleted += SetsView_DragItemsCompleted;
        SizeChanged += SetsView_SizeChanged;

        // Selection Hook
        SelectionChanged += SetsView_SelectionChanged;
        InitializeTabDragging();
    }

    /// <summary>
    /// Occurs when a set's Close button is clicked.  Set <see cref="SetClosingEventArgs.Cancel"/> to true to prevent automatic Set Closure.
    /// </summary>
    public event EventHandler<SetClosingEventArgs> SetClosing;

    /// <summary>
    /// Occurs when a set is selected in <see cref="SetsView"/>.
    /// </summary>
    public event EventHandler<SetSelectedEventArgs> SetSelected;

    /// <summary>
    /// Occurs when a set is tapped in <see cref="SetsView"/>.
    /// </summary>
    public event EventHandler<SetSelectedEventArgs> SetTapped;

    /// <summary>
    /// Occurs when a set is double tapped in <see cref="SetsView"/>.
    /// </summary>
    public event EventHandler<SetSelectedEventArgs> SetDoubleTapped;

    /// <inheritdoc/>
    protected override DependencyObject GetContainerForItemOverride()
    {
        return new SetsViewItem();
    }

    /// <inheritdoc/>
    protected override bool IsItemItsOwnContainerOverride(object item)
    {
        return item is SetsViewItem;
    }

    /// <inheritdoc/>
    protected override void OnApplyTemplate()
    {
        InvalidateTabDrag();
        _railViewport = null;
        _scrollerReady = false;
        base.OnApplyTemplate();

        if (_setItemsPresenter != null)
        {
            _setItemsPresenter.SizeChanged -= SetsView_SizeChanged;
        }

        if (_setsScroller != null)
        {
            _setsScroller.Loaded -= SetsScrollViewer_Loaded;
            _setsScroller.LayoutUpdated -= SetsScrollViewer_PendingScrollLayoutUpdated;
            _setsScroller.ViewChanged -= SetsScrollViewer_ViewChanged;
        }

        _setsContentPresenter = GetTemplateChild(SetsContentPresenterName) as ContentPresenter;
        _setsViewContainer = GetTemplateChild(SetsViewContainerName) as Grid;
        _setItemsPresenter = GetTemplateChild(SetsItemsPresenterName) as ItemsPresenter;
        _setsScroller = GetTemplateChild(SetsScrollViewerName) as ScrollViewer;

        if (_setItemsPresenter != null)
        {
            _setItemsPresenter.SizeChanged += SetsView_SizeChanged;
        }

        if (_setsScroller != null)
        {
            _setsScroller.Loaded += SetsScrollViewer_Loaded;
            _setsScroller.ViewChanged += SetsScrollViewer_ViewChanged;
        }
    }

    private void SetsScrollViewer_Loaded(object sender, RoutedEventArgs e)
    {
        _scrollerReady = false;
        if (_setsScrollBackButton != null)
        {
            _setsScrollBackButton.Click -= ScrollSetBackButton_Click;
            _setsScrollBackButton.UnregisterPropertyChangedCallback(VisibilityProperty, _setsScrollBackVisibilityToken);
        }

        if (_setsScrollForwardButton != null)
        {
            _setsScrollForwardButton.Click -= ScrollSetForwardButton_Click;
            _setsScrollForwardButton.UnregisterPropertyChangedCallback(VisibilityProperty, _setsScrollForwardVisibilityToken);
        }

        _setsScrollBackButton = _setsScroller.FindDescendant(SetsScrollBackButtonName) as ButtonBase;
        _setsScrollForwardButton = _setsScroller.FindDescendant(SetsScrollForwardButtonName) as ButtonBase;
        _setsItemsScrollViewerLeftSideShadow = _setsScroller.FindDescendant(SetsItemsScrollViewerLeftSideShadowName) as DropShadowPanel;
        _setsItemsScrollViewerRightSideShadow = _setsScroller.FindDescendant(SetsItemsScrollViewerRightSideShadowName) as DropShadowPanel;

        if (_setsScrollBackButton != null)
        {
            _setsScrollBackButton.Click += ScrollSetBackButton_Click;
            _setsScrollBackVisibilityToken = _setsScrollBackButton.RegisterPropertyChangedCallback(VisibilityProperty, ScrollButton_VisibilityChanged);
        }

        if (_setsScrollForwardButton != null)
        {
            _setsScrollForwardButton.Click += ScrollSetForwardButton_Click;
            _setsScrollForwardVisibilityToken = _setsScrollForwardButton.RegisterPropertyChangedCallback(VisibilityProperty, ScrollButton_VisibilityChanged);
        }

        UpdateScrollButtonNames();
        UpdateSetSeparators();

        // Tab headers finish sizing during their own Loaded handlers. Applying
        // an offset here would clamp it against an extent that can still be zero.
        _setsScroller.LayoutUpdated -= SetsScrollViewer_PendingScrollLayoutUpdated;
        _setsScroller.LayoutUpdated += SetsScrollViewer_PendingScrollLayoutUpdated;
    }

    private void UpdateScrollButtonNames()
    {
        if (_setsScrollBackButton != null)
            AutomationProperties.SetName(_setsScrollBackButton, ScrollBackButtonAutomationName);
        if (_setsScrollForwardButton != null)
            AutomationProperties.SetName(_setsScrollForwardButton, ScrollForwardButtonAutomationName);
    }

    private void SetsScrollViewer_PendingScrollLayoutUpdated(object sender, object e)
    {
        if (_setsScroller == null) return;
        // The empty scroller may load before restored headers are added.
        // Their first Loaded handler establishes the constrained header width.
        if (_setsScroller.IsLoaded && Items.Count > 0 && !_hasLoaded) return;
        _setsScroller.LayoutUpdated -= SetsScrollViewer_PendingScrollLayoutUpdated;
        if (!_setsScroller.IsLoaded) return;
        _scrollerReady = true;
        ApplyPendingScroll();
    }

    private void ScrollSetBackButton_Click(object sender, RoutedEventArgs e)
    {
        _setsScroller.ChangeView(Math.Max(0, _setsScroller.HorizontalOffset - ScrollAmount), null, null);
    }

    private void ScrollSetForwardButton_Click(object sender, RoutedEventArgs e)
    {
        _setsScroller.ChangeView(Math.Min(_setsScroller.ScrollableWidth, _setsScroller.HorizontalOffset + ScrollAmount), null, null);
    }

    private void SetsView_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_mouseDrag is { External: false } drag && !ReferenceEquals(SelectedItem, drag.Item)) ResetMouseDrag();
        if (_isDragging)
        {
            // Skip if we're dragging, we'll reset when we're done.
            return;
        }

        // Input can change selection before a deferred scroll is applied. Keep
        // the latest selection visible, but do not let synthetic Loaded refreshes
        // replace the explicitly restored tab-strip position.
        if (e?.AddedItems.Count > 0 && (_pendingScrollOffset != null || _pendingScrollItem != null) &&
            SelectedItem is SetsViewItem selected)
            ScrollSetIntoView(selected);

        if (_setsContentPresenter != null)
        {
            if (SelectedItem == null)
            {
                _setsContentPresenter.Content = null;
                _setsContentPresenter.ContentTemplate = null;
            }
            else
            {
                if (ContainerFromItem(SelectedItem) is SetsViewItem container)
                {
                    // Reassigning even the same Content can rebuild UWP's
                    // presentation on the next layout and unload the editor.
                    if (!ReferenceEquals(_setsContentPresenter.Content, container.Content))
                        _setsContentPresenter.Content = container.Content;
                    if (!ReferenceEquals(_setsContentPresenter.ContentTemplate, container.ContentTemplate))
                        _setsContentPresenter.ContentTemplate = container.ContentTemplate;

                    if (e != null) _setsContentPresenter.Loaded += SetsContentPresenter_Loaded;
                }
            }

            UpdateScrollViewerShadows();
            UpdateSetSeparators();
        }

        // If our width can be effected by the selection, need to run algorithm.
        if (!double.IsNaN(SelectedSetWidth))
        {
            SetsView_SizeChanged(sender, null);
        }
    }

    private void UpdateSetSeparators()
    {
        for (int i = 0; i < Items.Count; i++) UpdateSetSeparator(i);
    }

    private void ScrollButton_VisibilityChanged(DependencyObject sender, DependencyProperty property)
    {
        UpdateSetSeparator(0);
        UpdateSetSeparator(Items.Count - 1);
    }

    private void UpdateSetSeparator(int index)
    {
        if (index < 0 || index >= Items.Count || ContainerFromIndex(index) is not SetsViewItem set) return;

        // Overflow arrows replace the strip's outer separators, including when
        // an arrow is disabled at the start/end of the scroll range.
        if (index == 0 && SelectedIndex > 0 && _setsScrollBackButton?.Visibility != Visibility.Visible)
            set.ShowLeftSideSeparator();
        else
            set.HideLeftSideSeparator();

        if (index != SelectedIndex && index != SelectedIndex - 1 &&
            (index != Items.Count - 1 || _setsScrollForwardButton?.Visibility != Visibility.Visible))
            set.ShowRightSideSeparator();
        else
            set.HideRightSideSeparator();
    }

    private void SetsContentPresenter_Loaded(object sender, RoutedEventArgs e)
    {
        _setsContentPresenter.Loaded -= SetsContentPresenter_Loaded;
        var args = new SetSelectedEventArgs(((SetsViewItem)ContainerFromItem(SelectedItem))?.Content, (SetsViewItem)ContainerFromItem(SelectedItem));
        SetSelected?.Invoke(this, args);
    }

    /// <inheritdoc/>
    protected override void PrepareContainerForItemOverride(DependencyObject element, object item)
    {
        base.PrepareContainerForItemOverride(element, item);

        if (element is not SetsViewItem setItem) return;

        setItem.Loaded -= SetsViewItem_Loaded;
        setItem.Closing -= SetsViewItem_Closing;
        setItem.Tapped -= SetsViewItem_Tapped;
        setItem.DoubleTapped -= SetsViewItem_DoubleTapped;
        setItem.PointerEntered -= SetItem_PointerEntered;
        setItem.PointerExited -= SetItem_PointerExited;
        setItem.Loaded += SetsViewItem_Loaded;
        setItem.Closing += SetsViewItem_Closing;
        setItem.Tapped += SetsViewItem_Tapped;
        setItem.DoubleTapped += SetsViewItem_DoubleTapped;
        setItem.PointerEntered += SetItem_PointerEntered;
        setItem.PointerExited += SetItem_PointerExited;

        if (setItem.Header == null)
        {
            setItem.Header = item;
        }

        if (setItem.HeaderTemplate == null)
        {
            var headertemplatebinding = new Binding()
            {
                Source = this,
                Path = new PropertyPath(nameof(ItemHeaderTemplate)),
                Mode = BindingMode.OneWay
            };
            setItem.SetBinding(SetsViewItem.HeaderTemplateProperty, headertemplatebinding);
        }

        if (setItem.IsClosable != true && setItem.ReadLocalValue(SetsViewItem.IsClosableProperty) ==
            DependencyProperty.UnsetValue)
        {
            var iscloseablebinding = new Binding()
            {
                Source = this,
                Path = new PropertyPath(nameof(CanCloseSets)),
                Mode = BindingMode.OneWay,
            };
            setItem.SetBinding(SetsViewItem.IsClosableProperty, iscloseablebinding);
        }
    }

    private void SetItem_PointerEntered(object sender, PointerRoutedEventArgs e)
    {
        if (sender is SetsViewItem set)
        {
            set.HideRightSideSeparator();
            var index = IndexFromContainer(set);
            if (index > 0)
            {
                (ContainerFromIndex(index - 1) as SetsViewItem)?.HideRightSideSeparator();
            }

            set.HideLeftSideSeparator();
        }
    }

    private void SetItem_PointerExited(object sender, PointerRoutedEventArgs e)
    {
        if (sender is SetsViewItem set)
        {
            var index = IndexFromContainer(set);
            UpdateSetSeparator(index);
            UpdateSetSeparator(index - 1);
        }
    }

    private void SetsViewItem_DoubleTapped(object sender, DoubleTappedRoutedEventArgs e)
    {
        var args = new SetSelectedEventArgs(((SetsViewItem)ContainerFromItem(SelectedItem))?.Content, (SetsViewItem)ContainerFromItem(SelectedItem));
        SetDoubleTapped?.Invoke(sender, args);
    }

    private void SetsViewItem_Tapped(object sender, TappedRoutedEventArgs e)
    {
        var args = new SetSelectedEventArgs(((SetsViewItem)ContainerFromItem(SelectedItem))?.Content, (SetsViewItem)ContainerFromItem(SelectedItem));
        SetTapped?.Invoke(sender, args);
    }

    private void SetsViewItem_Loaded(object sender, RoutedEventArgs e)
    {
        var setItem = sender as SetsViewItem;

        setItem.Loaded -= SetsViewItem_Loaded;

        // Only need to do this once.
        if (!_hasLoaded)
        {
            _hasLoaded = true;

            // Need to set a set's selection on load, otherwise ListView resets to null.
            SetInitialSelection();

            // Need to make sure ContentPresenter is set to content based on selection.
            SetsView_SelectionChanged(this, null);

            // Need to make sure we've registered our removal method.
            ItemsSource_PropertyChanged(this, null);

            // Make sure we complete layout now.
            SetsView_SizeChanged(this, null);
        }
    }

    private void SetsViewItem_Closing(object sender, SetClosingEventArgs e)
    {
        var item = ItemFromContainer(e.Set);

        var args = new SetClosingEventArgs(item, e.Set);
        SetClosing?.Invoke(this, args);

        if (!args.Cancel)
        {
            if (ItemsSource == null)
            {
                e.Set.PrepareForClosing();
                Items?.Remove(item);
            }
            else if (_mutableItemsSource is { IsReadOnly: false, IsFixedSize: false })
            {
                e.Set.PrepareForClosing();
                _mutableItemsSource.Remove(item);
            }
        }
    }

    private void SetsView_DragItemsStarting(object sender, DragItemsStartingEventArgs e)
    {
        // Admission can throw; keep the framework operation cancelled unless
        // every subscriber accepted it and the control owns its completion.
        e.Cancel = true;
        e.Cancel = !BeginNativeDrag(e.Items.FirstOrDefault(), e.Data);
    }

    private bool BeginNativeDrag(object item, DataPackage data)
    {
        if (_mouseDrag != null || _nativeDrag != null) return false;
        StopHeaderSettlement();
        // Keep track of drag so we don't modify content until done.
        _isDragging = true;

        if (_setsItemsScrollViewerLeftSideShadow != null) _setsItemsScrollViewerLeftSideShadow.Visibility = Visibility.Collapsed;
        if (_setsItemsScrollViewerRightSideShadow != null) _setsItemsScrollViewerRightSideShadow.Visibility = Visibility.Collapsed;
        var starting = new TabDragStartingEventArgs(Guid.NewGuid(), item, data);
        _nativeDrag = starting;
        bool accepted = false;
        Exception error = null;
        try
        {
            TabDragStarting?.Invoke(this, starting);
            accepted = !starting.Cancel;
            return accepted;
        }
        catch (Exception exception) { error = exception; throw; }
        finally
        {
            // A previous subscriber may already own a transfer reservation.
            // Refusal/exception must retire that operation as well as our latch.
            if (!accepted) CompleteNativeDrag(DataPackageOperation.None, error);
        }
    }

    private void SetsView_DragItemsCompleted(ListViewBase sender, DragItemsCompletedEventArgs args) =>
        CompleteNativeDrag(args.DropResult);

    private void CompleteNativeDrag(DataPackageOperation result, Exception error = null)
    {
        if (_nativeDrag is not { } drag) return;
        _nativeDrag = null;
        _isDragging = false;

        // None also means Escape or an invalid target; it never authorizes tear-off.
        SetsView_SelectionChanged(this, null);
        TabDragCompleted?.Invoke(this, new TabDragCompletedEventArgs(drag.OperationId, drag.Item, result, error));
    }

    private void SetsScrollViewer_ViewChanged(object sender, ScrollViewerViewChangedEventArgs e)
    {
        _scrollViewerHorizontalOffset = _setsScroller.HorizontalOffset;
        UpdateScrollViewerShadows();
        UpdateScrollViewerNavigateButtons();
        if (_mouseDrag is { Preview: true } drag) UpdateRail(drag);
    }

    public void ScrollToLastSet()
    {
        ScrollTo(double.MaxValue);
    }

    public void ScrollTo(double offset)
    {
        _pendingScrollItem = null;
        _pendingScrollOffset = offset;
        SchedulePendingScroll();
    }

    public void ScrollSetIntoView(SetsViewItem item)
    {
        _pendingScrollOffset = null;
        _pendingScrollItem = item;
        SchedulePendingScroll();
    }

    private void SchedulePendingScroll()
    {
        // New tabs can arrive after the scroller itself has loaded. Their header
        // sizes must settle before ChangeView clamps the requested offset.
        _scrollerReady = false;
        if (_setsScroller == null) return;
        _setsScroller.LayoutUpdated -= SetsScrollViewer_PendingScrollLayoutUpdated;
        _setsScroller.LayoutUpdated += SetsScrollViewer_PendingScrollLayoutUpdated;
        _setsScroller.InvalidateArrange();
    }

    private void ApplyPendingScroll()
    {
        if (_applyingScroll || !_scrollerReady || _setsScroller?.IsLoaded != true) return;
        _applyingScroll = true;
        try
        {
            while (_scrollerReady && _setsScroller?.IsLoaded == true &&
                (_pendingScrollOffset != null || _pendingScrollItem != null))
            {
                // Layout may issue a newer request. Read it afterward, and drain
                // requests made by ChangeView/ScrollIntoView before leaving the guard.
                _setsScroller.UpdateLayout();
                if (!_scrollerReady || _setsScroller?.IsLoaded != true) return;
                if (_pendingScrollOffset is double offset)
                {
                    // Preserve the requested position for checkpoints even before
                    // the asynchronous ViewChanged event publishes the actual offset.
                    _scrollViewerHorizontalOffset = Math.Clamp(offset, 0, _setsScroller.ScrollableWidth);
                    _pendingScrollOffset = null;
                    _setsScroller.ChangeView(offset, 0.0f, 1.0f, disableAnimation: true);
                }
                else if (_pendingScrollItem is { } item)
                {
                    _pendingScrollItem = null;
                    if (Items.Contains(item)) ScrollIntoView(item);
                }
            }
        }
        catch (Exception ex)
        {
            throw new InvalidOperationException("SetsView failed to apply its requested scroll position.", ex);
        }
        finally { _applyingScroll = false; }
    }

    // HACK: Simulate left most and right most (tab) edge shadow
    // since I am too lazy to figure out the "right way" to make shadow visible on scroll viewer edges
    /// TODO This method should be removed when better solution is available in the future
    private void UpdateScrollViewerShadows()
    {
        if (_setsItemsScrollViewerLeftSideShadow == null ||
            _setsItemsScrollViewerRightSideShadow == null)
        {
            return;
        }

        if (Items?.Count == 1)
        {
            _setsItemsScrollViewerLeftSideShadow.Visibility = Visibility.Visible;
            _setsItemsScrollViewerRightSideShadow.Visibility = Visibility.Visible;
            return;
        }

        if (SelectedIndex == 0)
        {
            if (Math.Abs(_scrollViewerHorizontalOffset) < 3)
            {
                _setsItemsScrollViewerLeftSideShadow.Visibility = Visibility.Visible;
                _setsItemsScrollViewerRightSideShadow.Visibility = Visibility.Collapsed;
                return;
            }
        }
        else if (SelectedIndex == Items?.Count - 1)
        {
            var offset = _setsScroller.ExtentWidth - _setsScroller.ViewportWidth - _scrollViewerHorizontalOffset;
            if (Math.Abs(offset) < 3)
            {
                _setsItemsScrollViewerLeftSideShadow.Visibility = Visibility.Collapsed;
                _setsItemsScrollViewerRightSideShadow.Visibility = Visibility.Visible;
                return;
            }
        }

        _setsItemsScrollViewerLeftSideShadow.Visibility = Visibility.Collapsed;
        _setsItemsScrollViewerRightSideShadow.Visibility = Visibility.Collapsed;
    }

    private void UpdateScrollViewerNavigateButtons()
    {
        if (Math.Abs(_setsScroller.HorizontalOffset - _setsScroller.ScrollableWidth) < 0.1)
        {
            _setsScrollBackButton.IsEnabled = true;
            _setsScrollForwardButton.IsEnabled = false;
        }
        else if (Math.Abs(_setsScroller.HorizontalOffset) < 0.1)
        {
            _setsScrollBackButton.IsEnabled = false;
            _setsScrollForwardButton.IsEnabled = true;
        }
        else
        {
            _setsScrollBackButton.IsEnabled = true;
            _setsScrollForwardButton.IsEnabled = true;
        }
    }
}
