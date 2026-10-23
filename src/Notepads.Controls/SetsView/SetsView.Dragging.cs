// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Diagnostics;
using CommunityToolkit.WinUI;
using Windows.ApplicationModel.DataTransfer;
using Windows.Devices.Input;
using Windows.Foundation;
using Windows.UI.Core;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;
using Windows.UI.Xaml.Controls.Primitives;
using Windows.UI.Xaml.Data;
using Windows.UI.Xaml.Input;
using Windows.UI.Xaml.Media;
using Windows.UI.Xaml.Media.Animation;

namespace Notepads.Controls;

public partial class SetsView
{
    private const string LocalDragProperty = "Notepads.TabRailOperation";
    private const int DragZIndex = 1_000_000; // UWP Canvas.ZIndex's supported maximum.
    private MouseTabDrag _mouseDrag;
    private TabDragStartingEventArgs _nativeDrag;
    private bool _railRendering;
    private bool _updatingRail;
    private long? _railFrameTime;
    private CoreWindow _railWindow;
    private FrameworkElement _railViewport;
    private HeaderSettlement _headerSettlement;

    public event EventHandler<TabDragStartingEventArgs> TabDragStarting;
    public event EventHandler<TabDragCompletedEventArgs> TabDragCompleted;
    /// <summary>A local header action finished; consumers may focus its selected content.</summary>
    public event EventHandler<SetSelectedEventArgs> SetInteractionCompleted;

    /// <summary>SelectionChanged observers must ignore intermediate collection updates during MoveSet.</summary>
    public bool IsReordering { get; private set; }

    private sealed class HeaderPlacement(SetsViewItem item, double start, double width)
    {
        internal readonly SetsViewItem Item = item;
        internal readonly double Start = start;
        internal readonly double Width = width;
        private readonly TranslateTransform _translation = item.HeaderDragTranslation;
        private readonly double _original = item.HeaderDragTranslation.X;
        private readonly TranslateTransform _layout = item.HeaderLayoutTranslation;
        private readonly double _originalLayout = item.HeaderLayoutTranslation.X;
        private double _target;
        private Storyboard _animation;
        private readonly FrameworkElement _root = item.HeaderDragRoot;
        private double _layoutX;
        private double _axis;

        internal void Move(double offset, bool animate)
        {
            if (_target == offset) return;
            _target = offset;
            double from = _translation.X;
            _animation?.Stop();
            _translation.X = _original + offset;
            if (!animate) return;
            _animation = new Storyboard();
            _animation.Children.Add(Movement(_translation, from, _original + offset));
            _animation.Begin();
        }

        private static DoubleAnimation Movement(TranslateTransform translation, double from, double to)
        {
            var movement = new DoubleAnimation
            {
                From = from, To = to, Duration = TimeSpan.FromMilliseconds(90),
                EasingFunction = new CubicEase { EasingMode = EasingMode.EaseOut }
            };
            Storyboard.SetTarget(movement, translation);
            Storyboard.SetTargetProperty(movement, "X");
            return movement;
        }

        internal void Capture(UIElement relativeTo)
        {
            _layoutX = Item.TransformToVisual(relativeTo).TransformPoint(default).X;
            var parent = (UIElement)VisualTreeHelper.GetParent(_root);
            var transform = parent.TransformToVisual(relativeTo);
            _axis = transform.TransformPoint(new Point(1, 0)).X - transform.TransformPoint(default).X;
        }

        internal void Settle(UIElement relativeTo, Storyboard animation)
        {
            double finalLayoutX = Item.TransformToVisual(relativeTo).TransformPoint(default).X;
            double from = _originalLayout + (_layoutX - finalLayoutX) / _axis;
            double to = _originalLayout - _target;
            if (!double.IsFinite(from)) return;
            // Keep the neighbor's compositor clock running. Reading its animated
            // X on the UI thread can lag the displayed frame and replay motion.
            // A separate translation rebases the new slot without changing that
            // clock, then settles any remaining distance (including rounding).
            _layout.X = to;
            if (Math.Abs(from - to) > .01)
                animation.Children.Add(Movement(_layout, from, to));
        }

        internal void Restore()
        {
            _animation?.Stop();
            _animation = null;
            _translation.X = _original;
            _layout.X = _originalLayout;
            _target = 0;
        }
    }

