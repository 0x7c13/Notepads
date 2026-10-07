// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#pragma once
#include <condition_variable>
#include <array>
#include <deque>
#include <future>
#include <thread>
#include <bcrypt.h>
#include "ScintillaWin.h"

namespace WinUIEditor
{
	enum class JournalFrameKind : uint32_t
	{
		Begin = 1,
		Insert = 2,
		Delete = 3,
		Commit = 4,
		Abort = 5
	};

	struct JournalDigest
	{
		std::string value;
	};
	struct JournalPrefix
	{
		std::wstring path;
		uint64_t baseSequence{};
		uint64_t sequence{};
		uint64_t byteLength{};
		uint64_t documentByteLength{};
		std::string sha256;
		std::shared_ptr<JournalDigest> digest;
		uint64_t maximumDocumentByteLength{};
	};

	class NativeJournalFile : public std::enable_shared_from_this<NativeJournalFile>
	{
	  public:
		static std::shared_ptr<NativeJournalFile> Start(std::wstring path, uint64_t baseSequence, uint64_t documentByteLength);
		~NativeJournalFile() noexcept;
		void Append(JournalFrameKind kind, uint64_t sequence, uint64_t position, uint64_t length, uint64_t documentByteLength,
			std::string_view bytes = {});
		JournalPrefix Capture(uint64_t sequence, uint64_t documentByteLength);
		JournalPrefix Flush(JournalPrefix prefix);
		void Stop() noexcept;
		void WaitStopped();
		bool IsStopped();
		bool Faulted();
		static JournalPrefix Validate(std::wstring path, uint64_t baseSequence, uint64_t sequence, uint64_t byteLength,
			std::string const &sha256, std::function<bool()> const &canceled = {});
		static std::shared_ptr<NativeJournalFile> Import(
			std::wstring path, JournalPrefix const &prefix, std::function<bool()> const &canceled = {});
		static void Replay(Scintilla::Internal::Document &document, JournalPrefix const &prefix, std::function<bool()> const &canceled);
		static void ValidateCanonicalDocument(Scintilla::Internal::Document const &document, std::function<bool()> const &canceled = {});
		void RecordFailure(HRESULT error) noexcept;
		void AppendCancellable(JournalFrameKind kind, uint64_t sequence, uint64_t position, uint64_t length,
			uint64_t documentByteLength, std::string_view bytes, std::function<void()> const &check);
		void WaitForQueueDrain(std::function<void()> const &check);

	  private:
		struct Work
		{
			std::vector<uint8_t> bytes;
			uint64_t committedSequence{};
			uint64_t committedDocumentLength{};
			bool commit{};
		};
		std::wstring _path;
		HANDLE _file{INVALID_HANDLE_VALUE};
		BCRYPT_ALG_HANDLE _algorithm{};
		BCRYPT_HASH_HANDLE _hash{};
		std::vector<uint8_t> _hashObject;
		std::mutex _mutex;
		std::condition_variable _changed;
		std::deque<Work> _queue;
		size_t _queuedBytes{};
		uint64_t _baseSequence{};
		uint64_t _logicalBytes{};
		uint64_t _writtenBytes{};
		uint64_t _flushedBytes{};
		uint64_t _writtenSequence{};
		uint64_t _writtenDocumentLength{};
		std::map<uint64_t, std::weak_ptr<JournalDigest>> _requestedHashes;
		HRESULT _failure{S_OK};
		bool _stopping{};
		bool _stopped{};
		NativeJournalFile(std::wstring path, uint64_t baseSequence, uint64_t documentByteLength);
		void InitializeHash();
		std::string CurrentHash();
		void Run() noexcept;
		void Queue(Work work, std::function<void()> const &check = {});
	};

	class NativeDocumentJournal : public Scintilla::Internal::DocWatcher
	{
	  public:
		NativeDocumentJournal(Scintilla::Internal::Document &document, std::shared_ptr<NativeJournalFile> file, uint64_t sequence);
		~NativeDocumentJournal() override;
		uint64_t Sequence() const noexcept
		{
			return _sequence;
		}
		JournalPrefix Capture();
		std::shared_ptr<NativeJournalFile> File() const noexcept
		{
			return _file;
		}
		void Detach() noexcept;
		void BeginReplacement();
		void StageReplacement(Scintilla::Internal::SplitView text, std::function<void()> const &check);
		void AbortReplacementOnWorker() noexcept;
		void CommitReplacement() noexcept;
		void FinishReplacement() noexcept;
		void AbandonReplacement() noexcept;
		bool PreparingReplacement() const noexcept { return _preparingReplacement; }
		void RequestStop() noexcept;
		void NotifyModifyAttempt(Scintilla::Internal::Document *, void *) override
		{
		}
		void NotifySavePoint(Scintilla::Internal::Document *, void *, bool) override
		{
		}
		void NotifyStyleNeeded(Scintilla::Internal::Document *, void *, Sci::Position) override
		{
		}
		void NotifyErrorOccurred(Scintilla::Internal::Document *, void *, Scintilla::Status) override
		{
		}
		void NotifyDeleted(Scintilla::Internal::Document *, void *) noexcept override;
		void NotifyModified(Scintilla::Internal::Document *, Scintilla::Internal::DocModification modification, void *) override;
		void NotifyGroupCompleted(Scintilla::Internal::Document *, void *) noexcept override;
		void NotifyTransactionAborted(Scintilla::Internal::Document *, void *) noexcept override;

	  private:
		Scintilla::Internal::Document *_document;
		std::shared_ptr<NativeJournalFile> _file;
		uint64_t _sequence;
		bool _operation{};
		bool _preparingReplacement{};
		bool _replacementStaged{};
		uint64_t _replacementSequence{};
		uint64_t _replacementOldLength{};
		uint64_t _replacementNewLength{};
		bool _stopRequested{};
		JournalPrefix _replacementPrefix;
		void Complete() noexcept;
	};
} // namespace WinUIEditor
