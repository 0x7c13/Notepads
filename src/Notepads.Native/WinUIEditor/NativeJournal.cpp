// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include "pch.h"
#include "NativeJournal.h"
#include <fileapifromapp.h>
#pragma comment(lib, "bcrypt.lib")

namespace WinUIEditor
{
	namespace
	{
		constexpr size_t HeaderLength = 48;
		constexpr size_t FrameLength = 56;
		constexpr size_t ChunkLength = 65536;
		constexpr size_t QueueLimit = 4 * 1024 * 1024;
		void CheckNt(NTSTATUS status)
		{
			if (status < 0)
				winrt::throw_hresult(HRESULT_FROM_NT(status));
		}
		uint32_t Crc(std::string_view bytes, uint32_t value = 0)
		{
			value = ~value;
			for (unsigned char byte : bytes)
			{
				value ^= byte;
				for (int bit = 0; bit < 8; ++bit)
					value = (value >> 1) ^ (0xedb88320u & (0u - (value & 1)));
			}
			return ~value;
		}
		void Put(std::vector<uint8_t> &bytes, size_t offset, uint64_t value, size_t width)
		{
			for (size_t i = 0; i < width; ++i)
				bytes[offset + i] = static_cast<uint8_t>(value >> (8 * i));
		}
		uint64_t Get(const uint8_t *bytes, size_t offset, size_t width)
		{
			uint64_t result = 0;
			for (size_t i = 0; i < width; ++i)
				result |= uint64_t{bytes[offset + i]} << (8 * i);
			return result;
		}
		std::string_view View(std::vector<uint8_t> const &bytes)
		{
			return {reinterpret_cast<const char *>(bytes.data()), bytes.size()};
		}
		std::vector<uint8_t> Header(uint64_t sequence, uint64_t length)
		{
			std::vector<uint8_t> bytes(HeaderLength);
			memcpy(bytes.data(), "NPJRNL02", 8);
			Put(bytes, 8, 2, 4);
			Put(bytes, 12, HeaderLength, 4);
			Put(bytes, 16, sequence, 8);
			Put(bytes, 24, length, 8);
			Put(bytes, 40, Crc(View(bytes).substr(0, 40)), 4);
			return bytes;
		}
		std::vector<uint8_t> Frame(
			JournalFrameKind kind, uint64_t sequence, uint64_t position, uint64_t length, uint64_t documentLength, std::string_view text)
		{
			if (text.size() > ChunkLength)
				winrt::throw_hresult(E_INVALIDARG);
			std::vector<uint8_t> bytes(FrameLength + text.size());
			Put(bytes, 0, 0x3246524e, 4);
			Put(bytes, 4, static_cast<uint32_t>(kind), 4);
			Put(bytes, 8, sequence, 8);
			Put(bytes, 16, text.size(), 4);
			Put(bytes, 24, position, 8);
			Put(bytes, 32, length, 8);
			Put(bytes, 40, documentLength, 8);
			if (!text.empty())
				memcpy(bytes.data() + FrameLength, text.data(), text.size());
			auto checksum = Crc(View(bytes).substr(0, 48));
			checksum = Crc(View(bytes).substr(FrameLength), checksum);
			Put(bytes, 48, checksum, 4);
			return bytes;
		}
		void WriteAll(HANDLE file, std::vector<uint8_t> const &bytes)
		{
			size_t offset = 0;
			while (offset < bytes.size())
			{
				DWORD written;
				if (!WriteFile(file, bytes.data() + offset, static_cast<DWORD>(bytes.size() - offset), &written, nullptr))
					winrt::throw_last_error();
				if (!written)
					winrt::throw_hresult(HRESULT_FROM_WIN32(ERROR_WRITE_FAULT));
				offset += written;
			}
		}
		std::string Hex(std::array<uint8_t, 32> const &digest)
		{
			constexpr char digits[] = "0123456789abcdef";
			std::string result(64, '0');
			for (size_t i = 0; i < digest.size(); ++i)
			{
				result[2 * i] = digits[digest[i] >> 4];
				result[2 * i + 1] = digits[digest[i] & 15];
			}
			return result;
		}
		struct FileHandle
		{
			HANDLE value{INVALID_HANDLE_VALUE};
			explicit FileHandle(std::wstring const &path)
			{
				value = CreateFile2FromAppW(path.c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, OPEN_EXISTING, nullptr);
				if (value == INVALID_HANDLE_VALUE)
					winrt::throw_last_error();
			}
			~FileHandle()
			{
				if (value != INVALID_HANDLE_VALUE)
					CloseHandle(value);
			}
			FileHandle(FileHandle const &) = delete;
		};
		struct Hasher
		{
			BCRYPT_ALG_HANDLE algorithm{};
			BCRYPT_HASH_HANDLE hash{};
			std::vector<uint8_t> object;
			Hasher()
			{
				try
				{
					CheckNt(BCryptOpenAlgorithmProvider(&algorithm, BCRYPT_SHA256_ALGORITHM, nullptr, 0));
					DWORD length, returned;
					CheckNt(BCryptGetProperty(
						algorithm, BCRYPT_OBJECT_LENGTH, reinterpret_cast<PUCHAR>(&length), sizeof(length), &returned, 0));
					object.resize(length);
					CheckNt(BCryptCreateHash(algorithm, &hash, object.data(), length, nullptr, 0, 0));
				}
				catch (...)
				{
					if (hash)
						BCryptDestroyHash(hash);
					if (algorithm)
						BCryptCloseAlgorithmProvider(algorithm, 0);
					throw;
				}
			}
			~Hasher()
			{
				if (hash)
					BCryptDestroyHash(hash);
				if (algorithm)
					BCryptCloseAlgorithmProvider(algorithm, 0);
			}
			void Add(std::vector<uint8_t> const &bytes)
			{
				if (!bytes.empty())
					CheckNt(BCryptHashData(hash, const_cast<PUCHAR>(bytes.data()), static_cast<ULONG>(bytes.size()), 0));
			}
			std::string Finish()
			{
				std::array<uint8_t, 32> result{};
				CheckNt(BCryptFinishHash(hash, result.data(), static_cast<ULONG>(result.size()), 0));
				return Hex(result);
			}
		};
		[[noreturn]] void Corrupt()
		{
			winrt::throw_hresult(HRESULT_FROM_WIN32(ERROR_FILE_CORRUPT));
		}
		std::vector<uint8_t> ReadAll(HANDLE file, size_t length)
		{
			std::vector<uint8_t> bytes(length);
			size_t offset = 0;
			while (offset < length)
			{
				DWORD count;
				if (!ReadFile(file, bytes.data() + offset, static_cast<DWORD>(length - offset), &count, nullptr))
					winrt::throw_last_error();
				if (!count)
					Corrupt();
				offset += count;
			}
			return bytes;
		}
		using ReplayFrame = std::function<void(JournalFrameKind, uint64_t, uint64_t, std::vector<uint8_t> const &)>;
		JournalPrefix Inspect(JournalPrefix prefix, ReplayFrame const &apply = {}, std::function<bool()> const &canceled = {},
			std::optional<uint64_t> initialLength = {})
		{
			if (prefix.byteLength < HeaderLength || prefix.sha256.size() != 64 || prefix.sequence < prefix.baseSequence)
				Corrupt();
			FileHandle file(prefix.path);
			Hasher hash;
			auto header = ReadAll(file.value, HeaderLength);
			if (memcmp(header.data(), "NPJRNL02", 8) || Get(header.data(), 8, 4) != 2 || Get(header.data(), 12, 4) != HeaderLength ||
				Get(header.data(), 16, 8) != prefix.baseSequence || Get(header.data(), 24, 8) > INT32_MAX || Get(header.data(), 32, 8) ||
				Get(header.data(), 44, 4) || Get(header.data(), 40, 4) != Crc(View(header).substr(0, 40)))
				Corrupt();
			hash.Add(header);
			uint64_t length = Get(header.data(), 24, 8), beforeLength = length, sequence = prefix.baseSequence, consumed = HeaderLength;
			uint64_t maximumLength = length;
			if (initialLength && *initialLength != length)
				Corrupt();
			bool operation = false;
			while (consumed < prefix.byteLength)
			{
				if (canceled && canceled())
					throw winrt::hresult_canceled();
				if (prefix.byteLength - consumed < FrameLength)
					Corrupt();
				auto frame = ReadAll(file.value, FrameLength);
				const auto kind = static_cast<JournalFrameKind>(Get(frame.data(), 4, 4));
				const auto frameSequence = Get(frame.data(), 8, 8), count = Get(frame.data(), 16, 4);
				const auto position = Get(frame.data(), 24, 8), amount = Get(frame.data(), 32, 8), postLength = Get(frame.data(), 40, 8);
				if (Get(frame.data(), 0, 4) != 0x3246524e || count > ChunkLength || count > prefix.byteLength - consumed - FrameLength ||
					Get(frame.data(), 20, 4) || Get(frame.data(), 52, 4))
					Corrupt();
				auto payload = ReadAll(file.value, static_cast<size_t>(count));
				if (Get(frame.data(), 48, 4) != Crc(View(payload), Crc(View(frame).substr(0, 48))))
					Corrupt();
				hash.Add(frame);
				hash.Add(payload);
				if (kind == JournalFrameKind::Abort)
				{
					if (!operation || frameSequence != sequence || count || position || amount || postLength != beforeLength)
						Corrupt();
					length = beforeLength;
					operation = false;
				}
				else
				{
					if (sequence == UINT64_MAX || frameSequence != sequence + 1)
						Corrupt();
					switch (kind)
					{
					case JournalFrameKind::Begin:
						if (operation || count || position || amount || postLength)
							Corrupt();
						operation = true;
						beforeLength = length;
						break;
					case JournalFrameKind::Insert:
						if (!operation || !count || amount != count || position > length || postLength || count > INT32_MAX - length)
							Corrupt();
						length += count;
						maximumLength = std::max(maximumLength, length);
						break;
					case JournalFrameKind::Delete:
						if (!operation || count || !amount || position > length || amount > length - position || postLength)
							Corrupt();
						length -= amount;
						break;
					case JournalFrameKind::Commit:
						if (!operation || count || position || amount || postLength != length)
							Corrupt();
						operation = false;
						++sequence;
						break;
					default:
						Corrupt();
					}
				}
				if (apply)
					apply(kind, position, amount, payload);
				consumed += FrameLength + count;
			}
			if (operation || sequence != prefix.sequence)
				Corrupt();
			if (hash.Finish() != prefix.sha256)
				Corrupt();
			prefix.documentByteLength = length;
			prefix.maximumDocumentByteLength = maximumLength;
			return prefix;
		}
		void ValidateCanonical(Scintilla::Internal::Document const &document, std::function<bool()> const &canceled = {})
		{
			std::vector<char> bytes(ChunkLength);
			unsigned int remaining = 0;
			uint8_t minimum = 0x80, maximum = 0xbf;
			for (Sci::Position offset = 0; offset < document.Length();)
			{
				if (canceled && canceled())
					throw winrt::hresult_canceled();
				const auto count = std::min(static_cast<Sci::Position>(ChunkLength), document.Length() - offset);
				document.GetCharRange(bytes.data(), offset, count);
				for (Sci::Position i = 0; i < count; ++i)
				{
					const auto value = static_cast<uint8_t>(bytes[i]);
					if (remaining)
					{
						if (value < minimum || value > maximum)
							Corrupt();
						--remaining;
						minimum = 0x80;
						maximum = 0xbf;
					}
					else if (value < 0x80)
					{
						if (value == '\n')
							Corrupt();
					}
					else if (value >= 0xc2 && value <= 0xdf)
						remaining = 1;
					else if (value >= 0xe0 && value <= 0xef)
					{
						remaining = 2;
						minimum = value == 0xe0 ? 0xa0 : 0x80;
						maximum = value == 0xed ? 0x9f : 0xbf;
					}
					else if (value >= 0xf0 && value <= 0xf4)
					{
						remaining = 3;
						minimum = value == 0xf0 ? 0x90 : 0x80;
						maximum = value == 0xf4 ? 0x8f : 0xbf;
					}
					else
						Corrupt();
				}
				offset += count;
			}
			if (remaining)
				Corrupt();
		}
	} // namespace

