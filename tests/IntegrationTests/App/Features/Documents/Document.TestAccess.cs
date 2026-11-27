// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

// Access only: these members call the production operations and XAML handlers.
using System;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Controls;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Controls.GoTo;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace Notepads.Features.Documents.IO
{
    internal static partial class DocumentFileWriter
    {
        internal static IDocumentFileUpdates TestUpdates;
        static partial void OnSelectUpdates(ref IDocumentFileUpdates updates) => updates = TestUpdates ?? updates;
        internal static Func<string, string> TestTransactionLocation;
        static partial void OnTransactionLocationObserved(ref string observed)
        {
            if (TestTransactionLocation != null) observed = TestTransactionLocation(observed);
        }
        internal static Action<StorageFile> TestLocalCommit;
        static partial void OnLocalCommit(StorageFile file) => TestLocalCommit?.Invoke(file);
    }

    internal static partial class DocumentSaveTarget
    {
        internal static Task<string> TestReadHashAsync(Windows.Storage.Streams.IInputStream input, CancellationToken cancellation) =>
            ReadSha256Async(input, cancellation);

        internal static Action<bool> TestProvingCaseSpelling;
        static partial void OnProvingCaseSpelling(bool grant) => TestProvingCaseSpelling?.Invoke(grant);
        internal static Func<string, bool> TestNamedMetadataUnavailable;
        static partial void OnOpeningNamedMetadata(string path, ref bool unavailable) =>
            unavailable = TestNamedMetadataUnavailable?.Invoke(path) == true;
        internal static Func<string, string> TestPinnedLocation;
        internal static bool TestAnchorMoved;
        static partial void OnCheckingPinnedAnchor(ref string finalPath)
        {
            if (TestAnchorMoved) finalPath += "-moved";
        }
        static partial void OnRecheckingPinnedLocation(ref string observed)
        {
            if (TestPinnedLocation != null) observed = TestPinnedLocation(observed);
        }
        internal static bool TestMissingParentTag;
        internal static bool TestMissingParentName;
        static partial void OnCheckingOrdinaryParent(ref bool hasTag, ref string finalPath)
        {
            if (TestMissingParentTag) hasTag = false;
            if (TestMissingParentName) finalPath = null;
        }
        internal static Func<StorageFile, CancellationToken, Task> TestOriginalVersionRead;
        static partial void OnOriginalVersionRead(StorageFile file, CancellationToken cancellation, ref Task observation) =>
            observation = TestOriginalVersionRead?.Invoke(file, cancellation);
        internal static Func<StorageFile, CancellationToken, Task> TestVersionRead;
        static partial void OnVersionRead(StorageFile file, CancellationToken cancellation, ref Task observation) =>
            observation = TestVersionRead?.Invoke(file, cancellation);

        internal static Microsoft.Win32.SafeHandles.SafeFileHandle TestPinLockMetadata(string path) =>
            CreateFileFromAppW(path, 0, System.IO.FileShare.ReadWrite | System.IO.FileShare.Delete,
                IntPtr.Zero, System.IO.FileMode.Open, 0, IntPtr.Zero);

        internal static int TestRawPathAccess(string path, uint access, uint flags)
        {
            using var handle = CreateFileFromAppW(ExtendedPath(path), access, System.IO.FileShare.ReadWrite | System.IO.FileShare.Delete,
                IntPtr.Zero, System.IO.FileMode.Open, flags, IntPtr.Zero);
            return handle.IsInvalid ? System.Runtime.InteropServices.Marshal.GetLastPInvokeError() : 0;
        }

        internal static string TestRawEntrySpelling(string path)
        {
            using var handle = CreateFileFromAppW(path, 0, System.IO.FileShare.ReadWrite | System.IO.FileShare.Delete,
                IntPtr.Zero, System.IO.FileMode.Open, 0x00200000, IntPtr.Zero);
            return GetFinalPath(handle);
        }

        internal static string TestDescribeGrantedTarget(StorageFile file)
        {
            var path = ExtendedPath(file.Path);
            using var named = OpenMetadataByName(path);
            var error = named.IsInvalid ? System.Runtime.InteropServices.Marshal.GetLastPInvokeError() : 0;
            using var granted = OpenGrantedMetadata(file);
            return $"requested={path}, named metadata={error}, granted DOS={GetFinalPath(granted, 0)}, granted final={GetFinalPath(granted)}";
        }

        internal static IDisposable TestPinNamespace(string path)
        {
            var pins = new NamespacePins();
            try { pins.Protect(ExtendedPath(path)); return pins; }
            catch { pins.Dispose(); throw; }
        }
    }
}

namespace Notepads.Infrastructure.Storage
{
    internal sealed partial class FileLockLease
    {
        internal static Action<string> TestLeaseWait;
        static partial void OnWaitingForLease(string path) => TestLeaseWait?.Invoke(path);
    }
}

namespace Notepads.Presentation.Controls.TextEditor
{
    public sealed partial class TextEditor
    {
        internal Action TestReadingRenamedLocation;
        partial void OnReadingRenamedLocation() => TestReadingRenamedLocation?.Invoke();
        internal Func<Task> TestSaveReaderCaptured;
        partial void OnSaveReaderCaptured(ref Task observation) => observation = TestSaveReaderCaptured?.Invoke();
        internal Func<Task> TestResolvingSaveTarget;
        partial void OnResolvingSaveTarget(ref Task observation) => observation = TestResolvingSaveTarget?.Invoke();
        internal async Task TestStopFileChecksAsync()
        {
            StopCheckingFileStatus();
            await _fileStatusCheckerCompletion;
        }