    private sealed class HeaderSettlement(SetsView owner, MouseTabDrag drag)
    {
        internal readonly HeaderPlacement[] Headers = drag?.Headers ?? [];
        internal readonly SetsViewItem Item = drag?.Item;
        internal readonly int OriginalZ = drag?.OriginalZ ?? 0;
        internal readonly bool Animate = drag?.Animate == true;
        // Even an aligned source may have neighbors finishing their old preview.
        internal readonly Storyboard Animation = new() { Duration = TimeSpan.FromMilliseconds(90) };
        private readonly object _transitions = owner.ReadLocalValue(ItemContainerTransitionsProperty);
        private readonly Binding _binding = owner.GetBindingExpression(ItemContainerTransitionsProperty)?.ParentBinding;
        private bool _transitionsRestored;

        internal void RestoreTransitions(SetsView owner)
        {
            if (_transitionsRestored) return;
            _transitionsRestored = true;
            if (_binding != null) owner.SetBinding(ItemContainerTransitionsProperty, _binding);
            else if (_transitions == DependencyProperty.UnsetValue) owner.ClearValue(ItemContainerTransitionsProperty);
            else owner.SetValue(ItemContainerTransitionsProperty, _transitions);
        }
    }

    private sealed class MouseTabDrag(SetsViewItem item, Pointer pointer, Point pressed)
    {
        internal readonly SetsViewItem Item = item;
        internal readonly Pointer Pointer = pointer;
        internal readonly Point Pressed = pressed;
        internal readonly Guid Operation = Guid.NewGuid();
        internal readonly int OriginalZ = Canvas.GetZIndex(item);
        internal Point Position = pressed;
        internal HeaderPlacement[] Headers;
        internal double[] Centers;
        internal int Source;
        internal int Target;
        internal double Grab;
        internal double Direction;
        internal bool Started;
        internal bool External;
        internal bool Retired;
        internal bool Dirty = true;
        internal bool Animate;
        internal bool Preview;
        internal bool LocalDrop;
        internal bool Measured;
    }

    private void InitializeTabDragging()
    {
        PreviewKeyDown += RailKeyDown;
        Loaded += RailLoaded;
        Unloaded += RailUnloaded;
        RegisterPropertyChangedCallback(FlowDirectionProperty, (_, _) => InvalidateTabDrag());
    }

    private void RailLoaded(object sender, RoutedEventArgs args)
    {
        if (_railWindow != null) _railWindow.Activated -= RailWindowActivated;
        _railWindow = Window.Current.CoreWindow;
        _railWindow.Activated += RailWindowActivated;
    }

    private void RailUnloaded(object sender, RoutedEventArgs args)
    {
        if (_railWindow != null) _railWindow.Activated -= RailWindowActivated;
        _railWindow = null;
        InvalidateTabDrag();
    }

    private void RailWindowActivated(CoreWindow sender, WindowActivatedEventArgs args)
    {
        if (args.WindowActivationState != CoreWindowActivationState.Deactivated) return;
        StopHeaderSettlement();
        if (_mouseDrag is { External: true } drag) HideRail(drag);
        else ResetMouseDrag();
    }

    internal bool TryStartMouseDrag(SetsViewItem item, PointerRoutedEventArgs args)
    {
        if (_mouseDrag != null || _nativeDrag != null || !CanDragItems || !CanReorderItems || ItemsSource != null ||
            SelectionMode != ListViewSelectionMode.Single || !IsEnabled || !item.IsEnabled ||
            item.HeaderDragTranslation == null || item.HeaderLayoutTranslation == null || item.HeaderDragRoot == null || _setsScroller == null || _setItemsPresenter == null ||
            args.Pointer.PointerDeviceType != PointerDeviceType.Mouse) return false;
        var point = args.GetCurrentPoint(this);
        if (!point.Properties.IsLeftButtonPressed || point.Properties.IsMiddleButtonPressed || point.Properties.IsRightButtonPressed)
            return false;
        for (var node = args.OriginalSource as DependencyObject; node != null && node != item;
            node = VisualTreeHelper.GetParent(node))
            if (node is ButtonBase) return false;
        StopHeaderSettlement();
        if (!CapturePointer(args.Pointer)) return false;
        var drag = new MouseTabDrag(item, args.Pointer, point.Position);
        _mouseDrag = drag;
        SelectedItem = item;
        // Pointer capture does not require keyboard focus. Let the consumer
        // focus the content when this local user interaction completes.
        if (_mouseDrag != drag || Items.IndexOf(item) < 0) { ResetMouseDrag(); return true; }
        item.DragStarting += MouseDragStarting;
        args.Handled = true;
        return true;
    }