	NativeJournalFile::NativeJournalFile(std::wstring path, uint64_t sequence, uint64_t length)
		: _path(std::move(path)), _baseSequence(sequence), _writtenSequence(sequence), _writtenDocumentLength(length)
	{
		_file = CreateFile2FromAppW(_path.c_str(), GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ, OPEN_EXISTING, nullptr);
		if (_file == INVALID_HANDLE_VALUE)
			winrt::throw_last_error();
		try
		{
			LARGE_INTEGER size;
			if (!GetFileSizeEx(_file, &size))
				winrt::throw_last_error();
			if (size.QuadPart != 0)
				winrt::throw_hresult(E_INVALIDARG);
			InitializeHash();
		}
		catch (...)
		{
			if (_hash)
				BCryptDestroyHash(_hash);
			if (_algorithm)
				BCryptCloseAlgorithmProvider(_algorithm, 0);
			CloseHandle(_file);
			throw;
		}
	}

	NativeJournalFile::~NativeJournalFile() noexcept
	{
		if (_hash)
			BCryptDestroyHash(_hash);
		if (_algorithm)
			BCryptCloseAlgorithmProvider(_algorithm, 0);
		if (_file != INVALID_HANDLE_VALUE)
			CloseHandle(_file);
	}

	void NativeJournalFile::InitializeHash()
	{
		CheckNt(BCryptOpenAlgorithmProvider(&_algorithm, BCRYPT_SHA256_ALGORITHM, nullptr, 0));
		DWORD size, returned;
		CheckNt(BCryptGetProperty(_algorithm, BCRYPT_OBJECT_LENGTH, reinterpret_cast<PUCHAR>(&size), sizeof(size), &returned, 0));
		_hashObject.resize(size);
		CheckNt(BCryptCreateHash(_algorithm, &_hash, _hashObject.data(), size, nullptr, 0, 0));
	}

