// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

// Hand-written WinRT APIs included inside the generated Editor runtimeclass.
		Boolean CanPrepareDiff(UInt64 savedByteLength, UInt64 savedLines);
		Windows.Foundation.IAsyncOperation<EditorDiffResult> CompareAsync(Editor other);
		void ApplyDiffPresentation(EditorDiffResult result, Boolean oldSide);
		void SetDiffColours(Int32 line, Int32 inlineColour, Int32 gap, Int32 gapHatch);
		void DetachDocument();
		// Installs a complete compiled lexer configuration; optional OOM pauses coloring.
		void SetLexerLanguage(String name, String[] keywords, String[] propertyNames, String[] propertyValues);
		EditorSyntaxPauseReason SyntaxHighlightingPauseReason { get; };
		// Counted plain-text insertion using the native selection/paste rules.
		void PasteText(String text);
		// Canonical UTF-8 with CR line endings; the stream remains caller-owned.
		Windows.Foundation.IAsyncActionWithProgress<UInt64> LoadUtf8Async(Windows.Storage.Streams.IInputStream stream, UInt64 byteLength, Boolean preserveUndo);
		Windows.Storage.Streams.IBuffer ReadUtf8Range(Int64 offset, Int32 maxBytes);
		EditorUtf8Reader AcquireUtf8Reader();
		UInt64 DocumentSequence { get; };
		UInt64 DocumentRevision { get; };
		UInt64 SelectionRevision { get; };
		Windows.Foundation.IAsyncOperation<EditorSearchResult> FindRegexAsync(String pattern, Boolean matchCase, Int64 origin, Boolean previous, Boolean wrap, Int64 excludedEmpty);
		Windows.Foundation.IAsyncOperation<EditorSearchResult> ReplaceRegexAsync(String pattern, Boolean matchCase, String replacement, Int64 origin, Boolean previous, Boolean replaceAll);
		void StartJournal(String localPath, UInt64 baselineSequence);
		Windows.Foundation.IAsyncAction StartJournalFromCheckpointAsync(String localPath, EditorJournalCheckpoint checkpoint);
		Windows.Foundation.IAsyncAction RotateJournalAsync(String localPath, UInt64 expectedSequence);
		EditorJournalCheckpoint AcquireJournalCheckpoint();
		Windows.Foundation.IAsyncAction StopJournalAsync();
		// The active journal's writer failed; rotate to a fresh baseline to resume recovery.
		Boolean JournalFaulted { get; };
		Windows.Foundation.IAsyncActionWithProgress<UInt64> RestoreUtf8Async(Windows.Storage.Streams.IInputStream baselineStream, UInt64 baselineLength, EditorJournalCheckpoint checkpoint);
