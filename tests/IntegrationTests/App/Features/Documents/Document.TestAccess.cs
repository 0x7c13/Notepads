// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

// Access only: these members call the production operations and XAML handlers.
using System;
using System.Threading;
using System.Threading.Tasks;
using Notepads.Presentation.Controls.FindAndReplace;
using Notepads.Presentation.Controls.GoTo;
using Notepads.Presentation.Controls.TextEditor;
using Windows.Storage;
using Windows.UI.Xaml;
using Windows.UI.Xaml.Controls;

namespace Notepads.Presentation.Controls.TextEditor
{
    public sealed partial class TextEditor
    {
        internal async Task TestStopFileChecksAsync()
        {
            StopCheckingFileStatus();
            await _fileStatusCheckerCompletion;
        }

        internal Task TestCheckFileStatusAsync() => CheckAndUpdateFileStatusAsync(CancellationToken.None);
        internal Task TestHoldDocumentOperationAsync(Func<Task> action) => RunDocumentOperationAsync(_ => action());
        internal Task TestSaveWithCancellationAsync(StorageFile file, CancellationToken cancellation) =>
            RunDocumentOperationAsync(token => SaveAndUpdateEditorStateAsync(file, token), cancellation);
        internal FindAndReplaceControl TestFindControl => (FindAndReplaceControl)FindAndReplacePlaceholder?.Content;
        internal GoToControl TestGoToControl => (GoToControl)GoToPlaceholder?.Content;
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
        internal Task<bool> TestSaveDocumentAsync(ITextEditor editor, bool saveAs = false) => SaveAsync(editor, saveAs);
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
