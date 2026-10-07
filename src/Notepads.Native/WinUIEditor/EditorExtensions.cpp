// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "EditorBaseControl.h"
#include "EditorWrapper.h"
#include "EditorUtf8Reader.h"
#include "EditorJournalCheckpoint.h"
#include <exception>

namespace winrt::WinUIEditor::implementation
{
	Windows::Storage::Streams::IBuffer Editor::ReadUtf8Range(int64_t offset, int32_t maxBytes)
	{
		auto view = _editor.get();
		if (!view)
			winrt::throw_hresult(RO_E_CLOSED);
		if (!view->Dispatcher().HasThreadAccess())
			winrt::throw_hresult(RPC_E_WRONG_THREAD);
		const auto length = Length();
		if (CodePage() != Scintilla::CpUtf8)
			throw hresult_invalid_argument();
		if (maxBytes < Scintilla::Internal::UTF8MaxBytes || offset < 0 || offset > length)
			throw hresult_invalid_argument();
		auto continuation = [&](int64_t position)
		{
			return Scintilla::Internal::UTF8IsTrailByte(static_cast<unsigned char>(GetCharAt(position)));
		};
		if (offset != length && continuation(offset))
			throw hresult_invalid_argument();
		auto end = offset + std::min(length - offset, static_cast<int64_t>(maxBytes));
		while (end < length && end > offset && continuation(end))
			--end;
		const auto count = static_cast<uint32_t>(end - offset);
		Windows::Storage::Streams::Buffer buffer{count};
		if (count)
		{
			Scintilla::TextRangeFull range{{offset, end}, reinterpret_cast<char *>(buffer.data())};
			// SCI_GETTEXTRANGEFULL appends a NUL, so copy from its counted
			// document range directly rather than overrunning the owned buffer.
			std::vector<char> bytes(static_cast<size_t>(count) + 1);
			range.lpstrText = bytes.data();
			view->PublicWndProc(Scintilla::Message::GetTextRangeFull, 0, reinterpret_cast<Scintilla::sptr_t>(&range));
			memcpy(buffer.data(), bytes.data(), count);
		}
		buffer.Length(count);
		return buffer;
	}

	WinUIEditor::EditorUtf8Reader Editor::AcquireUtf8Reader()
	{
		auto view = _editor.get();
		if (!view)
			winrt::throw_hresult(RO_E_CLOSED);
		if (!view->Dispatcher().HasThreadAccess())
			winrt::throw_hresult(RPC_E_WRONG_THREAD);
		if (_textLoadState && !_textLoadState->canceled.load())
			winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
		if (CodePage() != Scintilla::CpUtf8)
			throw hresult_invalid_argument();
		return make<implementation::EditorUtf8Reader>(*this, view);
	}

	Windows::Foundation::IAsyncActionWithProgress<uint64_t> Editor::LoadUtf8Async(
		Windows::Storage::Streams::IInputStream stream, uint64_t byteLength, bool preserveUndo)
	{
		auto lifetime = get_strong();
		auto view = _editor.get();
		if (!view)
			winrt::throw_hresult(RO_E_CLOSED);
		if (!view->Dispatcher().HasThreadAccess())
			winrt::throw_hresult(RPC_E_WRONG_THREAD);
		if (!stream || byteLength > INT32_MAX)
			throw hresult_invalid_argument();
		if (preserveUndo && CodePage() != Scintilla::CpUtf8)
			throw hresult_invalid_argument();
		if (_textLoadState && !_textLoadState->canceled.load())
			winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
		auto state = std::make_shared<TextLoadState>();
		auto cancellation = co_await get_cancellation_token();
		auto progress = co_await get_progress_token();
		cancellation.callback([state]() noexcept { state->canceled.store(true); });
		cancellation.enable_propagation(false);
		_textLoadState = state;
		auto operation = LoadUtf8OperationAsync(std::move(stream), byteLength, preserveUndo, std::move(state));
		operation.Progress([progress](auto const &, uint64_t consumed) { progress(consumed); });
		co_await operation;
	}

