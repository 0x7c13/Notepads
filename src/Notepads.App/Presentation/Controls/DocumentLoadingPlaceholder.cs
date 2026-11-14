// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

using System;
using System.Collections.Generic;
using System.Numerics;
using Windows.ApplicationModel.Resources;
using Windows.Foundation;
using Windows.UI;
using Windows.UI.Composition;
using Windows.UI.Core;
using Windows.UI.ViewManagement;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Automation;
using Windows.UI.Xaml.Automation.Peers;
using Windows.UI.Xaml.Hosting;

namespace Notepads.Presentation.Controls;

/// <summary>A decorative document outline; only a presented tab owns compositor resources.</summary>
public sealed partial class DocumentLoadingPlaceholder : FrameworkElement
{
    private const int MaximumLines = 128;
    private const int EstimatedBytesPerLine = 96;
    private const float LineSpacing = 28;
    private const float LineHeight = 12;
    private static readonly float[] LineWidths = [0.88f, 0.66f, 0.78f, 0.94f, 0.57f, 0.74f, 0.85f, 0.46f];
    private readonly int _estimatedLines = MaximumLines;
    private readonly bool _rightToLeft;
    private readonly List<CompositionRoundedRectangleGeometry> _lines = [];

    private UISettings _settings;
    private AccessibilitySettings _accessibility;
    private Window _window;
    private ShapeVisual _visual;
    private CompositionLinearGradientBrush _brush;

    public DocumentLoadingPlaceholder()
    {
        Margin = new Thickness(28, 24, 28, 0);
        VerticalAlignment = VerticalAlignment.Top;
        IsHitTestVisible = false;
        // Like the editor, use document direction without mirroring the XAML canvas.
        FlowDirection = FlowDirection.LeftToRight;
        AutomationProperties.SetName(this, ResourceLoader.GetForCurrentView().GetString("WorkspaceTab_Loading"));
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
        SizeChanged += (_, _) => { UpdateGeometry(); UpdateAnimation(); };
        ActualThemeChanged += (_, _) => UpdateAppearance();
        RegisterPropertyChangedCallback(VisibilityProperty, (_, _) => UpdateAnimation());
    }

    internal DocumentLoadingPlaceholder(long? documentByteLength, bool rightToLeft) : this()
    {
        _rightToLeft = rightToLeft;
        // A visual estimate from existing UTF-8 recovery metadata, never a line-count scan.
        // Legacy sources without a size use the visible viewport as their bound.
        if (documentByteLength is long bytes)
            _estimatedLines = (int)Math.Clamp(bytes / EstimatedBytesPerLine +
                (bytes % EstimatedBytesPerLine > 0 ? 1 : 0), 1, MaximumLines);
    }

    protected override AutomationPeer OnCreateAutomationPeer() => new LoadingPeer(this);

    protected override Size MeasureOverride(Size availableSize) => new(
        Math.Min(720, availableSize.Width),
        Math.Min((_estimatedLines - 1) * LineSpacing + LineHeight, availableSize.Height));

    private sealed partial class LoadingPeer(DocumentLoadingPlaceholder owner) : FrameworkElementAutomationPeer(owner)
    {
        protected override AutomationControlType GetAutomationControlTypeCore() => AutomationControlType.ProgressBar;
        protected override string GetClassNameCore() => nameof(DocumentLoadingPlaceholder);
    }

    private void OnLoaded(object sender, RoutedEventArgs args)
    {
        if (_visual != null) return;
        _settings = new UISettings();
        _accessibility = new AccessibilitySettings();
        _window = Window.Current;
        var compositor = ElementCompositionPreview.GetElementVisual(this).Compositor;
        _visual = compositor.CreateShapeVisual();
        _visual.Clip = compositor.CreateInsetClip();
        _brush = compositor.CreateLinearGradientBrush();
        _brush.MappingMode = CompositionMappingMode.Absolute;
        for (var index = 0; index < 3; index++)
            _brush.ColorStops.Add(compositor.CreateColorGradientStop(index / 2f, Colors.Transparent));
        ElementCompositionPreview.SetElementChildVisual(this, _visual);
        _window.VisibilityChanged += OnWindowVisibilityChanged;
        _accessibility.HighContrastChanged += OnHighContrastChanged;
        if (OperatingSystem.IsWindowsVersionAtLeast(10, 0, 19041))
            _settings.AnimationsEnabledChanged += OnAnimationsEnabledChanged;
        UpdateGeometry();
        UpdateAppearance();
    }

