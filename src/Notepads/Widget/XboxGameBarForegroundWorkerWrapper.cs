namespace Notepads
{
    using System;
    using System.Threading.Tasks;
    using Windows.UI.Core;
    using Notepads.Extensions;
    using Microsoft.Gaming.XboxGameBar.Restricted;
    using Microsoft.Gaming.XboxGameBar;

    public class XboxGameBarForegroundWorkerWrapper
    {
        XboxGameBarWidget _widget;
        private Func<Task> _func;
        private CoreDispatcher _dispatcher;

        public event EventHandler CancelOperationRequested;

        public XboxGameBarForegroundWorkerWrapper(
            XboxGameBarWidget widget,
            CoreDispatcher dispatcher,
            Func<Task> foregroundWorkFunc)
        {
            _widget = widget;
            _func = foregroundWorkFunc;
            _dispatcher = dispatcher;
        }

        public async Task ExecuteAsync()
        {
            // Create a lambda for the UI work and re-use if not running as a Game Bar widget
            // If you are doing async work on the UI thread inside this lambda, it must be awaited before the lambda returns to ensure Game Bar is
            // in the right state for the entirety of the foreground operation.
            // We recommend using the Dispatcher RunTaskAsync task extension to make this easier
            // Look at Extensions/DispatcherTaskExtensions.cs
            // For more information you can read this blog post: https://devblogs.microsoft.com/oldnewthing/20190327-00/?p=102364
            // For another approach more akin to how C++/WinRT handles awaitable thread switching, read this blog post: https://devblogs.microsoft.com/oldnewthing/20190328-00/?p=102368
            ForegroundWorkHandler foregroundWorkHandler = () =>
            {
                var mainTask = Task.Run(async () =>
                {
                    await _dispatcher.RunTaskAsync(async () =>
                    {
                        await _func.Invoke();
                    });
                });

                var continueTask = mainTask.ContinueWith(t =>
                {
                    if (t.IsFaulted)
                    {
                        // Throw the inner exception if it's there, otherwise just throw the outer one
                        throw t.Exception.InnerException ?? t.Exception;
                    }

                    return true;
                }).AsAsyncOperation();

                return continueTask;
            };

            var foregroundWorker = new XboxGameBarForegroundWorker(_widget, foregroundWorkHandler);

            foregroundWorker.CancelOperationRequested += CancelOperationRequestedHandler;
            await foregroundWorker.ExecuteAsync();
            foregroundWorker.CancelOperationRequested -= CancelOperationRequestedHandler;
        }

        private void CancelOperationRequestedHandler(XboxGameBarForegroundWorker sender, object args)
        {
            CancelOperationRequested?.Invoke(this, EventArgs.Empty);
        }
    }
}