    protected override void OnPointerMoved(PointerRoutedEventArgs args)
    {
        if (_mouseDrag is not { External: false } drag || args.Pointer.PointerId != drag.Pointer.PointerId)
        { base.OnPointerMoved(args); return; }
        var point = args.GetCurrentPoint(this);
        if (!point.Properties.IsLeftButtonPressed) { ResetMouseDrag(); return; }
        drag.Position = point.Position;
        if (!drag.Started && !TabDragGeometry.HasStarted(point.Position.X - drag.Pressed.X, point.Position.Y - drag.Pressed.Y))
        { args.Handled = true; return; }
        if (!drag.Started)
        {
            drag.Started = true;
            drag.Animate = new UISettings().AnimationsEnabled;
            _isDragging = true;
            Canvas.SetZIndex(drag.Item, DragZIndex);
        }
        if (!TryGetHeaderBounds(out var bounds)) { ResetMouseDrag(); return; }
        if (TabDragGeometry.Escapes(point.Position.Y, bounds.Top, bounds.Bottom))
        {
            // Windows owns completion from this point, including expected capture loss.
            drag.External = true;
            HideRail(drag);
            var start = args.GetCurrentPoint(drag.Item);
            ReleasePointerCapture(drag.Pointer);
            _ = RunExternalDragAsync(drag, start);
        }
        else UpdateRail(drag);
        args.Handled = true;
    }

    private async System.Threading.Tasks.Task RunExternalDragAsync(MouseTabDrag drag, Windows.UI.Input.PointerPoint point)
    {
        var result = DataPackageOperation.None;
        Exception error = null;
        try { result = await drag.Item.StartDragAsync(point); }
        catch (Exception exception) { error = exception; }
        finally
        {
            ResetMouseDrag();
            TabDragCompleted?.Invoke(this, new TabDragCompletedEventArgs(drag.Operation, drag.Item, result, error));
        }
    }

    protected override void OnPointerReleased(PointerRoutedEventArgs args)
    {
        if (_mouseDrag is not { External: false } drag || args.Pointer.PointerId != drag.Pointer.PointerId)
        { base.OnPointerReleased(args); return; }
        drag.Position = args.GetCurrentPoint(this).Position;
        bool commit = drag.Started && TryGetHeaderBounds(out var bounds) &&
            !TabDragGeometry.Escapes(drag.Position.Y, bounds.Top, bounds.Bottom);
        if (commit) UpdateRail(drag);
        commit &= drag.Preview && !drag.Retired;
        if (commit) CommitRail(drag);
        else ResetMouseDrag();
        args.Handled = true;
        CompleteHeaderInteraction(drag.Item);
    }

    protected override void OnPointerCaptureLost(PointerRoutedEventArgs args)
    {
        if (_mouseDrag is { External: false } drag && args.Pointer.PointerId == drag.Pointer.PointerId) ResetMouseDrag();
        base.OnPointerCaptureLost(args);
    }

    protected override void OnPointerCanceled(PointerRoutedEventArgs args)
    {
        if (_mouseDrag is { External: false } drag && args.Pointer.PointerId == drag.Pointer.PointerId) ResetMouseDrag();
        base.OnPointerCanceled(args);
    }

