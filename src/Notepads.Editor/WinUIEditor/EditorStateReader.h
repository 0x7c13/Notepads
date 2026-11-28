// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include "EditorStateReader.g.h"
#include "EditorBaseControl.h"

namespace winrt::WinUIEditor::implementation
{
	struct EditorStateReader : EditorStateReaderT<EditorStateReader>
	{
		EditorStateReader(WinUIEditor::EditorUtf8Reader lease, com_ptr<EditorBaseControl> const &view);
		uint64_t Length() const;
		uint64_t Sequence() const;
		Windows::Foundation::IAsyncOperation<Windows::Storage::Streams::IBuffer> ReadAsync();
		void Close();
	private:
		apartment_context _owner;
		// Reuse the existing reader's apartment-safe read-lease lifetime.
		WinUIEditor::EditorUtf8Reader _lease{nullptr};
		std::unique_ptr<::WinUIEditor::EditorStateWriter> _writer;
		std::atomic<bool> _closed{false};
	};
}
