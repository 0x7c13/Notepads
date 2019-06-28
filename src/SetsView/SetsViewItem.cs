// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

namespace SetsView
{
    using System;
    using Windows.Devices.Input;
    using Windows.UI.Input;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Input;

    /// <summary>
    /// Item Container for a <see cref="SetsView"/>.
    /// </summary>
    [TemplatePart(Name = SetCloseButtonName, Type = typeof(ButtonBase))]
    public partial class SetsViewItem : ListViewItem
    {
        private const string SetCloseButtonName = "CloseButton";

        private ButtonBase _setCloseButton;

        private bool _isMiddleClick;

        /// <summary>
        /// Initializes a new instance of the <see cref="SetsViewItem"/> class.
        /// </summary>
        public SetsViewItem()
        {
            DefaultStyleKey = typeof(SetsViewItem);
        }

        /// <summary>
        /// Fired when the Set's close button is clicked.
        /// </summary>
        public event EventHandler<SetClosingEventArgs> Closing;

        /// <inheritdoc/>
        protected override void OnApplyTemplate()
        {
            base.OnApplyTemplate();

            if (_setCloseButton != null)
            {
                _setCloseButton.Click -= SetCloseButton_Click;
            }

            _setCloseButton = GetTemplateChild(SetCloseButtonName) as ButtonBase;

            if (_setCloseButton != null)
            {
                _setCloseButton.Click += SetCloseButton_Click;
            }
        }

        /// <inheritdoc/>
        protected override void OnPointerPressed(PointerRoutedEventArgs e)
        {
            base.OnPointerPressed(e);

            _isMiddleClick = false;

            if (e.Pointer.PointerDeviceType == PointerDeviceType.Mouse)
            {
                PointerPoint pointerPoint = e.GetCurrentPoint(this);

                // Record if middle button is pressed
                if (pointerPoint.Properties.IsMiddleButtonPressed)
                {
                    _isMiddleClick = true;
                }
            }
        }

        /// <inheritdoc/>
        protected override void OnPointerReleased(PointerRoutedEventArgs e)
        {
            base.OnPointerReleased(e);

            // Close on Middle-Click
            if (_isMiddleClick)
            {
                SetCloseButton_Click(this, null);
            }

            _isMiddleClick = false;
        }

        public void Close()
        {
            if (IsClosable)
            {
                Closing?.Invoke(this, new SetClosingEventArgs(Content, this));
            }
        }

        private void SetCloseButton_Click(object sender, RoutedEventArgs e)
        {
            if (IsClosable)
            {
                Closing?.Invoke(this, new SetClosingEventArgs(Content, this));
            }
        }
    }
}