	Windows::Foundation::IAsyncActionWithProgress<uint64_t> Editor::LoadUtf8OperationAsync(Windows::Storage::Streams::IInputStream stream,
		uint64_t byteLength, bool preserveUndo, std::shared_ptr<TextLoadState> state, WinUIEditor::EditorJournalCheckpoint checkpoint)
	{
		auto lifetime = get_strong();
		auto view = _editor.get();
		if (!view)
			throw_hresult(RO_E_CLOSED);
		struct LoadingState
		{
			std::shared_ptr<TextLoadState> &active;
			std::shared_ptr<TextLoadState> owner;
			DWORD owningThreadId;
			~LoadingState()
			{
				if (::GetCurrentThreadId() == owningThreadId && active == owner)
					active.reset();
			}
		} loadingState{_textLoadState, state, ::GetCurrentThreadId()};
		auto uiThread = apartment_context();
		auto progress = co_await get_progress_token();
		// Candidate text is streamed without optional styles. Syntax admission
		// enables storage after publication; undo preservation needs no styles.
		const auto options = static_cast<Scintilla::sptr_t>(Scintilla::DocumentOption::StylesNone) |
			(preserveUndo ? view->PublicWndProc(Scintilla::Message::GetDocumentOptions, 0, 0) : 0);
		auto pointer = view->PublicWndProc(Scintilla::Message::CreateLoader, static_cast<Scintilla::uptr_t>(byteLength), options);
		if (!pointer)
			throw_hresult(E_OUTOFMEMORY);
		Scintilla::Internal::Document *nativeDocument = nullptr;
		bool loaderOwnsIndex = false;
		auto releaseLoader = [&](Scintilla::ILoader *loader)
		{
			if (loaderOwnsIndex)
				nativeDocument->ReleaseLineCharacterIndex(Scintilla::LineCharacterIndexType::Utf16);
			loader->Release();
		};
		std::unique_ptr<Scintilla::ILoader, decltype(releaseLoader)> loader(reinterpret_cast<Scintilla::ILoader *>(pointer), releaseLoader);
		co_await resume_background();
		std::exception_ptr failure;
		try
		{
			uint64_t consumed = 0;
			unsigned int continuationBytes = 0;
			uint8_t nextMin = 0x80, nextMax = 0xbf;
			Windows::Storage::Streams::Buffer buffer{65536};
			while (consumed < byteLength)
			{
				if (state->canceled.load())
					throw hresult_canceled();
				const auto request = static_cast<uint32_t>(std::min(uint64_t{buffer.Capacity()}, byteLength - consumed));
				auto received = co_await stream.ReadAsync(buffer, request, Windows::Storage::Streams::InputStreamOptions::Partial);
				const auto count = received.Length();
				if (count == 0 || count > request)
					throw_hresult(HRESULT_FROM_WIN32(ERROR_HANDLE_EOF));
				for (uint32_t i = 0; i < count; ++i)
				{
					const uint8_t value = received.data()[i];
					if (continuationBytes)
					{
						if (value < nextMin || value > nextMax)
							throw hresult_invalid_argument();
						--continuationBytes;
						nextMin = 0x80;
						nextMax = 0xbf;
					}
					else if (value < 0x80)
					{
						if (value == '\n')
							throw hresult_invalid_argument();
					}
					else if (value >= 0xc2 && value <= 0xdf)
						continuationBytes = 1;
					else if (value >= 0xe0 && value <= 0xef)
					{
						continuationBytes = 2;
						nextMin = value == 0xe0 ? 0xa0 : 0x80;
						nextMax = value == 0xed ? 0x9f : 0xbf;
					}
					else if (value >= 0xf0 && value <= 0xf4)
					{
						continuationBytes = 3;
						nextMin = value == 0xf0 ? 0x90 : 0x80;
						nextMax = value == 0xf4 ? 0x8f : 0xbf;
					}
					else
						throw hresult_invalid_argument();
				}
				const auto status = loader->AddData(reinterpret_cast<const char *>(received.data()), count);
				if (status == static_cast<int>(Scintilla::Status::BadAlloc))
					throw_hresult(E_OUTOFMEMORY);
				if (status != static_cast<int>(Scintilla::Status::Ok))
					throw_hresult(E_FAIL);
				consumed += count;
				progress(consumed);
			}
			if (continuationBytes)
				throw hresult_invalid_argument();
			if (state->canceled.load())
				throw hresult_canceled();
			auto remainder = co_await stream.ReadAsync(buffer, 1, Windows::Storage::Streams::InputStreamOptions::Partial);
			if (remainder.Length() != 0)
				throw hresult_invalid_argument();
			auto document = loader->ConvertToDocument();
			nativeDocument = static_cast<Scintilla::Internal::Document *>(static_cast<Scintilla::IDocumentEditable *>(document));
			nativeDocument->SetDBCSCodePage(Scintilla::CpUtf8);
			nativeDocument->eolMode = Scintilla::EndOfLine::Cr;
			if (checkpoint)
			{
				co_await checkpoint.FlushAsync();
				auto prefix = get_self<implementation::EditorJournalCheckpoint>(checkpoint)->Prefix();
				::WinUIEditor::NativeJournalFile::Replay(*nativeDocument, prefix, [state]() { return state->canceled.load(); });
			}
			nativeDocument->AllocateLineCharacterIndex(Scintilla::LineCharacterIndexType::Utf16);
			loaderOwnsIndex = true;
		}
		catch (...)
		{
			failure = std::current_exception();
		}
		co_await uiThread;
		if (failure)
			std::rethrow_exception(failure);
		// Stream publication must dispatch synchronously after a TSF batch
		// retires; its notification queue would otherwise copy the full edit.
		co_await view->WaitForTextStoreUnlockAsync();
		progress(byteLength);
		if (state->canceled.load() || _textLoadState != state)
			throw hresult_canceled();
		if (preserveUndo)
		{
			view->ReplaceDocumentWithUndo(*nativeDocument);
			view->PublicWndProc(Scintilla::Message::SetEOLMode, static_cast<Scintilla::uptr_t>(Scintilla::EndOfLine::Cr), 0);
		}
		else
		{
			view->SetLoadedDocument(*nativeDocument);
			view->PublicWndProc(Scintilla::Message::SetUndoCollection, 1, 0);
			view->PublicWndProc(Scintilla::Message::SetSavePoint, 0, 0);
		}
	}
	void Editor::PasteText(hstring const &text)
	{
		const auto view{_editor.get()};
		if (!view)
			winrt::throw_hresult(RO_E_CLOSED);
		view->PasteText(text);
	}

} // namespace winrt::WinUIEditor::implementation
