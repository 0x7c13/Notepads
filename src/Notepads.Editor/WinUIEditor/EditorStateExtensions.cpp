// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "EditorWrapper.h"
#include "EditorStateReader.h"
#include "UndoHistory.h"
#include <winrt/Windows.Security.Cryptography.h>
#include <winrt/Windows.Security.Cryptography.Core.h>

namespace winrt::WinUIEditor::implementation
{
	WinUIEditor::EditorStateReader Editor::AcquireStateReader()
	{
		auto lease = AcquireUtf8Reader();
		return make<implementation::EditorStateReader>(std::move(lease), _editor.get());
	}

	Windows::Foundation::IAsyncAction Editor::RestoreStateAsync(Windows::Storage::Streams::IInputStream stream,
		uint64_t byteLength, hstring sha256)
	{
		auto lifetime = get_strong();
		auto canceled = std::make_shared<std::atomic<bool>>(false);
		auto cancellation = co_await get_cancellation_token();
		cancellation.callback([canceled]() noexcept { canceled->store(true); });
		cancellation.enable_propagation(false);
		// This is the first suspension visible to callers. The uncancelled inner
		// operation always returns to the owner and releases its lease before this
		// action completes, including when a stream read fails or Cancel is called.
		co_await RestoreStateOperationAsync(std::move(stream), byteLength, std::move(sha256), std::move(canceled));
	}

	Windows::Foundation::IAsyncAction Editor::RestoreStateOperationAsync(Windows::Storage::Streams::IInputStream stream,
		uint64_t byteLength, hstring sha256, std::shared_ptr<std::atomic<bool>> canceled)
	{
		auto lifetime = get_strong();
		if (!stream || sha256.size() != 64) throw hresult_invalid_argument();
		auto lease = AcquireUtf8Reader();
		auto view = _editor.get();
		const auto length = lease.Length();
		const auto sequence = lease.Sequence();
		const auto revision = view->DocumentRevision();
		auto owner = apartment_context();
		std::exception_ptr failure;
		::WinUIEditor::EditorState state;
		co_await resume_background();
		try {
			::WinUIEditor::EditorStateReader reader(byteLength);
			auto hash = Windows::Security::Cryptography::Core::HashAlgorithmProvider::OpenAlgorithm(L"SHA256").CreateHash();
			Windows::Storage::Streams::Buffer buffer{65536};
			uint64_t consumed = 0;
			while (consumed < byteLength) {
				if (canceled->load()) throw hresult_canceled();
				const auto request = static_cast<uint32_t>(std::min(uint64_t{65536}, byteLength - consumed));
				auto received = co_await stream.ReadAsync(buffer, request, Windows::Storage::Streams::InputStreamOptions::Partial);
				if (!received.Length() || received.Length() > request) throw hresult_invalid_argument();
				hash.Append(received);
				reader.Feed({received.data(), received.Length()});
				consumed += received.Length();
			}
			if (canceled->load()) throw hresult_canceled();
			auto extra = co_await stream.ReadAsync(buffer, 1, Windows::Storage::Streams::InputStreamOptions::Partial);
			if (extra.Length() || Windows::Security::Cryptography::CryptographicBuffer::EncodeToHexString(hash.GetValueAndReset()) != sha256)
				throw hresult_invalid_argument();
			state = reader.Finish(length, sequence);
		} catch (...) { failure = std::current_exception(); }
		co_await owner;
		if (!failure) co_await view->WaitForTextStoreUnlockAsync();
		lease.Close();
		if (failure) std::rethrow_exception(failure);
		if (canceled->load()) throw hresult_canceled();
		if (view->IsFinalized() || view->DocumentRevision() != revision ||
			(_textLoadState && !_textLoadState->canceled.load())) throw_hresult(E_ILLEGAL_METHOD_CALL);
		view->PublishEditorState(std::move(state));
	}
}
