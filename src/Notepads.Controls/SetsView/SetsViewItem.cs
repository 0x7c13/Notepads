﻿// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.

namespace Notepads.Controls
{
    using System;
    using Windows.Devices.Input;
    using Windows.System;
    using Windows.UI;
    using Windows.UI.Core;
    using Windows.UI.Input;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;
    using Windows.UI.Xaml.Controls.Primitives;
    using Windows.UI.Xaml.Input;
    using Windows.UI.Xaml.Media;
    using Windows.UI.Xaml.Shapes;

    /// <summary>
    /// Item Container for a <see cref="SetsView"/>.
    /// </summary>
    [TemplatePart(Name = SetCloseButtonName, Type = typeof(ButtonBase))]
    [TemplatePart(Name = SetLeftSideSeparatorName, Type = typeof(Border))]
    [TemplatePart(Name = SetRightSideSeparatorName, Type = typeof(Border))]
    [TemplatePart(Name = SetSelectionIndicatorName, Type = typeof(Rectangle))]
    public partial class SetsViewItem : ListViewItem
    {
        private const string SetCloseButtonName = "CloseButton";

        private const string SetLeftSideSeparatorName = "LeftSideSeparator";

        private const string SetRightSideSeparatorName = "RightSideSeparator";

        private const string SetSelectionIndicatorName = "SelectionIndicator";

        private ButtonBase _setCloseButton;

        private Border _setLeftSideSeparator;

        private Border _setRightSideSeparator;

        private Rectangle _setSelectionIndicator;

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

        public void ShowLeftSideSeparator()
        {
            if (_setLeftSideSeparator != null)
            {
                _setLeftSideSeparator.Visibility = Visibility.Visible;
            }
        }

        public void HideLeftSideSeparator()
        {
            if (_setLeftSideSeparator != null)
            {
                _setLeftSideSeparator.Visibility = Visibility.Collapsed;
            }
        }

        public void ShowRightSideSeparator()
        {
            if (_setRightSideSeparator != null)
            {
                _setRightSideSeparator.Visibility = Visibility.Visible;
            }
        }

        public void HideRightSideSeparator()
        {
            if (_setRightSideSeparator != null)
            {
                _setRightSideSeparator.Visibility = Visibility.Collapsed;
            }
        }

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

            _setLeftSideSeparator = GetTemplateChild(SetLeftSideSeparatorName) as Border;
            _setRightSideSeparator = GetTemplateChild(SetRightSideSeparatorName) as Border;
            _setSelectionIndicator = GetTemplateChild(SetSelectionIndicatorName) as Rectangle;
        }

        /// <inheritdoc/>
        protected override void OnPointerPressed(PointerRoutedEventArgs e)
        {
            _isMiddleClick = false;

            if (e?.Pointer.PointerDeviceType == PointerDeviceType.Mouse)
            {
                PointerPoint pointerPoint = e.GetCurrentPoint(this);

                // Record if middle button is pressed
                if (pointerPoint.Properties.IsMiddleButtonPressed)
                {
                    _isMiddleClick = true;
                }

                // Disable unwanted behaviour inherited by ListViewItem:
                // Disable "Ctrl + Left click" to deselect tab
                // Or variant like "Ctrl + Shift + Left click"
                // Or "Ctrl + Alt + Left click"
                if (pointerPoint.Properties.IsLeftButtonPressed)
                {
                    var ctrl = Window.Current.CoreWindow.GetKeyState(VirtualKey.Control);
                    if (ctrl.HasFlag(CoreVirtualKeyStates.Down))
                    {
                        // return here so the event won't be picked up by the base class
                        // but keep this event unhandled so it can be picked up further
                        return;
                    }
                }
            }

            base.OnPointerPressed(e);
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

        public void PrepareForClosing()
        {
            _setSelectionIndicator.Fill = new SolidColorBrush(Colors.Transparent);
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