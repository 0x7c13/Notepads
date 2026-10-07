// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once

namespace winrt::WinUIEditor::implementation
{
	// Cancellation signals a job, but publishes completion after worker/lease drain.
	template<typename TResult, typename TJob>
	struct DrainingAsyncOperation : implements<DrainingAsyncOperation<TResult, TJob>,
		Windows::Foundation::IAsyncOperation<TResult>, Windows::Foundation::IAsyncInfo>
	{
		using Handler = Windows::Foundation::AsyncOperationCompletedHandler<TResult>;
		using AsyncStatus = Windows::Foundation::AsyncStatus;
		explicit DrainingAsyncOperation(std::shared_ptr<TJob> job) : _job(std::move(job)) {}
		uint32_t Id() const noexcept { return 1; }
		AsyncStatus Status() const noexcept { std::lock_guard guard(_mutex); return _status; }
		hresult ErrorCode() const noexcept { std::lock_guard guard(_mutex); return _error; }
		void Cancel() noexcept { std::lock_guard guard(_mutex); if (_status == AsyncStatus::Started) _job->canceled.store(true); }
		void Close() const noexcept {}
		TResult GetResults() const
		{
			std::lock_guard guard(_mutex);
			if (_status == AsyncStatus::Started) throw_hresult(E_ILLEGAL_METHOD_CALL);
			check_hresult(_error);
			return _result;
		}
		Handler Completed() const { std::lock_guard guard(_mutex); return _completed ? _completed.get() : nullptr; }
		void Completed(Handler const &handler)
		{
			AsyncStatus status;
			{
				std::lock_guard guard(_mutex);
				if (_handlerAssigned) throw_hresult(E_ILLEGAL_DELEGATE_ASSIGNMENT);
				_handlerAssigned = true;
				status = _status;
				if (status == AsyncStatus::Started) { _completed = handler ? make_agile(handler) : nullptr; return; }
			}
			if (handler) handler(*this, status);
		}
		void Finish(TResult result, hresult error = S_OK) noexcept
		{
			Handler handler{nullptr};
			AsyncStatus status;
			{
				std::lock_guard guard(_mutex);
				_result = std::move(result);
				_error = error;
				_status = error == HRESULT_FROM_WIN32(ERROR_CANCELLED) ? AsyncStatus::Canceled : FAILED(error) ? AsyncStatus::Error : AsyncStatus::Completed;
				status = _status;
				try { if (_completed) handler = _completed.get(); } catch (...) {}
				_completed = nullptr;
			}
			try { if (handler) handler(*this, status); } catch (...) {}
		}
	private:
		// Runtime classes start null; default construction would activate one.
		static TResult EmptyResult() noexcept
		{
			if constexpr (std::is_convertible_v<std::nullptr_t, TResult>) return nullptr;
			else return {};
		}
		std::shared_ptr<TJob> _job;
		mutable std::mutex _mutex;
		AsyncStatus _status{AsyncStatus::Started};
		hresult _error{S_OK};
		TResult _result{EmptyResult()};
		agile_ref<Handler> _completed;
		bool _handlerAssigned{};
	};
}