	std::string NativeJournalFile::CurrentHash()
	{
		std::vector<uint8_t> copyObject(_hashObject.size());
		BCRYPT_HASH_HANDLE copy{};
		CheckNt(BCryptDuplicateHash(_hash, &copy, copyObject.data(), static_cast<ULONG>(copyObject.size()), 0));
		std::array<uint8_t, 32> digest{};
		const auto result = BCryptFinishHash(copy, digest.data(), static_cast<ULONG>(digest.size()), 0);
		BCryptDestroyHash(copy);
		CheckNt(result);
		return Hex(digest);
	}

	std::shared_ptr<NativeJournalFile> NativeJournalFile::Start(std::wstring path, uint64_t sequence, uint64_t length)
	{
		auto file = std::shared_ptr<NativeJournalFile>(new NativeJournalFile(std::move(path), sequence, length));
		file->Queue({Header(sequence, length), sequence, length, true});
		std::thread([file]() { file->Run(); }).detach();
		return file;
	}

	void NativeJournalFile::Queue(Work work, std::function<void()> const &check)
	{
		std::unique_lock guard(_mutex);
		while (SUCCEEDED(_failure) && !_stopping && _queuedBytes + work.bytes.size() > QueueLimit)
		{
			if (check) check();
			_changed.wait_for(guard, std::chrono::milliseconds(20));
		}
		if (check) check();
		if (FAILED(_failure))
			winrt::throw_hresult(_failure);
		if (_stopping)
			winrt::throw_hresult(RO_E_CLOSED);
		const auto size = work.bytes.size();
		_queue.push_back(std::move(work));
		_logicalBytes += size;
		_queuedBytes += size;
		_changed.notify_all();
	}

