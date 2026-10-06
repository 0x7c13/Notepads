// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "EditorUtf8Reader.h"
#include "EditorUtf8Reader.g.cpp"
#include "Helpers.h"
#include <exception>

namespace winrt::WinUIEditor::implementation
{
	EditorUtf8Reader::EditorUtf8Reader(WinUIEditor::Editor const &editor, com_ptr<EditorBaseControl> const &view)
		: _editor(editor), _view(view), _length(static_cast<uint64_t>(editor.Length())), _sequence(editor.DocumentSequence())
	{
		if (_length > INT32_MAX)
			throw hresult_invalid_argument();
		view->AcquireReadLease();
	}

	EditorUtf8Reader::~EditorUtf8Reader()
	{
		ReleaseLease();
	}

	fire_and_forget EditorUtf8Reader::final_release(std::unique_ptr<EditorUtf8Reader> self) noexcept
	{
		if (::GetCurrentThreadId() == self->_owningThreadId)
			co_return;
		try
		{
			co_await resume_background();
			co_await self->_owningApartment;
		}
		catch (hresult_error const &error)
		{
			if (!::WinUIEditor::IsApartmentDisconnected(error.code()))
				std::terminate();
			// A disconnected owner cannot run edits concurrently.
		}
	}

	uint64_t EditorUtf8Reader::Length() const
	{
		if (_closed.load())
			throw_hresult(RO_E_CLOSED);
		return _length;
	}
	uint64_t EditorUtf8Reader::Sequence() const
	{
		if (_closed.load())
			throw_hresult(RO_E_CLOSED);
		return _sequence;
	}

	Windows::Storage::Streams::IBuffer EditorUtf8Reader::Read(int64_t offset, int32_t maxBytes)
	{
		if (_closed.load())
			throw_hresult(RO_E_CLOSED);
		if (::GetCurrentThreadId() != _owningThreadId)
			throw_hresult(RPC_E_WRONG_THREAD);
		return _editor.ReadUtf8Range(offset, maxBytes);
	}

	Windows::Foundation::IAsyncOperation<Windows::Storage::Streams::IBuffer> EditorUtf8Reader::ReadAsync(int64_t offset, int32_t maxBytes)
	{
		auto lifetime = get_strong();
		co_await _owningApartment;
		co_return Read(offset, maxBytes);
	}

	void EditorUtf8Reader::ReleaseLease() noexcept
	{
		if (_leaseHeld)
		{
			_view->ReleaseReadLease();
			_leaseHeld = false;
		}
	}

	fire_and_forget EditorUtf8Reader::ReleaseLeaseAsync()
	{
		auto lifetime = get_strong();
		try
		{
			co_await resume_background();
			co_await _owningApartment;
		}
		catch (hresult_error const &error)
		{
			if (!::WinUIEditor::IsApartmentDisconnected(error.code()))
				std::terminate();
		}
		ReleaseLease();
	}

	void EditorUtf8Reader::Close()
	{
		if (_closed.exchange(true))
			return;
		if (::GetCurrentThreadId() == _owningThreadId)
			ReleaseLease();
		else
			ReleaseLeaseAsync();
	}
} // namespace winrt::WinUIEditor::implementation