        internal Task TestCheckFileStatusAsync() => CheckAndUpdateFileStatusAsync(CancellationToken.None);
        internal Guid? TestPendingSaveId => _pendingFileSave?.Id;
        internal void TestMakeSavedFileVersionUnknown()
        {
            var previous = LastSavedSnapshot;
            LastSavedSnapshot = new Features.Documents.DocumentSnapshot(previous.Baseline, previous.Encoding,
                previous.LineEnding, -1);
            previous.Dispose();
        }
        internal Task TestHoldDocumentOperationAsync(Func<Task> action) => RunDocumentOperationAsync(_ => action());
        internal Task TestSaveWithCancellationAsync(StorageFile file, CancellationToken cancellation) =>
            SaveContentToFileAndUpdateEditorStateAsync(file, cancellation: cancellation);
        internal FindAndReplaceControl TestFindControl => (FindAndReplaceControl)FindAndReplacePlaceholder?.Content;
        internal GoToControl TestGoToControl => (GoToControl)GoToPlaceholder?.Content;
        internal InAppNotification TestFindNotification => FindAndReplacePlaceholder;
        internal InAppNotification TestGoToNotification => GoToPlaceholder;
        internal bool TestFindVisible => FindAndReplacePlaceholder?.Visibility == Visibility.Visible;
        internal bool TestGoToVisible => GoToPlaceholder?.Visibility == Visibility.Visible;
        internal void TestEscape() => OnEscapeKeyDown();
        internal string TestDocumentDisposalState => $"disposed={_disposed}, operations={_documentOperations.Completion.Status}, " +
            $"fileStatus={_fileStatusCheckerCompletion.Status}, journal={_journalShutdown.Status}, completion={_disposalCompletion.Task.Status}";
    }
}

namespace Notepads.Presentation.Views.MainPage
{
    public sealed partial class NotepadsMainPage
    {
        internal Workspace.TextEditorTransfer TestCaptureTransfer(ITextEditor editor) => new(editor, _context);
        internal Task<Features.Sessions.Recovery.SessionRecoveryPlan> TestDiscoverRecoveryAsync() => _context.Sessions.DiscoverRecoveryAsync();
        internal Action<StorageFile> TestPickedVersionCaptured;
        partial void OnPickedVersionCaptured(StorageFile file) => TestPickedVersionCaptured?.Invoke(file);

        internal Task<bool> TestSaveDocumentAsync(ITextEditor editor, bool saveAs = false, bool ignoreUnmodified = false) => SaveAsync(editor, saveAs, ignoreUnmodified);
        internal string TestFileStatusMessage => FileModificationStateIndicator?.Visibility == Visibility.Visible
            ? ToolTipService.GetToolTip(FileModificationStateIndicator) as string : null;
        internal Task<bool> TestSaveAllDocumentsAsync() => SaveAllAsync(NotepadsCore.GetTabs());
        internal Task TestRenameDocumentAsync(ITextEditor editor) => RenameFileAsync(editor);
        internal Task TestRevertAllChangesAsync() => ExecuteModificationCommandAsync("RevertAllChanges");
    }
}

namespace Notepads.Presentation.Controls.FindAndReplace
{
    public sealed partial class FindAndReplaceControl
    {
        internal TextBox TestFindInput => FindBar;
        internal TextBox TestReplaceInput => ReplaceBar;
        internal Button TestNextButton => SearchForwardButton;
        internal Button TestPreviousButton => SearchBackwardButton;
        internal Button TestReplaceButton => ReplaceButton;
        internal Button TestReplaceAllButton => ReplaceAllButton;
        internal bool TestReplaceVisible => ReplaceBarPlaceHolder.Visibility == Visibility.Visible;
        internal bool TestOptionIndicatorVisible => OptionButtonSelectionIndicator.Visibility == Visibility.Visible;
        internal ToggleMenuFlyoutItem TestMatchCase => MatchCaseToggle;
        internal ToggleMenuFlyoutItem TestWholeWord => MatchWholeWordToggle;
        internal ToggleMenuFlyoutItem TestRegex => UseRegexToggle;
        internal string TestInputState => $"loaded={IsLoaded}, text=[{FindBar.Text}], " +
            $"selection=[{FindBar.SelectionStart},{FindBar.SelectionLength}], " +
            $"canUpdateQuery={_shouldUpdateSearchString}, enterPressed={_enterPressed}, focus={FindBar.FocusState}";
        internal void TestOptionsChanged() => OptionButtonFlyoutItem_OnClick(OptionButton, new RoutedEventArgs());
        internal void TestToggleReplace() => ToggleReplaceModeButton_OnClick(ToggleReplaceModeButton, new RoutedEventArgs());
        internal void TestSubmitFindFromInput() => SearchForwardButton_OnClick(FindBar, new RoutedEventArgs());
        internal void TestSubmitReplaceFromInput() => ReplaceButton_OnClick(ReplaceBar, new RoutedEventArgs());
    }
}

namespace Notepads.Presentation.Controls.GoTo
{
    public sealed partial class GoToControl
    {
        internal TextBox TestInput => GoToBar;
        internal Button TestSearchButton => SearchButton;
    }
}
