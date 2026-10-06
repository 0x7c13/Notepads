// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include "EditorJournalCheckpoint.g.h"
#include "NativeJournal.h"

namespace winrt::WinUIEditor::implementation
{
	struct EditorJournalCheckpoint : EditorJournalCheckpointT<EditorJournalCheckpoint>
	{
		EditorJournalCheckpoint(std::shared_ptr<::WinUIEditor::NativeJournalFile> file, ::WinUIEditor::JournalPrefix prefix);
		hstring FilePath();
		uint64_t BaseSequence();
		uint64_t CommittedSequence();
		uint64_t CommittedByteLength();
		uint64_t CommittedDocumentByteLength();
		hstring PrefixSha256();
		WinUIEditor::EditorJournalCheckpoint Retain();
		Windows::Foundation::IAsyncAction FlushAsync();
		Windows::Foundation::IAsyncAction WriteBaselineAsync(Windows::Storage::Streams::IInputStream baseline, uint64_t baselineLength,
			Windows::Storage::Streams::IOutputStream output, uint64_t maximumDocumentBytes);
		Windows::Foundation::IAsyncAction CloseAsync();
		void Close();
		void PreserveForRecovery();
		static Windows::Foundation::IAsyncOperation<WinUIEditor::EditorJournalCheckpoint> OpenAsync(
			hstring filePath, uint64_t baseSequence, uint64_t committedSequence, uint64_t committedByteLength, hstring prefixSha256);
		::WinUIEditor::JournalPrefix Prefix();

	  private:
		std::mutex _mutex;
		std::shared_ptr<::WinUIEditor::NativeJournalFile> _file;
		::WinUIEditor::JournalPrefix _prefix;
		bool _closed{};
		void CheckOpen() const;
		struct BaselineWriteState
		{
			std::atomic<bool> canceled{false};
		};
		Windows::Foundation::IAsyncAction WriteBaselineOperationAsync(Windows::Storage::Streams::IInputStream baseline,
			uint64_t baselineLength, Windows::Storage::Streams::IOutputStream output, uint64_t maximumDocumentBytes,
			std::shared_ptr<BaselineWriteState> state);
	};
} // namespace winrt::WinUIEditor::implementation
namespace winrt::WinUIEditor::factory_implementation
{
	struct EditorJournalCheckpoint : EditorJournalCheckpointT<EditorJournalCheckpoint, implementation::EditorJournalCheckpoint>
	{
	};
} // namespace winrt::WinUIEditor::factory_implementation
