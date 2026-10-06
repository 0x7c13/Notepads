// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include "EditorUtf8Reader.g.h"
#include "EditorBaseControl.h"

namespace winrt::WinUIEditor::implementation
{
	struct EditorUtf8Reader : EditorUtf8ReaderT<EditorUtf8Reader>
	{
		EditorUtf8Reader(WinUIEditor::Editor const &editor, com_ptr<EditorBaseControl> const &view);
		~EditorUtf8Reader();
		static fire_and_forget final_release(std::unique_ptr<EditorUtf8Reader> self) noexcept;
		uint64_t Length() const;
		uint64_t Sequence() const;
		Windows::Storage::Streams::IBuffer Read(int64_t offset, int32_t maxBytes);
		Windows::Foundation::IAsyncOperation<Windows::Storage::Streams::IBuffer> ReadAsync(int64_t offset, int32_t maxBytes);
		void Close();

	  private:
		apartment_context _owningApartment;
		DWORD _owningThreadId{::GetCurrentThreadId()};
		WinUIEditor::Editor _editor{nullptr};
		com_ptr<EditorBaseControl> _view;
		uint64_t _length;
		uint64_t _sequence;
		std::atomic<bool> _closed{false};
		bool _leaseHeld{true};
		void ReleaseLease() noexcept;
		fire_and_forget ReleaseLeaseAsync();
	};
} // namespace winrt::WinUIEditor::implementation
