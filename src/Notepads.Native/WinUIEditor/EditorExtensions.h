// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

// Hand-written declarations included inside the generated Editor class.
// Keep these APIs independent of the Scintilla interface generator.
		bool CanPrepareDiff(uint64_t savedByteLength, uint64_t savedLines);
		Windows::Foundation::IAsyncOperation<WinUIEditor::EditorDiffResult> CompareAsync(WinUIEditor::Editor other);
		Windows::Foundation::IAsyncOperation<WinUIEditor::EditorDiffResult> RunDiffAsync(WinUIEditor::Editor other, std::shared_ptr<::WinUIEditor::DiffJob> job);
		void ApplyDiffPresentation(WinUIEditor::EditorDiffResult const &result, bool oldSide);
		void SetDiffColours(int32_t line, int32_t inlineColour, int32_t gap, int32_t gapHatch);
		void SetDiffHighlight(int64_t row, int64_t count, int32_t colour);
		void DetachDocument();
		void SetLexerLanguage(hstring const &name, array_view<hstring const> keywords,
			array_view<hstring const> propertyNames, array_view<hstring const> propertyValues);
		WinUIEditor::EditorSyntaxPauseReason SyntaxHighlightingPauseReason();
		void PasteText(hstring const &text);
		Windows::Foundation::IAsyncActionWithProgress<uint64_t> LoadUtf8Async(Windows::Storage::Streams::IInputStream stream, uint64_t byteLength, bool preserveUndo);
		Windows::Storage::Streams::IBuffer ReadUtf8Range(int64_t offset, int32_t maxBytes);
		WinUIEditor::EditorUtf8Reader AcquireUtf8Reader();
		uint64_t DocumentSequence();
		uint64_t DocumentRevision();
		uint64_t SelectionRevision();
		Windows::Foundation::IAsyncOperation<WinUIEditor::EditorSearchResult> FindRegexAsync(hstring pattern, bool matchCase, int64_t origin, bool previous, bool wrap, int64_t excludedEmpty);
		Windows::Foundation::IAsyncOperation<WinUIEditor::EditorSearchResult> ReplaceRegexAsync(hstring pattern, bool matchCase, hstring replacement, int64_t origin, bool previous, bool replaceAll);
		void StartJournal(hstring const &localPath, uint64_t baselineSequence);
		Windows::Foundation::IAsyncAction StartJournalFromCheckpointAsync(hstring localPath, WinUIEditor::EditorJournalCheckpoint checkpoint);
		Windows::Foundation::IAsyncAction RotateJournalAsync(hstring localPath, uint64_t expectedSequence);
		WinUIEditor::EditorJournalCheckpoint AcquireJournalCheckpoint();
		Windows::Foundation::IAsyncAction StopJournalAsync();
		bool JournalFaulted();
		Windows::Foundation::IAsyncActionWithProgress<uint64_t> RestoreUtf8Async(Windows::Storage::Streams::IInputStream baselineStream, uint64_t baselineLength, WinUIEditor::EditorJournalCheckpoint checkpoint);
