// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/tree/master/Microsoft.Toolkit.Uwp.UI.Controls/InAppNotification

namespace Notepads.Controls
{
    using System;
    using System.Collections.Generic;
    using System.Linq;
    using Windows.UI.Xaml;
    using Windows.UI.Xaml.Controls;

    /// <summary>
    /// In App Notification defines a control to show local notification in the app.
    /// </summary>
    [TemplateVisualState(Name = StateContentVisible, GroupName = GroupContent)]
    [TemplateVisualState(Name = StateContentCollapsed, GroupName = GroupContent)]
    [TemplatePart(Name = DismissButtonPart, Type = typeof(Button))]
    public partial class InAppNotification : ContentControl
    {
        private InAppNotificationDismissKind _lastDismissKind;
        private readonly DispatcherTimer _openAnimationTimer = new DispatcherTimer();
        private readonly DispatcherTimer _closingAnimationTimer = new DispatcherTimer();
        private readonly DispatcherTimer _dismissTimer = new DispatcherTimer();
        private Button _dismissButton;
        private VisualStateGroup _visualStateGroup;
        private readonly List<NotificationOptions> _stackedNotificationOptions = new List<NotificationOptions>();

        /// <summary>
        /// Initializes a new instance of the <see cref="InAppNotification"/> class.
        /// </summary>
        public InAppNotification()
        {
            DefaultStyleKey = typeof(InAppNotification);

            _dismissTimer.Tick += DismissTimer_Tick;
            _openAnimationTimer.Tick += OpenAnimationTimer_Tick;
            _closingAnimationTimer.Tick += ClosingAnimationTimer_Tick;
        }

        /// <inheritdoc />
        protected override void OnApplyTemplate()
        {
            if (_dismissButton != null)
            {
                _dismissButton.Click -= DismissButton_Click;
            }

            _dismissButton = (Button)GetTemplateChild(DismissButtonPart);
            _visualStateGroup = (VisualStateGroup)GetTemplateChild(GroupContent);

            if (_dismissButton != null)
            {
                _dismissButton.Visibility = ShowDismissButton ? Visibility.Visible : Visibility.Collapsed;
                _dismissButton.Click += DismissButton_Click;
            }

            if (Visibility == Visibility.Visible)
            {
                VisualStateManager.GoToState(this, StateContentVisible, true);
            }
            else
            {
                VisualStateManager.GoToState(this, StateContentCollapsed, true);
            }

            base.OnApplyTemplate();
        }

        /// <summary>
        /// Show notification using the current template
        /// </summary>
        /// <param name="duration">Displayed duration of the notification in ms (less or equal 0 means infinite duration)</param>
        public void Show(int duration = 0)
        {
            lock (_openAnimationTimer)
                lock (_closingAnimationTimer)
                    lock (_dismissTimer)
                    {
                        _openAnimationTimer.Stop();
                        _closingAnimationTimer.Stop();
                        _dismissTimer.Stop();

                        var eventArgs = new InAppNotificationOpeningEventArgs();
                        Opening?.Invoke(this, eventArgs);

                        if (eventArgs.Cancel)
                        {
                            return;
                        }

                        Visibility = Visibility.Visible;
                        VisualStateManager.GoToState(this, StateContentVisible, true);

                        _openAnimationTimer.Interval = AnimationDuration;
                        _openAnimationTimer.Start();

                        if (duration > 0)
                        {
                            _dismissTimer.Interval = TimeSpan.FromMilliseconds(duration);
                            _dismissTimer.Start();
                        }
                    }
        }

        /// <summary>
        /// Show notification using text as the content of the notification
        /// </summary>
        /// <param name="text">Text used as the content of the notification</param>
        /// <param name="duration">Displayed duration of the notification in ms (less or equal 0 means infinite duration)</param>
        public void Show(string text, int duration = 0)
        {
            var notificationOptions = new NotificationOptions
            {
                Duration = duration,
                Content = text
            };
            Show(notificationOptions);
        }

