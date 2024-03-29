﻿// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2019-2024, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads.Views.MainPage
{
    using Notepads.Services;
    using Notepads.Utilities;

    public sealed partial class NotepadsMainPage : INotificationDelegate
    {
        private void InitializeNotificationCenter()
        {
            NotificationCenter.Instance.SetNotificationDelegate(this);
        }

        public void PostNotification(string message, int duration)
        {
            if (StatusNotification == null)
            {
                FindName("StatusNotification");  // Lazy loading
            }

            var textSize = FontUtility.GetTextSize(StatusNotification.FontFamily, StatusNotification.FontSize, message);
            StatusNotification.Width = textSize.Width + 100; // actual width + padding
            StatusNotification.Height = textSize.Height + 50; // actual height + padding
            StatusNotification.Show(message, duration);
        }
    }
}