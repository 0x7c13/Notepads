// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

namespace SetsView
{
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Media;

    /// <summary>
    /// Item Container for a <see cref="SetsView"/>.
    /// </summary>
    public partial class SetsViewItem
    {
        /// <summary>
        /// Gets or sets the header content for the set.
        /// </summary>
        public object Header
        {
            get => GetValue(HeaderProperty);
            set => SetValue(HeaderProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="Header"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="Header"/> dependency property.</returns>
        public static readonly DependencyProperty HeaderProperty =
            DependencyProperty.Register(nameof(Header), typeof(object), typeof(SetsViewItem), new PropertyMetadata(null));

        /// <summary>
        /// Gets or sets the icon to appear in the set header.
        /// </summary>
        public IconElement Icon
        {
            get => (IconElement)GetValue(IconProperty);
            set => SetValue(IconProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="Icon"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="Icon"/> dependency property.</returns>
        public static readonly DependencyProperty IconProperty =
            DependencyProperty.Register(nameof(Icon), typeof(IconElement), typeof(SetsViewItem), new PropertyMetadata(null));

        /// <summary>
        /// Gets or sets the template to override for the set header.
        /// </summary>
        public DataTemplate HeaderTemplate
        {
            get => (DataTemplate)GetValue(HeaderTemplateProperty);
            set => SetValue(HeaderTemplateProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="HeaderTemplate"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="HeaderTemplate"/> dependency property.</returns>
        public static readonly DependencyProperty HeaderTemplateProperty =
            DependencyProperty.Register(nameof(HeaderTemplate), typeof(DataTemplate), typeof(SetsViewItem), new PropertyMetadata(null));

        /// <summary>
        /// Gets or sets a value indicating whether the set can be closed by the user with the close button.
        /// </summary>
        public bool IsClosable
        {
            get => (bool)GetValue(IsClosableProperty);
            set => SetValue(IsClosableProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="IsClosable"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="IsClosable"/> dependency property.</returns>
        public static readonly DependencyProperty IsClosableProperty =
            DependencyProperty.Register(nameof(IsClosable), typeof(bool), typeof(SetsViewItem), new PropertyMetadata(null));

        /// <summary>
        /// Gets or sets the selection indicator brush
        /// </summary>
        public Brush SelectionIndicatorForeground
        {
            get => (Brush)GetValue(SelectionIndicatorForegroundProperty);
            set => SetValue(SelectionIndicatorForegroundProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="SelectionIndicatorForeground"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="SelectionIndicatorForeground"/> dependency property.</returns>
        public static readonly DependencyProperty SelectionIndicatorForegroundProperty =
            DependencyProperty.Register(nameof(SelectionIndicatorForeground), typeof(Brush), typeof(SetsViewItem), new PropertyMetadata(null));
    }
}