        /// <summary>
        /// Show notification using UIElement as the content of the notification
        /// </summary>
        /// <param name="element">UIElement used as the content of the notification</param>
        /// <param name="duration">Displayed duration of the notification in ms (less or equal 0 means infinite duration)</param>
        public void Show(UIElement element, int duration = 0)
        {
            var notificationOptions = new NotificationOptions
            {
                Duration = duration,
                Content = element
            };
            Show(notificationOptions);
        }

        /// <summary>
        /// Show notification using DataTemplate as the content of the notification
        /// </summary>
        /// <param name="dataTemplate">DataTemplate used as the content of the notification</param>
        /// <param name="duration">Displayed duration of the notification in ms (less or equal 0 means infinite duration)</param>
        public void Show(DataTemplate dataTemplate, int duration = 0)
        {
            var notificationOptions = new NotificationOptions
            {
                Duration = duration,
                Content = dataTemplate
            };
            Show(notificationOptions);
        }

        /// <summary>
        /// Dismiss the notification
        /// </summary>
        public void Dismiss()
        {
            Dismiss(InAppNotificationDismissKind.Programmatic);
        }

        /// <summary>
        /// Dismiss the notification
        /// </summary>
        /// <param name="dismissKind">Kind of action that triggered dismiss event</param>
        private void Dismiss(InAppNotificationDismissKind dismissKind)
        {
            lock (_openAnimationTimer)
                lock (_closingAnimationTimer)
                    lock (_dismissTimer)
                    {
                        if (Visibility == Visibility.Visible)
                        {
                            _dismissTimer.Stop();

                            // Continue to display notification if on remaining stacked notification
                            if (_stackedNotificationOptions.Any())
                            {
                                _stackedNotificationOptions.RemoveAt(0);

                                if (_stackedNotificationOptions.Any())
                                {
                                    var notificationOptions = _stackedNotificationOptions[0];

                                    UpdateContent(notificationOptions);

                                    if (notificationOptions.Duration > 0)
                                    {
                                        _dismissTimer.Interval = TimeSpan.FromMilliseconds(notificationOptions.Duration);
                                        _dismissTimer.Start();
                                    }

                                    return;
                                }
                            }

                            _openAnimationTimer.Stop();
                            _closingAnimationTimer.Stop();

                            var closingEventArgs = new InAppNotificationClosingEventArgs(dismissKind);
                            Closing?.Invoke(this, closingEventArgs);

                            if (closingEventArgs.Cancel)
                            {
                                return;
                            }

                            VisualStateManager.GoToState(this, StateContentCollapsed, true);

                            _lastDismissKind = dismissKind;

                            _closingAnimationTimer.Interval = AnimationDuration;
                            _closingAnimationTimer.Start();
                        }
                    }
        }

        /// <summary>
        /// Informs if the notification should be displayed immediately (based on the StackMode)
        /// </summary>
        /// <returns>True if notification should be displayed immediately</returns>
        private bool ShouldDisplayImmediately()
        {
            return StackMode != StackMode.QueueBehind ||
                (StackMode == StackMode.QueueBehind && _stackedNotificationOptions.Count == 0);
        }

        /// <summary>
        /// Update the Content of the notification
        /// </summary>
        /// <param name="notificationOptions">Information about the notification to display</param>
        private void UpdateContent(NotificationOptions notificationOptions)
        {
            switch (notificationOptions.Content)
            {
                case string text:
                    ContentTemplate = null;
                    Content = text;
                    break;
                case UIElement element:
                    ContentTemplate = null;
                    Content = element;
                    break;
                case DataTemplate dataTemplate:
                    ContentTemplate = dataTemplate;
                    Content = null;
                    break;
            }
        }

        /// <summary>
        /// Handle the display of the notification based on the current StackMode
        /// </summary>
        /// <param name="notificationOptions">Information about the notification to display</param>
        private void Show(NotificationOptions notificationOptions)
        {
            bool shouldDisplayImmediately = ShouldDisplayImmediately();

            if (StackMode == StackMode.QueueBehind)
            {
                _stackedNotificationOptions.Add(notificationOptions);
            }

            if (StackMode == StackMode.StackInFront)
            {
                _stackedNotificationOptions.Insert(0, notificationOptions);
            }

            if (shouldDisplayImmediately)
            {
                UpdateContent(notificationOptions);
                Show(notificationOptions.Duration);
            }
        }
    }
}