	void NativeJournalFile::Append(
		JournalFrameKind kind, uint64_t sequence, uint64_t position, uint64_t length, uint64_t documentLength, std::string_view bytes)
	{
		Queue({Frame(kind, sequence, position, length, documentLength, bytes), sequence, documentLength,
			kind == JournalFrameKind::Commit || kind == JournalFrameKind::Abort});
	}

	void NativeJournalFile::RecordFailure(HRESULT error) noexcept
	{
		std::lock_guard guard(_mutex);
		if (SUCCEEDED(_failure))
			_failure = error;
		_changed.notify_all();
	}
	void NativeJournalFile::AppendCancellable(JournalFrameKind kind, uint64_t sequence, uint64_t position,
		uint64_t length, uint64_t documentLength, std::string_view bytes, std::function<void()> const &check)
	{
		Queue({Frame(kind, sequence, position, length, documentLength, bytes), sequence, documentLength,
			kind == JournalFrameKind::Commit || kind == JournalFrameKind::Abort}, check);
	}
	void NativeJournalFile::WaitForQueueDrain(std::function<void()> const &check)
	{
		std::unique_lock guard(_mutex);
		while (_queuedBytes && SUCCEEDED(_failure) && !_stopping)
		{
			check();
			_changed.wait_for(guard, std::chrono::milliseconds(20));
		}
		if (FAILED(_failure)) winrt::throw_hresult(_failure);
		if (_stopping) winrt::throw_hresult(RO_E_CLOSED);
		check();
	}