    private void RailKeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (_mouseDrag is { External: false } drag && args.Key == Windows.System.VirtualKey.Escape)
        { ResetMouseDrag(); args.Handled = true; CompleteHeaderInteraction(drag.Item); }
    }

    private void CompleteHeaderInteraction(SetsViewItem item)
    {
        // Capture loss, deactivation and unloading also reset gestures. They
        // deliberately do not request focus; only local user completion does.
        if (IsLoaded && IsEnabled && ReferenceEquals(SelectedItem, item) && Items.Contains(item))
            SetInteractionCompleted?.Invoke(this, new SetSelectedEventArgs(item.Content, item));
    }

    private void MouseDragStarting(UIElement sender, DragStartingEventArgs args)
    {
        if (_mouseDrag is not { External: true, Retired: false } drag || sender != drag.Item)
        { args.Cancel = true; return; }
        args.Data.Properties[LocalDragProperty] = drag.Operation.ToString("N");
        args.AllowedOperations = DataPackageOperation.Move;
        var starting = new TabDragStartingEventArgs(drag.Operation, drag.Item, args.Data);
        TabDragStarting?.Invoke(this, starting);
        args.Cancel = starting.Cancel;
    }

    private bool IsMouseOperation(DragEventArgs args) => _mouseDrag is { External: true } drag &&
        args.DataView.Properties.TryGetValue(LocalDragProperty, out var operation) && Equals(operation, drag.Operation.ToString("N"));

    protected override void OnDragOver(DragEventArgs args)
    {
        if (!IsMouseOperation(args)) { base.OnDragOver(args); return; }
        var drag = _mouseDrag;
        drag.Position = args.GetPosition(this);
        bool inside = !drag.Retired && TryGetHeaderBounds(out var bounds) && bounds.Contains(drag.Position);
        args.Handled = true;
        args.AcceptedOperation = inside ? DataPackageOperation.Move : DataPackageOperation.None;
        args.DragUIOverride.IsContentVisible = !inside;
        args.DragUIOverride.IsCaptionVisible = false;
        args.DragUIOverride.IsGlyphVisible = false;
        if (inside) UpdateRail(drag);
        else HideRail(drag);
    }

    protected override void OnDragLeave(DragEventArgs args)
    {
        if (!IsMouseOperation(args)) { base.OnDragLeave(args); return; }
        HideRail(_mouseDrag);
        args.DragUIOverride.IsContentVisible = true;
        args.Handled = true;
    }

    protected override void OnDrop(DragEventArgs args)
    {
        if (!IsMouseOperation(args)) { base.OnDrop(args); return; }
        var drag = _mouseDrag;
        drag.Position = args.GetPosition(this);
        args.Handled = true;
        args.AcceptedOperation = DataPackageOperation.None;
        if (!drag.Retired && !drag.LocalDrop && TryGetHeaderBounds(out var bounds) && bounds.Contains(drag.Position))
        {
            UpdateRail(drag);
            if (drag.Preview)
            {
                drag.LocalDrop = true;
                bool committed = CommitRail(drag);
                drag.Retired = true;
                if (committed)
                {
                    args.AcceptedOperation = DataPackageOperation.Move;
                    CompleteHeaderInteraction(drag.Item);
                }
            }
        }
        // The local operation never requests the durable transfer's data provider.
    }

    private bool TryGetHeaderBounds(out Rect bounds)
    {
        bounds = default;
        if (_setsScroller == null || !_setsScroller.IsLoaded || _setsScroller.ActualHeight <= 0) return false;
        bounds = _setsScroller.TransformToVisual(this).TransformBounds(new Rect(0, 0, _setsScroller.ActualWidth, _setsScroller.ActualHeight));
        return bounds.Width > 0 && bounds.Height > 0;
    }

    private bool MeasureRail(MouseTabDrag drag)
    {
        if (_setItemsPresenter == null || !_setItemsPresenter.IsLoaded || Items.Count == 0) return false;
        if (drag.Headers != null) foreach (var entry in drag.Headers) entry?.Restore();
        drag.Headers = new HeaderPlacement[Items.Count];
        drag.Centers = new double[Items.Count];
        drag.Source = Items.IndexOf(drag.Item);
        // TransformToVisual already applies UWP's inherited FlowDirection.
        // Measure the presenter's actual axis rather than mirroring it twice.
        drag.Direction = 1;
        if (drag.Source < 0) return false;
        for (int index = 0; index < Items.Count; index++)
        {
            if (ContainerFromIndex(index) is not SetsViewItem item || item.HeaderDragTranslation == null || item.HeaderLayoutTranslation == null ||
                item.HeaderDragRoot == null || item.ActualWidth <= 0)
                return false;
            var rect = item.TransformToVisual(_setItemsPresenter).TransformBounds(new Rect(0, 0, item.ActualWidth, item.ActualHeight));
            if (index == 1 && rect.Left < drag.Headers[0].Start)
            {
                drag.Direction = -1;
                var first = drag.Headers[0];
                drag.Headers[0] = new HeaderPlacement(first.Item, -first.Start - first.Width, first.Width);
                drag.Centers[0] = drag.Headers[0].Start + first.Width / 2;
            }
            double start = drag.Direction > 0 ? rect.Left : -rect.Right;
            if (index > 0 && start < drag.Headers[index - 1].Start) return false;
            drag.Headers[index] = new HeaderPlacement(item, start, rect.Width);
            drag.Centers[index] = start + rect.Width / 2;
        }
        var source = drag.Headers[drag.Source];
        var press = TransformToVisual(_setItemsPresenter).TransformPoint(drag.Pressed);
        drag.Grab = Math.Clamp(drag.Measured ? drag.Grab : drag.Direction * press.X - source.Start, 0, source.Width);
        drag.Measured = true;
        drag.Dirty = false;
        return true;
    }

    private void UpdateRail(MouseTabDrag drag)
    {
        if (_updatingRail || drag.Retired || _mouseDrag != drag) return;
        _updatingRail = true;
        try
        {
            if (drag.Dirty && !MeasureRail(drag)) { InvalidateTabDrag(); return; }
            if (!drag.Preview)
            {
                Canvas.SetZIndex(drag.Item, DragZIndex);
                if (drag.Item.HeaderDragBackground != null) drag.Item.HeaderDragBackground.Visibility = Visibility.Visible;
            }
            var point = TransformToVisual(_setItemsPresenter).TransformPoint(drag.Position);
            var source = drag.Headers[drag.Source];
            var last = drag.Headers[^1];
            double start = TabDragGeometry.SourceStart(drag.Direction * point.X, drag.Grab,
                drag.Headers[0].Start, last.Start + last.Width, source.Width);
            drag.Target = TabDragGeometry.TargetIndex(drag.Centers, drag.Source, start, source.Width);
            for (int index = 0; index < drag.Headers.Length; index++)
                drag.Headers[index].Move(drag.Direction * (index == drag.Source ? start - source.Start :
                    TabDragGeometry.NeighborOffset(index, drag.Source, drag.Target, source.Width)), index != drag.Source && drag.Animate);
            drag.Preview = true;
            if (RailEdgeVelocity(drag) != 0) StartRailRendering();
            else StopRailRendering();
        }
        finally { _updatingRail = false; }
    }

    private double RailEdgeVelocity(MouseTabDrag drag)
    {
        if (!TryGetHeaderBounds(out var band) || drag.Position.Y < band.Top || drag.Position.Y > band.Bottom ||
            _setsScroller.ScrollableWidth <= 0) return 0;
        var viewport = _railViewport ??= _setsScroller.FindDescendant("ScrollContentPresenter") as FrameworkElement;
        if (viewport == null) return 0;
        var bounds = viewport.TransformToVisual(this).TransformBounds(new Rect(0, 0, viewport.ActualWidth, viewport.ActualHeight));
        double velocity = TabDragGeometry.EdgeVelocity(drag.Position.X, bounds.Left, bounds.Right);
        double offset = _setsScroller.HorizontalOffset;
        return velocity < 0 && offset <= 0 || velocity > 0 && offset >= _setsScroller.ScrollableWidth ? 0 : velocity;
    }

    private void StartRailRendering()
    {
        if (_railRendering) return;
        _railRendering = true;
        _railFrameTime = null;
        CompositionTarget.Rendering += RailRendering;
    }

    private void StopRailRendering()
    {
        if (!_railRendering) return;
        CompositionTarget.Rendering -= RailRendering;
        _railRendering = false;
        _railFrameTime = null;
    }

    private void RailRendering(object sender, object args)
    {
        if (_mouseDrag is not { Started: true, Retired: false } drag || !drag.Preview)
        { StopRailRendering(); return; }
        var now = Stopwatch.GetTimestamp();
        double elapsed = _railFrameTime is long previous ? Math.Clamp(Stopwatch.GetElapsedTime(previous, now).TotalSeconds, 0, .05) : 0;
        _railFrameTime = now;
        double velocity = RailEdgeVelocity(drag);
        if (velocity != 0 && elapsed > 0)
            _setsScroller.ChangeView(Math.Clamp(_setsScroller.HorizontalOffset + velocity * elapsed, 0, _setsScroller.ScrollableWidth), null, null, true);
        UpdateRail(drag);
    }

    private void HideRail(MouseTabDrag drag)
    {
        StopRailRendering();
        if (drag.Headers != null) foreach (var entry in drag.Headers) entry?.Restore();
        if (!ReferenceEquals(_headerSettlement?.Item, drag.Item)) RestoreHeaderAppearance(drag.Item, drag.OriginalZ);
        drag.Preview = false;
    }

    private static void RestoreHeaderAppearance(SetsViewItem item, int originalZ)
    {
        Canvas.SetZIndex(item, originalZ);
        if (item.HeaderDragBackground != null) item.HeaderDragBackground.Visibility = Visibility.Collapsed;
    }

    private bool CommitRail(MouseTabDrag drag) => CommitHeaderMove(drag.Item, drag.Target, drag);

    private bool CommitHeaderMove(SetsViewItem item, int index, MouseTabDrag drag = null)
    {
        if (IsReordering || (drag == null && (_mouseDrag != null || _nativeDrag != null))) return false;
        StopRailRendering();
        StopHeaderSettlement();
        var settlement = new HeaderSettlement(this, drag);
        foreach (var header in settlement.Headers) header.Capture(_setItemsPresenter);
        _headerSettlement = settlement;
        if (drag != null)
        {
            drag.Headers = null; // Settlement now owns the retained preview transforms.
            drag.Preview = false;
            if (!drag.External) ResetMouseDrag();
        }

        // Retain preview positions through the normal arrange pass. Forcing
        // UpdateLayout here exposes intermediate transforms to rendering.
        LayoutUpdated += HeaderSettlementLayoutUpdated;
        bool committed = false;
        try
        {
            ItemContainerTransitions = null;
            committed = MoveSetCore(item, index);
            // A no-op reorder has no arrange pass to wait for.
            if (committed) InvalidateArrange();
            return committed;
        }
        finally
        {
            if (!committed) StopHeaderSettlement();
        }
    }

    private void HeaderSettlementLayoutUpdated(object sender, object args)
    {
        LayoutUpdated -= HeaderSettlementLayoutUpdated;
        var settlement = _headerSettlement;
        if (settlement == null) return;
        bool animating = false;
        try
        {
            if (settlement.Animate)
                foreach (var header in settlement.Headers) header.Settle(_setItemsPresenter, settlement.Animation);
            if (ReferenceEquals(_headerSettlement, settlement) && settlement.Animate)
            {
                settlement.Animation.Completed += HeaderSettlementCompleted;
                settlement.Animation.Begin();
                animating = true;
            }
        }
        finally
        {
            // The native transition collection stays suppressed for the whole
            // settlement, including compositor frames after this arrange pass.
            if (!animating && ReferenceEquals(_headerSettlement, settlement)) StopHeaderSettlement();
        }
    }

    private void HeaderSettlementCompleted(object sender, object args)
    {
        if (ReferenceEquals(_headerSettlement?.Animation, sender)) StopHeaderSettlement();
    }

    private void StopHeaderSettlement()
    {
        var settlement = _headerSettlement;
        if (settlement == null) return;
        _headerSettlement = null;
        LayoutUpdated -= HeaderSettlementLayoutUpdated;
        settlement.Animation.Completed -= HeaderSettlementCompleted;
        settlement.Animation.Stop();
        foreach (var header in settlement.Headers) header.Restore();
        if (settlement.Item != null) RestoreHeaderAppearance(settlement.Item, settlement.OriginalZ);
        settlement.RestoreTransitions(this);
    }

    private void ResetMouseDrag()
    {
        var drag = _mouseDrag;
        if (drag == null) return;
        _mouseDrag = null; // Releasing capture can synchronously reenter this control.
        HideRail(drag);
        drag.Item.DragStarting -= MouseDragStarting;
        _isDragging = false;
        ReleasePointerCapture(drag.Pointer);
        if (drag.Started && IsLoaded) SetsView_SelectionChanged(this, null);
    }

    private void InvalidateTabDrag()
    {
        StopHeaderSettlement();
        if (_mouseDrag is not { } drag) return;
        if (!drag.External) ResetMouseDrag();
        else { drag.Retired = true; HideRail(drag); }
    }

    internal void InvalidateTabHeader() => InvalidateTabDrag();

    private void InvalidateRailLayout()
    {
        if (_mouseDrag is not { Retired: false } drag) return;
        drag.Dirty = true;
        if (drag.Preview) StartRailRendering();
    }

    /// <summary>Reorders a header without detaching the selected document or exposing transient selection.</summary>
    public bool MoveSet(SetsViewItem item, int index) => CommitHeaderMove(item, index);

    private bool MoveSetCore(SetsViewItem item, int index)
    {
        if (IsReordering || ItemsSource != null || index < 0 || index >= Items.Count) return false;
        int oldIndex = Items.IndexOf(item);
        if (oldIndex < 0) return false;
        if (oldIndex == index) return true;
        var selected = SelectedItem;
        bool wasDragging = _isDragging;
        IsReordering = true;
        _isDragging = true;
        try
        {
            Items.RemoveAt(oldIndex);
            Items.Insert(index, item);
            SelectedItem = selected;
        }
        finally
        {
            IsReordering = false;
            _isDragging = wasDragging;
            if (!wasDragging) SetsView_SelectionChanged(this, null);
            SetsView_SizeChanged(this, null);
        }
        return true;
    }
}
