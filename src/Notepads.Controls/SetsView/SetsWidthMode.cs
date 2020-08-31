// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

namespace Notepads.Controls
{
    using Windows.UI.Xaml;

    /// <summary>
    /// Possible modes for how to layout a <see cref="SetsViewItem"/> Header's Width in the <see cref="SetsView"/>.
    /// </summary>
    public enum SetsWidthMode
    {
        /// <summary>
        /// Each set header takes up as much space as it needs.  This is similar to how WPF and Visual Studio Code behave.
        /// Suggest to keep <see cref="SetsView.IsCloseButtonOverlay"/> set to false.
        /// <see cref="SetsView.SelectedSetWidth"/> is ignored.
        /// In this scenario, set width behavior is effectively turned off.  This can be useful when using custom styling or a custom panel for layout of <see cref="SetsViewItem"/> as well.
        /// </summary>
        Actual,

        /// <summary>
        /// Each set header will use the minimal space set by <see cref="FrameworkElement.MinWidth"/> on the <see cref="SetsViewItem"/>.
        /// Suggest to set the <see cref="SetsView.SelectedSetWidth"/> to show more content for the selected item.
        /// </summary>
        Compact,

        /// <summary>
        /// Each set header will fill to fit the available space.  If <see cref="SetsView.SelectedSetWidth"/> is set, that will be used as a Maximum Width.
        /// This is similar to how Microsoft Edge behaves when used with the <see cref="SetsView.SelectedSetWidth"/>.
        /// Suggest to set <see cref="SetsView.IsCloseButtonOverlay"/> to true.
        /// Suggest to set <see cref="SetsView.SelectedSetWidth"/> to 200 and the SetsViewItemHeaderMinWidth Resource to 90.
        /// </summary>
        Equal,
    }
}