	void NativeJournalFile::Run() noexcept
	{
		try
		{
			for (;;)
			{
				Work work;
				{
					std::unique_lock guard(_mutex);
					_changed.wait(guard, [&]() { return !_queue.empty() || _stopping || FAILED(_failure); });
					if (FAILED(_failure))
						winrt::throw_hresult(_failure);
					if (_queue.empty() && _stopping)
						break;
					work = std::move(_queue.front());
					_queue.pop_front();
				}
				WriteAll(_file, work.bytes);
				{
					std::lock_guard guard(_mutex);
					CheckNt(BCryptHashData(_hash, work.bytes.data(), static_cast<ULONG>(work.bytes.size()), 0));
					_writtenBytes += work.bytes.size();
					_queuedBytes -= work.bytes.size();
					if (work.commit)
					{
						_writtenSequence = work.committedSequence;
						_writtenDocumentLength = work.committedDocumentLength;
						const auto requested = _requestedHashes.find(_writtenBytes);
						if (requested != _requestedHashes.end())
							if (auto digest = requested->second.lock())
								digest->value = CurrentHash();
					}
					_changed.notify_all();
				}
			}
			if (!FlushFileBuffers(_file))
				winrt::throw_last_error();
		}
		catch (...)
		{
			RecordFailure(winrt::to_hresult());
		}
		{
			std::lock_guard guard(_mutex);
			if (_file != INVALID_HANDLE_VALUE)
			{
				CloseHandle(_file);
				_file = INVALID_HANDLE_VALUE;
			}
			_stopped = true;
			_changed.notify_all();
		}
	}

	JournalPrefix NativeJournalFile::Capture(uint64_t sequence, uint64_t documentLength)
	{
		std::lock_guard guard(_mutex);
		if (FAILED(_failure))
			winrt::throw_hresult(_failure);
		for (auto iterator = _requestedHashes.begin(); iterator != _requestedHashes.end();)
			if (iterator->second.expired())
				iterator = _requestedHashes.erase(iterator);
			else
				++iterator;
		auto digest = _requestedHashes[_logicalBytes].lock();
		if (!digest)
		{
			digest = std::make_shared<JournalDigest>();
			_requestedHashes[_logicalBytes] = digest;
		}
		if (_writtenSequence == sequence && _writtenBytes == _logicalBytes)
			digest->value = CurrentHash();
		return {_path, _baseSequence, sequence, _logicalBytes, documentLength, digest->value, digest};
	}