    private void OnUnloaded(object sender, RoutedEventArgs args)
    {
        if (_visual == null) return;
        _window.VisibilityChanged -= OnWindowVisibilityChanged;
        _accessibility.HighContrastChanged -= OnHighContrastChanged;
        if (OperatingSystem.IsWindowsVersionAtLeast(10, 0, 19041))
            _settings.AnimationsEnabledChanged -= OnAnimationsEnabledChanged;
        _brush.StopAnimation("Offset.X");
        ElementCompositionPreview.SetElementChildVisual(this, null);
        ResizeLines(0);
        foreach (var stop in _brush.ColorStops) stop.Dispose();
        _visual.Clip.Dispose();
        _visual.Dispose();
        _brush.Dispose();
        _visual = null;
        _brush = null;
        _window = null;
        _settings = null;
        _accessibility = null;
    }

    private void UpdateGeometry()
    {
        if (_visual == null) return;
        var width = (float)ActualWidth;
        _visual.Size = new Vector2(width, (float)ActualHeight);
        var contentWidth = Math.Min(width, 720);
        var visibleLines = ActualHeight <= 0 ? 0 :
            (int)Math.Min(MaximumLines, 1 + Math.Floor(Math.Max(0, ActualHeight - LineHeight) / LineSpacing));
        ResizeLines(Math.Min(_estimatedLines, visibleLines));
        for (var index = 0; index < _lines.Count; index++)
        {
            var length = contentWidth * LineWidths[index % LineWidths.Length];
            _lines[index].Size = new Vector2(length, LineHeight);
            _lines[index].Offset = new Vector2(_rightToLeft ? width - length : 0, index * LineSpacing);
        }
        _brush.EndPoint = new Vector2(Math.Max(1, width), 0);
    }

    private void ResizeLines(int count)
    {
        while (_lines.Count > count)
        {
            var index = _lines.Count - 1;
            var shape = _visual.Shapes[index];
            _visual.Shapes.RemoveAt(index);
            shape.Dispose();
            _lines[index].Dispose();
            _lines.RemoveAt(index);
        }
        while (_lines.Count < count)
        {
            var geometry = _visual.Compositor.CreateRoundedRectangleGeometry();
            geometry.CornerRadius = new Vector2(3);
            _lines.Add(geometry);
            var shape = _visual.Compositor.CreateSpriteShape(geometry);
            shape.FillBrush = _brush;
            _visual.Shapes.Add(shape);
        }
    }

    private void UpdateAppearance()
    {
        if (_brush == null) return;
        var foreground = _accessibility.HighContrast ? _settings.GetColorValue(UIColorType.Foreground) :
            ActualTheme == ElementTheme.Dark ? Colors.White : Colors.Black;
        var low = foreground;
        var high = foreground;
        if (!_accessibility.HighContrast) { low.A = 14; high.A = 22; }
        _brush.ColorStops[0].Color = _brush.ColorStops[2].Color = low;
        _brush.ColorStops[1].Color = high;
        UpdateAnimation();
    }

    private void UpdateAnimation()
    {
        if (_brush == null) return;
        _brush.StopAnimation("Offset.X");
        _brush.Offset = Vector2.Zero;
        if (ActualWidth <= 0 || Visibility != Visibility.Visible || !_window.Visible ||
            _accessibility.HighContrast || !_settings.AnimationsEnabled) return;
        using var animation = _brush.Compositor.CreateScalarKeyFrameAnimation();
        using var linear = _brush.Compositor.CreateLinearEasingFunction();
        animation.InsertKeyFrame(0, -(float)ActualWidth, linear);
        animation.InsertKeyFrame(1, (float)ActualWidth, linear);
        animation.Duration = TimeSpan.FromSeconds(1.6);
        animation.IterationBehavior = AnimationIterationBehavior.Forever;
        _brush.StartAnimation("Offset.X", animation);
    }

    private void OnWindowVisibilityChanged(object sender, VisibilityChangedEventArgs args) => UpdateAnimation();

    private void OnAnimationsEnabledChanged(UISettings sender, UISettingsAnimationsEnabledChangedEventArgs args) =>
        QueueAppearanceUpdate();

    private void OnHighContrastChanged(AccessibilitySettings sender, object args) => QueueAppearanceUpdate();

    private void QueueAppearanceUpdate() => _ = Dispatcher.RunAsync(CoreDispatcherPriority.Normal, () =>
    {
        if (_visual != null) UpdateAppearance();
    });
}
