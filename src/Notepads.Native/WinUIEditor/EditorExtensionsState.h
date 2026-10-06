// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

// Hand-written state included in the private section of Editor.
		struct TextLoadState
		{
			std::atomic<bool> canceled{ false };
		};
		std::shared_ptr<TextLoadState> _textLoadState;
		std::shared_ptr<::WinUIEditor::RegexJob> _regexJob;
		Windows::Foundation::IAsyncOperation<WinUIEditor::EditorSearchResult> RunRegexOperationAsync(::WinUIEditor::RegexRequest request, std::shared_ptr<::WinUIEditor::RegexJob> job);
		Windows::Foundation::IAsyncAction LoadTextOperationAsync(hstring text, std::shared_ptr<TextLoadState> state);
		Windows::Foundation::IAsyncActionWithProgress<uint64_t> LoadUtf8OperationAsync(Windows::Storage::Streams::IInputStream stream, uint64_t byteLength, bool preserveUndo, std::shared_ptr<TextLoadState> state, WinUIEditor::EditorJournalCheckpoint checkpoint = nullptr);
		Windows::Foundation::IAsyncAction PrepareJournalOperationAsync(hstring localPath, WinUIEditor::EditorJournalCheckpoint checkpoint, uint64_t expectedSequence, bool rotate, std::shared_ptr<TextLoadState> state);
