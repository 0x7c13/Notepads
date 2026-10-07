// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "EditorBaseControl.h"
#include "EditorWrapper.h"
#include "EditorJournalCheckpoint.h"

namespace winrt::WinUIEditor::implementation
{
	namespace
	{
		void RequireUI(com_ptr<EditorBaseControl> const &view)
		{
			if (!view)
				throw_hresult(RO_E_CLOSED);
			if (!view->Dispatcher().HasThreadAccess())
				throw_hresult(RPC_E_WRONG_THREAD);
		}
		void StopAndWait(std::shared_ptr<::WinUIEditor::NativeJournalFile> const &file) noexcept
		{
			if (!file)
				return;
			file->Stop();
			try
			{
				file->WaitStopped();
			}
			catch (...)
			{
			}
		}
	} // namespace
	uint64_t Editor::DocumentSequence()
	{
		auto view = _editor.get();
		RequireUI(view);
		return view->DocumentSequence();
	}
	void Editor::StartJournal(hstring const &path, uint64_t sequence)
	{
		auto view = _editor.get();
		RequireUI(view);
		if (_regexJob || view->HasJournal() || (_textLoadState && !_textLoadState->canceled.load()))
			throw_hresult(E_ILLEGAL_METHOD_CALL);
		if (CodePage() != Scintilla::CpUtf8)
			throw hresult_invalid_argument();
		auto file = ::WinUIEditor::NativeJournalFile::Start(std::wstring{path}, sequence, static_cast<uint64_t>(Length()));
		try
		{
			view->StartJournal(file, sequence, false);
		}
		catch (...)
		{
			StopAndWait(file);
			throw;
		}
	}
	WinUIEditor::EditorJournalCheckpoint Editor::AcquireJournalCheckpoint()
	{
		auto view = _editor.get();
		RequireUI(view);
		return make<implementation::EditorJournalCheckpoint>(view->ActiveJournalFile(), view->AcquireJournalPrefix());
	}
	Windows::Foundation::IAsyncAction Editor::StopJournalAsync()
	{
		auto lifetime = get_strong();
		auto view = _editor.get();
		RequireUI(view);
		if (_regexJob) throw_hresult(E_ILLEGAL_METHOD_CALL);
		auto files = view->StopJournal();
		if (files.empty())
			co_return;
		// Stop is already recorded on UI. The file owns its draining worker,
		// independently of whether the public asynchronous action is awaited.
		co_await resume_background();
		for (auto const &file : files)
			file->WaitStopped();
	}
	bool Editor::JournalFaulted()
	{
		auto view = _editor.get();
		RequireUI(view);
		auto file = view->ActiveJournalFile();
		return file && file->Faulted();
	}
	Windows::Foundation::IAsyncAction Editor::StartJournalFromCheckpointAsync(hstring path, WinUIEditor::EditorJournalCheckpoint checkpoint)
	{
		auto lifetime = get_strong();
		auto view = _editor.get();
		RequireUI(view);
		if (_regexJob || !checkpoint || view->HasJournal() || (_textLoadState && !_textLoadState->canceled.load()))
			throw_hresult(E_ILLEGAL_METHOD_CALL);
		auto state = std::make_shared<TextLoadState>();
		auto cancellation = co_await get_cancellation_token();
		cancellation.callback([state]() noexcept { state->canceled.store(true); });
		cancellation.enable_propagation(false);
		_textLoadState = state;
		co_await PrepareJournalOperationAsync(std::move(path), std::move(checkpoint), 0, false, state);
	}
	Windows::Foundation::IAsyncAction Editor::RotateJournalAsync(hstring path, uint64_t sequence)
	{
		auto lifetime = get_strong();
		auto view = _editor.get();
		RequireUI(view);
		if (_regexJob || !view->HasJournal() || view->DocumentSequence() != sequence || (_textLoadState && !_textLoadState->canceled.load()))
			throw_hresult(E_ILLEGAL_METHOD_CALL);
		auto state = std::make_shared<TextLoadState>();
		auto cancellation = co_await get_cancellation_token();
		cancellation.callback([state]() noexcept { state->canceled.store(true); });
		cancellation.enable_propagation(false);
		_textLoadState = state;
		co_await PrepareJournalOperationAsync(std::move(path), nullptr, sequence, true, state);
	}
	Windows::Foundation::IAsyncAction Editor::PrepareJournalOperationAsync(
		hstring path, WinUIEditor::EditorJournalCheckpoint checkpoint, uint64_t sequence, bool rotate, std::shared_ptr<TextLoadState> state)
	{
		auto lifetime = get_strong();
		auto view = _editor.get();
		RequireUI(view);
		const auto length = static_cast<uint64_t>(Length());
		const auto revision = view->DocumentRevision();
		auto owner = apartment_context();
		const auto owningThread = ::GetCurrentThreadId();
		struct LoadingState
		{
			std::shared_ptr<TextLoadState> &active;
			std::shared_ptr<TextLoadState> state;
			DWORD thread;
			~LoadingState()
			{
				if (::GetCurrentThreadId() == thread && active == state)
					active.reset();
			}
		} loading{_textLoadState, state, owningThread};
		std::shared_ptr<::WinUIEditor::NativeJournalFile> file;
		std::exception_ptr failure;
		co_await resume_background();
		try
		{
			if (checkpoint)
			{
				co_await checkpoint.FlushAsync();
				const auto prefix = get_self<implementation::EditorJournalCheckpoint>(checkpoint)->Prefix();
				if (prefix.documentByteLength != length)
					throw hresult_invalid_argument();
				sequence = prefix.sequence;
				file = ::WinUIEditor::NativeJournalFile::Import(std::wstring{path}, prefix, [state]() { return state->canceled.load(); });
			}
			else
			{
				file = ::WinUIEditor::NativeJournalFile::Start(std::wstring{path}, sequence, length);
				file->Flush(file->Capture(sequence, length));
			}
			if (state->canceled.load())
				throw hresult_canceled();
		}
		catch (...)
		{
			failure = std::current_exception();
			StopAndWait(file);
		}
		try
		{
			co_await owner;
		}
		catch (...)
		{
			StopAndWait(file);
			throw;
		}
		if (failure)
			std::rethrow_exception(failure);
		// Preparation touches only immutable recovery data. Editing remains
		// enabled; publish only if the document is still the captured revision.
		if (state->canceled.load() || _textLoadState != state || view->DocumentRevision() != revision ||
			static_cast<uint64_t>(Length()) != length)
		{
			co_await resume_background();
			StopAndWait(file);
			co_await owner;
			throw hresult_canceled();
		}
		auto previous = view->ActiveJournalFile();
		try
		{
			view->StartJournal(file, sequence, rotate);
		}
		catch (...)
		{
			failure = std::current_exception();
		}
		if (failure)
		{
			co_await resume_background();
			StopAndWait(file);
			co_await owner;
			std::rethrow_exception(failure);
		}
		// Publication has succeeded. Previous-journal write failures cannot
		// reverse the new writer or make the host retain the wrong baseline.
		if (previous)
		{
			co_await resume_background();
			StopAndWait(previous);
			co_await owner;
		}
	}
	Windows::Foundation::IAsyncActionWithProgress<uint64_t> Editor::RestoreUtf8Async(
		Windows::Storage::Streams::IInputStream stream, uint64_t length, WinUIEditor::EditorJournalCheckpoint checkpoint)
	{
		auto lifetime = get_strong();
		auto view = _editor.get();
		RequireUI(view);
		if (!stream || !checkpoint || length > INT32_MAX || view->HasJournal())
			throw hresult_invalid_argument();
		if (_textLoadState && !_textLoadState->canceled.load())
			throw_hresult(E_ILLEGAL_METHOD_CALL);
		auto state = std::make_shared<TextLoadState>();
		auto cancellation = co_await get_cancellation_token();
		auto progress = co_await get_progress_token();
		cancellation.callback([state]() noexcept { state->canceled.store(true); });
		cancellation.enable_propagation(false);
		_textLoadState = state;
		auto operation = LoadUtf8OperationAsync(std::move(stream), length, false, state, std::move(checkpoint));
		operation.Progress([progress](auto const &, uint64_t consumed) { progress(consumed); });
		co_await operation;
	}
} // namespace winrt::WinUIEditor::implementation
