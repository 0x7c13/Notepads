// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

namespace SetsView
{
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;

    /// <summary>
    /// SetsView properties.
    /// </summary>
    public partial class SetsView
    {
        /// <summary>
        /// Gets or sets the content to appear to the left or above the set strip.
        /// </summary>
        public object SetsStartHeader
        {
            get => GetValue(SetsStartHeaderProperty);
            set => SetValue(SetsStartHeaderProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="SetsStartHeader"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="SetsStartHeader"/> dependency property.</returns>
        public static readonly DependencyProperty SetsStartHeaderProperty =
            DependencyProperty.Register(nameof(SetsStartHeader), typeof(object), typeof(SetsView), new PropertyMetadata(null, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets the <see cref="DataTemplate"/> for the <see cref="SetsStartHeader"/>.
        /// </summary>
        public DataTemplate SetsStartHeaderTemplate
        {
            get => (DataTemplate)GetValue(SetsStartHeaderTemplateProperty);
            set => SetValue(SetsStartHeaderTemplateProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="SetsStartHeaderTemplate"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="SetsStartHeaderTemplate"/> dependency property.</returns>
        public static readonly DependencyProperty SetsStartHeaderTemplateProperty =
            DependencyProperty.Register(nameof(SetsStartHeaderTemplate), typeof(DataTemplate), typeof(SetsView), new PropertyMetadata(null, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets the content to appear next to the set strip.
        /// </summary>
        public object SetsActionHeader
        {
            get => GetValue(SetsActionHeaderProperty);
            set => SetValue(SetsActionHeaderProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="SetsActionHeader"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="SetsActionHeader"/> dependency property.</returns>
        public static readonly DependencyProperty SetsActionHeaderProperty =
            DependencyProperty.Register(nameof(SetsActionHeader), typeof(object), typeof(SetsView), new PropertyMetadata(null, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets the <see cref="DataTemplate"/> for the <see cref="SetsActionHeader"/>.
        /// </summary>
        public DataTemplate SetsActionHeaderTemplate
        {
            get => (DataTemplate)GetValue(SetsActionHeaderTemplateProperty);
            set => SetValue(SetsActionHeaderTemplateProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="SetsActionHeaderTemplate"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="SetsActionHeaderTemplate"/> dependency property.</returns>
        public static readonly DependencyProperty SetsActionHeaderTemplateProperty =
            DependencyProperty.Register(nameof(SetsActionHeaderTemplate), typeof(DataTemplate), typeof(SetsView), new PropertyMetadata(null, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets the content to appear to the right or below the action header.
        /// </summary>
        public object SetsPaddingHeader
        {
            get => GetValue(SetsPaddingHeaderProperty);
            set => SetValue(SetsPaddingHeaderProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="SetsPaddingHeader"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="SetsPaddingHeader"/> dependency property.</returns>
        public static readonly DependencyProperty SetsPaddingHeaderProperty =
            DependencyProperty.Register(nameof(SetsPaddingHeader), typeof(object), typeof(SetsView), new PropertyMetadata(null, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets the <see cref="DataTemplate"/> for the <see cref="SetsPaddingHeader"/>.
        /// </summary>
        public DataTemplate SetsPaddingHeaderTemplate
        {
            get => (DataTemplate)GetValue(SetsPaddingHeaderTemplateProperty);
            set => SetValue(SetsPaddingHeaderTemplateProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="SetsPaddingHeaderTemplate"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="SetsPaddingHeaderTemplate"/> dependency property.</returns>
        public static readonly DependencyProperty SetsPaddingHeaderTemplateProperty =
            DependencyProperty.Register(nameof(SetsPaddingHeaderTemplate), typeof(DataTemplate), typeof(SetsView), new PropertyMetadata(null, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets the content to appear to the right or below the set strip.
        /// </summary>
        public object SetsEndHeader
        {
            get => GetValue(SetsEndHeaderProperty);
            set => SetValue(SetsEndHeaderProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="SetsEndHeader"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="SetsEndHeader"/> dependency property.</returns>
        public static readonly DependencyProperty SetsEndHeaderProperty =
            DependencyProperty.Register(nameof(SetsEndHeader), typeof(object), typeof(SetsView), new PropertyMetadata(null, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets the <see cref="DataTemplate"/> for the <see cref="SetsEndHeader"/>.
        /// </summary>
        public DataTemplate SetsEndHeaderTemplate
        {
            get => (DataTemplate)GetValue(SetsEndHeaderTemplateProperty);
            set => SetValue(SetsEndHeaderTemplateProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="SetsEndHeaderTemplate"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="SetsEndHeaderTemplate"/> dependency property.</returns>
        public static readonly DependencyProperty SetsEndHeaderTemplateProperty =
            DependencyProperty.Register(nameof(SetsEndHeaderTemplate), typeof(DataTemplate), typeof(SetsView), new PropertyMetadata(null, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets the default <see cref="DataTemplate"/> for the <see cref="SetsViewItem.HeaderTemplate"/>.
        /// </summary>
        public DataTemplate ItemHeaderTemplate
        {
            get => (DataTemplate)GetValue(ItemHeaderTemplateTemplateProperty);
            set => SetValue(ItemHeaderTemplateTemplateProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="ItemHeaderTemplate"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="ItemHeaderTemplate"/> dependency property.</returns>
        public static readonly DependencyProperty ItemHeaderTemplateTemplateProperty =
            DependencyProperty.Register(nameof(ItemHeaderTemplate), typeof(DataTemplate), typeof(SetsView), new PropertyMetadata(null, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets a value indicating whether by default a Set can be closed or not if no value to <see cref="SetsViewItem.IsClosable"/> is provided.
        /// </summary>
        public bool CanCloseSets
        {
            get => (bool)GetValue(CanCloseSetsProperty);
            set => SetValue(CanCloseSetsProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="CanCloseSets"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="CanCloseSets"/> dependency property.</returns>
        public static readonly DependencyProperty CanCloseSetsProperty =
            DependencyProperty.Register(nameof(CanCloseSets), typeof(bool), typeof(SetsView), new PropertyMetadata(false, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets a value indicating whether a <see cref="SetsViewItem"/> Close Button should be included in layout calculations.
        /// </summary>
        public bool IsCloseButtonOverlay
        {
            get => (bool)GetValue(IsCloseButtonOverlayProperty);
            set => SetValue(IsCloseButtonOverlayProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="IsCloseButtonOverlay"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="IsCloseButtonOverlay"/> dependency property.</returns>
        public static readonly DependencyProperty IsCloseButtonOverlayProperty =
            DependencyProperty.Register(nameof(IsCloseButtonOverlay), typeof(bool), typeof(SetsView), new PropertyMetadata(false, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets a value indicating the size of the selected set.  By default this is set to Auto and the selected set size doesn't change.
        /// </summary>
        public double SelectedSetWidth
        {
            get => (double)GetValue(SelectedSetWidthProperty);
            set => SetValue(SelectedSetWidthProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="SelectedSetWidth"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="SelectedSetWidth"/> dependency property.</returns>
        public static readonly DependencyProperty SelectedSetWidthProperty =
            DependencyProperty.Register(nameof(SelectedSetWidth), typeof(double), typeof(SetsView), new PropertyMetadata(double.NaN, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets or sets the current <see cref="SetsWidthMode"/> which determins how set headers' width behave.
        /// </summary>
        public SetsWidthMode SetsWidthBehavior
        {
            get => (SetsWidthMode)GetValue(SetsWidthBehaviorProperty);
            set => SetValue(SetsWidthBehaviorProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="SetsWidthBehavior"/> dependency property.
        /// </summary>
        /// <returns>The identifier for the <see cref="SetsWidthBehavior"/> dependency property.</returns>
        public static readonly DependencyProperty SetsWidthBehaviorProperty =
            DependencyProperty.Register(nameof(SetsWidthBehavior), typeof(SetsWidthMode), typeof(SetsView), new PropertyMetadata(SetsWidthMode.Actual, OnLayoutEffectingPropertyChanged));

        /// <summary>
        /// Gets the attached property value to indicate if this grid column should be ignored when calculating header sizes.
        /// </summary>
        /// <param name="obj">Grid Column.</param>
        /// <returns>Boolean indicating if this column is ignored by SetsViewHeader logic.</returns>
        public static bool GetIgnoreColumn(ColumnDefinition obj)
        {
            return (bool)obj.GetValue(IgnoreColumnProperty);
        }

        /// <summary>
        /// Sets the attached property value for <see cref="IgnoreColumnProperty"/>
        /// </summary>
        /// <param name="obj">Grid Column.</param>
        /// <param name="value">Boolean value</param>
        public static void SetIgnoreColumn(ColumnDefinition obj, bool value)
        {
            obj.SetValue(IgnoreColumnProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="IgnoreColumnProperty"/> attached property.
        /// </summary>
        /// <returns>The identifier for the IgnoreColumn attached property.</returns>
        public static readonly DependencyProperty IgnoreColumnProperty =
            DependencyProperty.RegisterAttached("IgnoreColumn", typeof(bool), typeof(SetsView), new PropertyMetadata(false));

        /// <summary>
        /// Gets the attached value indicating this column should be restricted for the <see cref="SetsViewItem"/> headers.
        /// </summary>
        /// <param name="obj">Grid Column.</param>
        /// <returns>True if this column should be constrained.</returns>
        public static bool GetConstrainColumn(ColumnDefinition obj)
        {
            return (bool)obj.GetValue(ConstrainColumnProperty);
        }

        /// <summary>
        /// Sets the attached property value for the <see cref="ConstrainColumnProperty"/>
        /// </summary>
        /// <param name="obj">Grid Column.</param>
        /// <param name="value">Boolean value.</param>
        public static void SetConstrainColumn(ColumnDefinition obj, bool value)
        {
            obj.SetValue(ConstrainColumnProperty, value);
        }

        /// <summary>
        /// Identifies the <see cref="ConstrainColumnProperty"/> attached property.
        /// </summary>
        /// <returns>The identifier for the ConstrainColumn attached property.</returns>
        public static readonly DependencyProperty ConstrainColumnProperty =
            DependencyProperty.RegisterAttached("ConstrainColumn", typeof(bool), typeof(SetsView), new PropertyMetadata(false));
    }
}
