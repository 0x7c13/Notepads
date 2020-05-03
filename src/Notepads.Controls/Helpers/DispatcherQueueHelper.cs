// Licensed to the .NET Foundation under one or more agreements.
// The .NET Foundation licenses this file to you under the MIT license.
// See the LICENSE file in the project root for more information.
// Source: https://github.com/windows-toolkit/WindowsCommunityToolkit/blob/8464f8e5263686c1484732bdea86ebba3f30a075/Microsoft.Toolkit.Uwp/Helpers/DispatcherQueueHelper.cs

namespace Notepads.Controls.Helpers
{
    using System;
    using System.Threading.Tasks;
    using Windows.System;

    /// <summary>
    /// This class provides static methods helper for executing code in a DispatcherQueue.
    /// </summary>
    public static class DispatcherQueueHelper
    {
        /// <summary>
        /// Extension method for <see cref="DispatcherQueue"/>. Offering an actual awaitable <see cref="Task"/> with optional result that will be executed on the given dispatcher.
        /// </summary>
        /// <param name="dispatcher">DispatcherQueue of a thread to run <paramref name="function"/>.</param>
        /// <param name="function"> Function to be executed on the given dispatcher.</param>
        /// <param name="priority">DispatcherQueue execution priority, default is normal.</param>
        /// <returns>An awaitable <see cref="Task"/> for the operation.</returns>
        /// <remarks>If the current thread has UI access, <paramref name="function"/> will be invoked directly.</remarks>
        public static Task ExecuteOnUIThreadAsync(this DispatcherQueue dispatcher, Action function, DispatcherQueuePriority priority = DispatcherQueuePriority.Normal)
        {
            if (function is null)
            {
                throw new ArgumentNullException(nameof(function));
            }

            /* Run the function directly when we have thread access.
             * Also reuse Task.CompletedTask in case of success,
             * to skip an unnecessary heap allocation for every invocation. */

            // Ignoring for now, but need to map the CurrentThreadID for all dispatcher queue code we have
            /*
            if (dispatcher.HasThreadAccess)
            {
                try
                {
                    function();

                    return Task.CompletedTask;
                }
                catch (Exception e)
                {
                    return Task.FromException(e);
                }
            }
            */

            var taskCompletionSource = new TaskCompletionSource<object>();

            _ = dispatcher?.TryEnqueue(priority, () =>
            {
                try
                {
                    function();

                    taskCompletionSource.SetResult(null);
                }
                catch (Exception e)
                {
                    taskCompletionSource.SetException(e);
                }
            });

            return taskCompletionSource.Task;
        }

        /// <summary>
        /// Extension method for <see cref="DispatcherQueue"/>. Offering an actual awaitable <see cref="Task{T}"/> with optional result that will be executed on the given dispatcher.
        /// </summary>
        /// <typeparam name="T">Returned data type of the function.</typeparam>
        /// <param name="dispatcher">DispatcherQueue of a thread to run <paramref name="function"/>.</param>
        /// <param name="function"> Function to be executed on the given dispatcher.</param>
        /// <param name="priority">DispatcherQueue execution priority, default is normal.</param>
        /// <returns>An awaitable <see cref="Task{T}"/> for the operation.</returns>
        /// <remarks>If the current thread has UI access, <paramref name="function"/> will be invoked directly.</remarks>
        public static Task<T> ExecuteOnUIThreadAsync<T>(this DispatcherQueue dispatcher, Func<T> function, DispatcherQueuePriority priority = DispatcherQueuePriority.Normal)
        {
            if (function is null)
            {
                throw new ArgumentNullException(nameof(function));
            }

            // Skip the dispatch, if possible
            // Ignoring for now, but need to map the CurrentThreadID for all dispatcher queue code we have
            /*
            if (dispatcher.HasThreadAccess)
            {
                try
                {
                    return Task.FromResult(function());
                }
                catch (Exception e)
                {
                    return Task.FromException<T>(e);
                }
            }
            */

            var taskCompletionSource = new TaskCompletionSource<T>();

            _ = dispatcher?.TryEnqueue(priority, () =>
            {
                try
                {
                    taskCompletionSource.SetResult(function());
                }
                catch (Exception e)
                {
                    taskCompletionSource.SetException(e);
                }
            });

            return taskCompletionSource.Task;
        }