	JournalPrefix NativeJournalFile::Flush(JournalPrefix prefix)
	{
		std::unique_lock guard(_mutex);
		_changed.wait(guard, [&]() { return FAILED(_failure) || _writtenBytes >= prefix.byteLength || _stopped; });
		if (FAILED(_failure))
			winrt::throw_hresult(_failure);
		if (_writtenBytes < prefix.byteLength)
			winrt::throw_hresult(RO_E_CLOSED);
		if (_file != INVALID_HANDLE_VALUE && !FlushFileBuffers(_file))
			winrt::throw_last_error();
		prefix.sha256 = prefix.digest ? prefix.digest->value : prefix.sha256;
		if (prefix.sha256.empty())
			winrt::throw_hresult(E_FAIL);
		return prefix;
	}

	void NativeJournalFile::Stop() noexcept
	{
		std::lock_guard guard(_mutex);
		_stopping = true;
		_changed.notify_all();
	}
	void NativeJournalFile::WaitStopped()
	{
		std::unique_lock guard(_mutex);
		_changed.wait(guard, [&]() { return _stopped; });
		if (FAILED(_failure))
			winrt::throw_hresult(_failure);
	}
	bool NativeJournalFile::IsStopped()
	{
		std::lock_guard guard(_mutex);
		return _stopped;
	}

	JournalPrefix NativeJournalFile::Validate(std::wstring path, uint64_t baseSequence, uint64_t sequence, uint64_t byteLength,
		std::string const &sha256, std::function<bool()> const &canceled)
	{
		return Inspect({std::move(path), baseSequence, sequence, byteLength, 0, sha256, {}}, {}, canceled);
	}

	std::shared_ptr<NativeJournalFile> NativeJournalFile::Import(
		std::wstring path, JournalPrefix const &prefix, std::function<bool()> const &canceled)
	{
		const auto verified = Inspect(prefix, {}, canceled);
		if (canceled && canceled())
			throw winrt::hresult_canceled();
		auto file =
			std::shared_ptr<NativeJournalFile>(new NativeJournalFile(std::move(path), prefix.baseSequence, verified.documentByteLength));
		FileHandle source(prefix.path);
		uint64_t copied = 0;
		while (copied < prefix.byteLength)
		{
			if (canceled && canceled())
				throw winrt::hresult_canceled();
			auto bytes = ReadAll(source.value, static_cast<size_t>(std::min(uint64_t{ChunkLength}, prefix.byteLength - copied)));
			WriteAll(file->_file, bytes);
			CheckNt(BCryptHashData(file->_hash, bytes.data(), static_cast<ULONG>(bytes.size()), 0));
			copied += bytes.size();
		}
		if (file->CurrentHash() != verified.sha256)
			Corrupt();
		if (canceled && canceled())
			throw winrt::hresult_canceled();
		if (!FlushFileBuffers(file->_file))
			winrt::throw_last_error();
		file->_writtenBytes = file->_logicalBytes = copied;
		file->_writtenSequence = prefix.sequence;
		file->_writtenDocumentLength = verified.documentByteLength;
		std::thread([file]() { file->Run(); }).detach();
		return file;
	}

