// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

namespace SetsView
{
    using System;
    using System.Linq;
    using Microsoft.Toolkit.Uwp.UI.Controls;
    using Microsoft.Toolkit.Uwp.UI.Extensions;
    using Windows.ApplicationModel.DataTransfer;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Data;
    using Windows.UI.Xaml.Input;

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

        private bool _hasLoaded;
        private bool _isDragging;

        private double _scrollViewerHorizontalOffset = .0f;
        public double ScrollViewerHorizontalOffset => _scrollViewerHorizontalOffset;

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
        }

        /// <summary>
        /// Occurs when a set is dragged by the user outside of the <see cref="SetsView"/>.  Generally, this paradigm is used to create a new-window with the torn-off set.
        /// The creation and handling of the new-window is left to the app's developer.
        /// </summary>
        public event EventHandler<SetDraggedOutsideEventArgs> SetDraggedOutside;

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
        protected override DependencyObject GetContainerForItemOverride() => new SetsViewItem();

        /// <inheritdoc/>
        protected override bool IsItemItsOwnContainerOverride(object item)
        {
            return item is SetsViewItem;
        }

        /// <inheritdoc/>
        protected override void OnApplyTemplate()
        {
            base.OnApplyTemplate();

            if (_setItemsPresenter != null)
            {
                _setItemsPresenter.SizeChanged -= SetsView_SizeChanged;
            }

            if (_setsScroller != null)
            {
                _setsScroller.Loaded -= SetsScrollViewer_Loaded;
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
            _setsScroller.Loaded -= SetsScrollViewer_Loaded;

            if (_setsScrollBackButton != null)
            {
                _setsScrollBackButton.Click -= ScrollSetBackButton_Click;
            }

            if (_setsScrollForwardButton != null)
            {
                _setsScrollForwardButton.Click -= ScrollSetForwardButton_Click;
            }

            _setsScrollBackButton = _setsScroller.FindDescendantByName(SetsScrollBackButtonName) as ButtonBase;
            _setsScrollForwardButton = _setsScroller.FindDescendantByName(SetsScrollForwardButtonName) as ButtonBase;
            _setsItemsScrollViewerLeftSideShadow = _setsScroller.FindDescendantByName(SetsItemsScrollViewerLeftSideShadowName) as DropShadowPanel;
            _setsItemsScrollViewerRightSideShadow = _setsScroller.FindDescendantByName(SetsItemsScrollViewerRightSideShadowName) as DropShadowPanel;

            if (_setsScrollBackButton != null)
            {
                _setsScrollBackButton.Click += ScrollSetBackButton_Click;
            }

            if (_setsScrollForwardButton != null)
            {
                _setsScrollForwardButton.Click += ScrollSetForwardButton_Click;
            }
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
            if (_isDragging)
            {
                // Skip if we're dragging, we'll reset when we're done.
                return;
            }

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
                        _setsContentPresenter.Content = container.Content;
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
            for (int i = 0; i < this.Items?.Count; i++)
            {
                if (i != SelectedIndex && i != SelectedIndex - 1)
                {
                    (ContainerFromIndex(i) as SetsViewItem)?.ShowRightSideSeparator();
                }
                else
                {
                    (ContainerFromIndex(i) as SetsViewItem)?.HideRightSideSeparator();
                }
            }
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

            var setItem = element as SetsViewItem;

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

            if (setItem.IsClosable != true && setItem.ReadLocalValue(SetsViewItem.IsClosableProperty) == DependencyProperty.UnsetValue)
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
            }
        }

        private void SetItem_PointerExited(object sender, PointerRoutedEventArgs e)
        {
            if (sender is SetsViewItem set)
            {
                var index = IndexFromContainer(set);

                if (SelectedIndex == index - 1)
                {
                    set.ShowRightSideSeparator();
                }
                else if (SelectedIndex == index + 1)
                {
                    if (index > 0)
                    {
                        (ContainerFromIndex(index - 1) as SetsViewItem)?.ShowRightSideSeparator();
                    }
                }
                else if (SelectedIndex != index)
                {
                    set.ShowRightSideSeparator();
                    if (index > 0)
                    {
                        (ContainerFromIndex(index - 1) as SetsViewItem)?.ShowRightSideSeparator();
                    }
                }
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
                if (ItemsSource != null)
                {
                    _removeItemsSourceMethod?.Invoke(ItemsSource, new object[] { item });
                }
                else
                {
                    Items?.Remove(item);
                }
            }
        }

        private void SetsView_DragItemsStarting(object sender, DragItemsStartingEventArgs e)
        {
            // Keep track of drag so we don't modify content until done.
            _isDragging = true;

            _setsItemsScrollViewerLeftSideShadow.Visibility = Visibility.Collapsed;
            _setsItemsScrollViewerRightSideShadow.Visibility = Visibility.Collapsed;
        }

        private void SetsView_DragItemsCompleted(ListViewBase sender, DragItemsCompletedEventArgs args)
        {
            _isDragging = false;

            // args.DropResult == None when outside of area (e.g. create new window)
            if (args.DropResult == DataPackageOperation.None)
            {
                var item = args.Items.FirstOrDefault();
                var set = ContainerFromItem(item) as SetsViewItem;

                if (set == null && item is FrameworkElement fe)
                {
                    set = fe.FindParent<SetsViewItem>();
                }

                if (set == null)
                {
                    // We still don't have a SetsViewItem, most likely is a static SetsViewItem in the template being dragged and not selected.
                    // This is a fallback scenario for static sets.
                    // Note: This can be wrong if two SetsViewItems share the exact same Content (i.e. a string), this should be unlikely in any practical scenario.
                    for (int i = 0; i < Items.Count; i++)
                    {
                        var setItem = ContainerFromIndex(i) as SetsViewItem;
                        if (ReferenceEquals(setItem.Content, item))
                        {
                            set = setItem;
                            break;
                        }
                    }
                }

                SetDraggedOutside?.Invoke(this, new SetDraggedOutsideEventArgs(item, set));

                UpdateScrollViewerShadows();
            }
            else
            {
                // If dragging the active set, there's an issue with the CP blanking.
                SetsView_SelectionChanged(this, null);
            }
        }

        private void SetsScrollViewer_ViewChanged(object sender, ScrollViewerViewChangedEventArgs e)
        {
            _scrollViewerHorizontalOffset = _setsScroller.HorizontalOffset;
            UpdateScrollViewerShadows();
            UpdateScrollViewerNavigateButtons();
        }

        public void ScrollToLastSet()
        {
            ScrollTo(double.MaxValue);
        }

        public void ScrollTo(double offset)
        {
            try
            {
                _setsScroller?.UpdateLayout();
                _setsScroller?.ChangeView(offset, 0.0f, 1.0f);
            }
            catch (Exception ex)
            {
                throw new Exception($"SetsView failed to scroll to offset: {(long)offset}, Exception: {ex}");
            }
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
}
