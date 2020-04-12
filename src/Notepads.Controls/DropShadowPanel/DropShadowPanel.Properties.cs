// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/DropShadowPanel

namespace Notepads.Controls
{
    using Windows.UI;
    using Windows.UI.Composition;
    using Windows.UI.Xaml;

    /// <summary>
    /// The <see cref="DropShadowPanel"/> control allows the creation of a DropShadow for any Xaml FrameworkElement in markup
    /// making it easier to add shadows to Xaml without having to directly drop down to Windows.UI.Composition APIs.
    /// </summary>
    public partial class DropShadowPanel
    {
        /// <summary>
        /// Identifies the <see cref="BlurRadius"/> dependency property.
        /// </summary>
        public static readonly DependencyProperty BlurRadiusProperty =
             DependencyProperty.Register(nameof(BlurRadius), typeof(double), typeof(DropShadowPanel), new PropertyMetadata(9.0, OnBlurRadiusChanged));

        /// <summary>
        /// Identifies the <see cref="Color"/> dependency property.
        /// </summary>
        public static readonly DependencyProperty ColorProperty =
            DependencyProperty.Register(nameof(Color), typeof(Color), typeof(DropShadowPanel), new PropertyMetadata(Colors.Black, OnColorChanged));

        /// <summary>
        /// Identifies the <see cref="OffsetX"/> dependency property.
        /// </summary>
        public static readonly DependencyProperty OffsetXProperty =
            DependencyProperty.Register(nameof(OffsetX), typeof(double), typeof(DropShadowPanel), new PropertyMetadata(0.0, OnOffsetXChanged));

        /// <summary>
        /// Identifies the <see cref="OffsetY"/> dependency property.
        /// </summary>
        public static readonly DependencyProperty OffsetYProperty =
            DependencyProperty.Register(nameof(OffsetY), typeof(double), typeof(DropShadowPanel), new PropertyMetadata(0.0, OnOffsetYChanged));

        /// <summary>
        /// Identifies the <see cref="OffsetZ"/> dependency property.
        /// </summary>
        public static readonly DependencyProperty OffsetZProperty =
            DependencyProperty.Register(nameof(OffsetZ), typeof(double), typeof(DropShadowPanel), new PropertyMetadata(0.0, OnOffsetZChanged));

        /// <summary>
        /// Identifies the <see cref="ShadowOpacity"/> dependency property.
        /// </summary>
        public static readonly DependencyProperty ShadowOpacityProperty =
            DependencyProperty.Register(nameof(ShadowOpacity), typeof(double), typeof(DropShadowPanel), new PropertyMetadata(1.0, OnShadowOpacityChanged));

        /// <summary>
        /// Identifies the <see cref="IsMasked"/> dependency property.
        /// </summary>
        public static readonly DependencyProperty IsMaskedProperty =
            DependencyProperty.Register(nameof(IsMasked), typeof(bool), typeof(DropShadowPanel), new PropertyMetadata(true, OnIsMaskedChanged));

        /// <summary>
        /// Gets DropShadow. Exposes the underlying composition object to allow custom Windows.UI.Composition animations.
        /// </summary>
        public DropShadow DropShadow => _dropShadow;

        /// <summary>
        /// Gets or sets the mask of the underlying <see cref="Windows.UI.Composition.DropShadow"/>.
        /// Allows for a custom <see cref="Windows.UI.Composition.CompositionBrush"/> to be set.
        /// </summary>
        public CompositionBrush Mask
        {
            get => _dropShadow?.Mask;

            set
            {
                if (_dropShadow != null)
                {
                    _dropShadow.Mask = value;
                }
            }
        }

        /// <summary>
        /// Gets or sets the blur radius of the drop shadow.
        /// </summary>
        public double BlurRadius
        {
            get => (double)GetValue(BlurRadiusProperty);
            set => SetValue(BlurRadiusProperty, value);
        }

        /// <summary>
        /// Gets or sets the color of the drop shadow.
        /// </summary>
        public Color Color
        {
            get => (Color)GetValue(ColorProperty);
            set => SetValue(ColorProperty, value);
        }

        /// <summary>
        /// Gets or sets the x offset of the drop shadow.
        /// </summary>
        public double OffsetX
        {
            get => (double)GetValue(OffsetXProperty);
            set => SetValue(OffsetXProperty, value);
        }

        /// <summary>
        /// Gets or sets the y offset of the drop shadow.
        /// </summary>
        public double OffsetY
        {
            get => (double)GetValue(OffsetYProperty);
            set => SetValue(OffsetYProperty, value);
        }

        /// <summary>
        /// Gets or sets the z offset of the drop shadow.
        /// </summary>
        public double OffsetZ
        {
            get => (double)GetValue(OffsetZProperty);
            set => SetValue(OffsetZProperty, value);
        }

        /// <summary>
        /// Gets or sets the opacity of the drop shadow.
        /// </summary>
        public double ShadowOpacity
        {
            get => (double)GetValue(ShadowOpacityProperty);
            set => SetValue(ShadowOpacityProperty, value);
        }

        /// <summary>
        /// Gets or sets a value indicating whether the panel uses an alpha mask to create a more precise shadow vs. a quicker rectangle shape.
        /// </summary>
        /// <remarks>
        /// Turn this off to lose fidelity and gain performance of the panel.
        /// </remarks>
        public bool IsMasked
        {
            get => (bool)GetValue(IsMaskedProperty);
            set => SetValue(IsMaskedProperty, value);
        }

        private static void OnBlurRadiusChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is DropShadowPanel panel)
            {
                panel.OnBlurRadiusChanged((double)e.NewValue);
            }
        }

        private static void OnColorChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is DropShadowPanel panel)
            {
                panel.OnColorChanged((Color)e.NewValue);
            }
        }

        private static void OnOffsetXChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is DropShadowPanel panel)
            {
                panel.OnOffsetXChanged((double)e.NewValue);
            }
        }

        private static void OnOffsetYChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is DropShadowPanel panel)
            {
                panel.OnOffsetYChanged((double)e.NewValue);
            }
        }

        private static void OnOffsetZChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is DropShadowPanel panel)
            {
                panel.OnOffsetZChanged((double)e.NewValue);
            }
        }

        private static void OnShadowOpacityChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is DropShadowPanel panel)
            {
                panel.OnShadowOpacityChanged((double)e.NewValue);
            }
        }

        private static void OnIsMaskedChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
        {
            if (d is DropShadowPanel panel)
            {
                panel.UpdateShadowMask();
            }
        }
    }
}