	void NativeJournalFile::Replay(
		Scintilla::Internal::Document &document, JournalPrefix const &prefix, std::function<bool()> const &canceled)
	{
		document.SetUndoCollection(true);
		Inspect(
			prefix,
			[&](JournalFrameKind kind, uint64_t position, uint64_t amount, std::vector<uint8_t> const &bytes)
			{
				switch (kind)
				{
				case JournalFrameKind::Begin:
					document.BeginUndoAction();
					break;
				case JournalFrameKind::Insert:
					if (document.InsertString(static_cast<Sci::Position>(position), reinterpret_cast<const char *>(bytes.data()),
							static_cast<Sci::Position>(bytes.size())) != static_cast<Sci::Position>(bytes.size()))
						winrt::throw_hresult(E_OUTOFMEMORY);
					break;
				case JournalFrameKind::Delete:
					if (!document.DeleteChars(static_cast<Sci::Position>(position), static_cast<Sci::Position>(amount)))
						winrt::throw_hresult(E_FAIL);
					break;
				case JournalFrameKind::Commit:
					document.EndUndoAction();
					document.DeleteUndoHistory();
					break;
				case JournalFrameKind::Abort:
					document.EndUndoAction();
					document.Undo();
					document.DeleteUndoHistory();
					break;
				}
			},
			canceled, static_cast<uint64_t>(document.Length()));
		ValidateCanonical(document, canceled);
		document.SetSavePoint();
	}
	void NativeJournalFile::ValidateCanonicalDocument(Scintilla::Internal::Document const &document, std::function<bool()> const &canceled)
	{
		ValidateCanonical(document, canceled);
	}