        /// <summary>
        /// Extension method for <see cref="DispatcherQueue"/>. Offering an actual awaitable <see cref="Task"/> with optional result that will be executed on the given dispatcher.
        /// </summary>
        /// <param name="dispatcher">DispatcherQueue of a thread to run <paramref name="function"/>.</param>
        /// <param name="function">Asynchronous function to be executed on the given dispatcher.</param>
        /// <param name="priority">DispatcherQueue execution priority, default is normal.</param>
        /// <returns>An awaitable <see cref="Task"/> for the operation.</returns>
        /// <remarks>If the current thread has UI access, <paramref name="function"/> will be invoked directly.</remarks>
        public static Task ExecuteOnUIThreadAsync(this DispatcherQueue dispatcher, Func<Task> function, DispatcherQueuePriority priority = DispatcherQueuePriority.Normal)
        {
            if (function is null)
            {
                throw new ArgumentNullException(nameof(function));
            }

            /* If we have thread access, we can retrieve the task directly.
             * We don't use ConfigureAwait(false) in this case, in order
             * to let the caller continue its execution on the same thread
             * after awaiting the task returned by this function. */

            // Ignoring for now, but need to map the CurrentThreadID for all dispatcher queue code we have
            /*
            if (dispatcher.HasThreadAccess)
            {
                try
                {
                    if (function() is Task awaitableResult)
                    {
                        return awaitableResult;
                    }

                    return Task.FromException(new InvalidOperationException("The Task returned by function cannot be null."));
                }
                catch (Exception e)
                {
                    return Task.FromException(e);
                }
            }
            */

            var taskCompletionSource = new TaskCompletionSource<object>();

            _ = dispatcher?.TryEnqueue(priority, async () =>
            {
                try
                {
                    if (function() is Task awaitableResult)
                    {
                        await awaitableResult.ConfigureAwait(false);

                        taskCompletionSource.SetResult(null);
                    }
                    else
                    {
                        taskCompletionSource.SetException(new InvalidOperationException("The Task returned by function cannot be null."));
                    }
                }
                catch (Exception e)
                {
                    taskCompletionSource.SetException(e);
                }
            });

            return taskCompletionSource.Task;
        }

        /// <summary>
        /// Extension method for <see cref="DispatcherQueue"/>. Offering an actual awaitable <see cref="Task{T}"/> with optional result that will be executed on the given dispatcher.
        /// </summary>
        /// <typeparam name="T">Returned data type of the function.</typeparam>
        /// <param name="dispatcher">DispatcherQueue of a thread to run <paramref name="function"/>.</param>
        /// <param name="function">Asynchronous function to be executed asynchronously on the given dispatcher.</param>
        /// <param name="priority">DispatcherQueue execution priority, default is normal.</param>
        /// <returns>An awaitable <see cref="Task{T}"/> for the operation.</returns>
        /// <remarks>If the current thread has UI access, <paramref name="function"/> will be invoked directly.</remarks>
        public static Task<T> ExecuteOnUIThreadAsync<T>(this DispatcherQueue dispatcher, Func<Task<T>> function, DispatcherQueuePriority priority = DispatcherQueuePriority.Normal)
        {
            if (function is null)
            {
                throw new ArgumentNullException(nameof(function));
            }

            // Skip the dispatch, if possible
            // Ignoring for now, but need to map the CurrentThreadID for all dispatcher queue code we have
            /*
            if (dispatcher.HasThreadAccess)
            {
                try
                {
                    if (function() is Task<T> awaitableResult)
                    {
                        return awaitableResult;
                    }

                    return Task.FromException<T>(new InvalidOperationException("The Task returned by function cannot be null."));
                }
                catch (Exception e)
                {
                    return Task.FromException<T>(e);
                }
            }
            */

            var taskCompletionSource = new TaskCompletionSource<T>();

            _ = dispatcher?.TryEnqueue(priority, async () =>
            {
                try
                {
                    if (function() is Task<T> awaitableResult)
                    {
                        var result = await awaitableResult.ConfigureAwait(false);

                        taskCompletionSource.SetResult(result);
                    }
                    else
                    {
                        taskCompletionSource.SetException(new InvalidOperationException("The Task returned by function cannot be null."));
                    }
                }
                catch (Exception e)
                {
                    taskCompletionSource.SetException(e);
                }
            });

            return taskCompletionSource.Task;
        }
    }
}