	NativeDocumentJournal::NativeDocumentJournal(
		Scintilla::Internal::Document &document, std::shared_ptr<NativeJournalFile> file, uint64_t sequence)
		: _document(&document), _file(std::move(file)), _sequence(sequence)
	{
		document.AddWatcher(this, nullptr, true);
	}
	NativeDocumentJournal::~NativeDocumentJournal()
	{
		Detach();
	}
	void NativeDocumentJournal::Detach() noexcept
	{
		if (_document)
		{
			_document->RemoveWatcher(this, nullptr);
			_document = nullptr;
		}
		RequestStop();
	}
	void NativeDocumentJournal::RequestStop() noexcept
	{
		_stopRequested = true;
		if (!_preparingReplacement) _file->Stop();
	}
	void NativeDocumentJournal::BeginReplacement()
	{
		if (!_document || _operation || _preparingReplacement || _sequence == UINT64_MAX)
			winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
		_replacementPrefix = _file->Capture(_sequence, _document->Length());
		_preparingReplacement = _operation = true;
		_replacementStaged = false;
		_replacementSequence = _sequence + 1;
		_replacementOldLength = _document->Length();
	}
	void NativeDocumentJournal::StageReplacement(Scintilla::Internal::SplitView text, std::function<void()> const &check)
	{
		_replacementNewLength = text.length;
		_file->AppendCancellable(JournalFrameKind::Begin, _replacementSequence, 0, 0, 0, {}, check);
		_replacementStaged = true;
		if (_replacementOldLength)
			_file->AppendCancellable(JournalFrameKind::Delete, _replacementSequence, 0, _replacementOldLength, 0, {}, check);
		for (size_t offset = 0; offset < text.length;)
		{
			check();
			const auto segmentEnd = offset < text.length1 ? text.length1 : text.length;
			const auto count = std::min(size_t{ChunkLength}, segmentEnd - offset);
			const char *bytes = offset < text.length1 ? text.segment1 + offset : text.segment2 + offset;
			_file->AppendCancellable(JournalFrameKind::Insert, _replacementSequence, offset, count, 0, {bytes, count}, check);
			offset += count;
		}
		_file->WaitForQueueDrain(check);
	}
	void NativeDocumentJournal::AbortReplacementOnWorker() noexcept
	{
		if (!_replacementStaged) return;
		try { _file->Append(JournalFrameKind::Abort, _replacementSequence - 1, 0, 0, _replacementOldLength); }
		catch (...) { _file->RecordFailure(winrt::to_hresult()); }
	}
	void NativeDocumentJournal::CommitReplacement() noexcept
	{
		_sequence = _replacementSequence;
		try { _file->Append(JournalFrameKind::Commit, _sequence, 0, 0, _replacementNewLength); }
		catch (...) { _file->RecordFailure(winrt::to_hresult()); }
		FinishReplacement();
	}
	void NativeDocumentJournal::FinishReplacement() noexcept
	{
		_preparingReplacement = _operation = false;
		if (_stopRequested) _file->Stop();
	}
	void NativeDocumentJournal::AbandonReplacement() noexcept
	{
		// Exceptional scheduling failure must not leave an unterminated
		// operation followed by unrelated edits in an apparently healthy file.
		if (_replacementStaged) _file->RecordFailure(E_FAIL);
		FinishReplacement();
	}
	void NativeDocumentJournal::NotifyDeleted(Scintilla::Internal::Document *, void *) noexcept
	{
		_document = nullptr;
		_file->Stop();
	}
	JournalPrefix NativeDocumentJournal::Capture()
	{
		// Autosave may retain the last complete prefix during preparation;
		// staged frames never become a recovery checkpoint before Commit.
		if (_preparingReplacement && _document) return _replacementPrefix;
		if (!_document || _operation)
			winrt::throw_hresult(E_ILLEGAL_METHOD_CALL);
		return _file->Capture(_sequence, _document->Length());
	}
	void NativeDocumentJournal::Complete() noexcept
	{
		if (!_operation || !_document)
			return;
		if (_sequence == UINT64_MAX)
		{
			_file->RecordFailure(HRESULT_FROM_WIN32(ERROR_ARITHMETIC_OVERFLOW));
			_operation = false;
			return;
		}
		++_sequence;
		try
		{
			_file->Append(JournalFrameKind::Commit, _sequence, 0, 0, _document->Length());
		}
		catch (...)
		{
			_file->RecordFailure(winrt::to_hresult());
		}
		_operation = false;
	}
	void NativeDocumentJournal::NotifyGroupCompleted(Scintilla::Internal::Document *, void *) noexcept
	{
		if (_preparingReplacement) return;
		Complete();
	}
	void NativeDocumentJournal::NotifyTransactionAborted(Scintilla::Internal::Document *, void *) noexcept
	{
		if (_preparingReplacement) return;
		if (!_operation)
			return;
		try
		{
			_file->Append(JournalFrameKind::Abort, _sequence, 0, 0, _document->Length());
		}
		catch (...)
		{
			_file->RecordFailure(winrt::to_hresult());
		}
		_operation = false;
	}
	void NativeDocumentJournal::NotifyModified(
		Scintilla::Internal::Document *document, Scintilla::Internal::DocModification modification, void *)
	{
		using namespace Scintilla;
		if (_preparingReplacement) return;
		if (!FlagSet(modification.modificationType, ModificationFlags::InsertText | ModificationFlags::DeleteText))
		{
			// Container undo actions carry no text payload, but may be the
			// final step of a group that changed text earlier in the batch.
			if (FlagSet(modification.modificationType, ModificationFlags::Undo | ModificationFlags::Redo) &&
				FlagSet(modification.modificationType, ModificationFlags::LastStepInUndoRedo))
				Complete();
			return;
		}
		try
		{
			if (!_operation)
			{
				_file->Append(JournalFrameKind::Begin, _sequence + 1, 0, 0, 0);
				_operation = true;
			}
			if (FlagSet(modification.modificationType, ModificationFlags::DeleteText))
				_file->Append(JournalFrameKind::Delete, _sequence + 1, modification.position, modification.length, 0);
			else
			{
				for (Sci::Position offset = 0; offset < modification.length;)
				{
					const auto count = std::min(static_cast<Sci::Position>(ChunkLength), modification.length - offset);
					_file->Append(JournalFrameKind::Insert, _sequence + 1, modification.position + offset, count, 0,
						{modification.text + offset, static_cast<size_t>(count)});
					offset += count;
				}
			}
		}
		catch (...)
		{
			_file->RecordFailure(winrt::to_hresult());
			_operation = true;
		}
		const bool undoRedo = FlagSet(modification.modificationType, ModificationFlags::Undo | ModificationFlags::Redo);
		if (undoRedo ? FlagSet(modification.modificationType, ModificationFlags::LastStepInUndoRedo)
					 : document->UndoSequenceDepth() == 0 && !document->InAtomicReplacement())
			Complete();
	}
} // namespace WinUIEditor
