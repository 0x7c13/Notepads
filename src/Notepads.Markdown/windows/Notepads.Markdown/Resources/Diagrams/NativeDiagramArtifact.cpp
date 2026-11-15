// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Diagrams/NativeDiagramArtifact.h"
#include "Resources/Diagrams/NativeDiagramWorker.h"
#include <cmath>
#include <cstring>
#include <new>

namespace Notepads::Markdown::Native
{
    NativeDiagramArtifactResult NativeDiagramArtifact::BuildAfterThreadExit(notepads_markdown_diagram_job *job) noexcept
    {
        NativeDiagramArtifactResult result;
        if (!job)
        {
            result.status = NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
            result.error.status = result.status;
            return result;
        }
        result.status = notepads_markdown_diagram_job_summary(job, &result.summary, &result.error);
        if (result.status != NOTEPADS_MARKDOWN_CORE_OK)
            return result;
        auto const &summary = result.summary;
        if (summary.state != NOTEPADS_MARKDOWN_DIAGRAM_COMPUTED || !std::isfinite(summary.width) || !std::isfinite(summary.height) ||
            summary.width <= 0 || summary.height <= 0 || summary.width > 1.0e7 || summary.height > 1.0e7 ||
            summary.bytes == 0 || summary.bytes > 4 * 1024 * 1024 || summary.bytes > SIZE_MAX ||
            summary.path_commands > 65536 || summary.prepaid_metadata_bytes != NOTEPADS_MARKDOWN_DIAGRAM_QUEUED_METADATA_BYTES)
        {
            result.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
            result.error.status = result.status;
            return result;
        }
        result.status = notepads_markdown_diagram_job_charge_native(job, summary.bytes, &result.error);
        if (result.status != NOTEPADS_MARKDOWN_CORE_OK)
            return result;
        result.bytes = static_cast<uint8_t *>(HeapAlloc(GetProcessHeap(), 0, static_cast<size_t>(summary.bytes)));
        if (!result.bytes)
        {
            result.status = NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
            result.error.status = result.status;
            return result;
        }
        result.capacity = summary.bytes;
        for (uint64_t start = 0; start < summary.bytes;)
        {
            auto capacity = (std::min)(uint64_t{NOTEPADS_MARKDOWN_DIAGRAM_MAX_BATCH}, summary.bytes - start);
            notepads_markdown_diagram_read_result read{notepads_markdown_core_header{NOTEPADS_MARKDOWN_DIAGRAM_ABI_VERSION, sizeof(notepads_markdown_diagram_read_result)}};
            result.status = notepads_markdown_diagram_job_read(job, start, result.bytes + static_cast<size_t>(start), capacity, &read,
                                                &result.error);
            if (result.status != NOTEPADS_MARKDOWN_CORE_OK)
                return result;
            if (read.written != capacity || read.next != start + capacity || read.reserved != 0 ||
                read.done != static_cast<uint32_t>(read.next == summary.bytes))
            {
                result.status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                result.error.status = result.status;
                return result;
            }
            start = read.next;
        }
        // Node, presentation, dispatch delegate and stream slots are all inside
        // the original prepaid metadata. Only the exact live SVG block is new.
        result.status =
            notepads_markdown_diagram_job_finalize_native(job, NOTEPADS_MARKDOWN_DIAGRAM_QUEUED_METADATA_BYTES + result.capacity, &result.error);
        result.finalized = result.status == NOTEPADS_MARKDOWN_CORE_OK;
        return result;
    }

    namespace
    {
        HRESULT NewStream(NativeDiagramWorker::PreparedNode *owner, uint8_t const *bytes, uint64_t count,
                          uint64_t position, IStream **output) noexcept;

        class ReadOnlyStream final : public IStream
        {
          public:
            ReadOnlyStream(NativeDiagramWorker::PreparedNode *owner, uint8_t const *bytes, uint64_t count,
                           uint64_t position, uint32_t slot) noexcept
                : _owner(owner), _bytes(bytes), _count(count), _position(position), _slot(slot)
            {
            }
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (id != __uuidof(IUnknown) && id != __uuidof(ISequentialStream) && id != __uuidof(IStream) &&
                    id != __uuidof(IAgileObject))
                    return E_NOINTERFACE;
                *output = static_cast<IStream *>(this);
                AddRef();
                return S_OK;
            }
            ULONG STDMETHODCALLTYPE AddRef() noexcept override
            {
                return _references.fetch_add(1) + 1;
            }
            ULONG STDMETHODCALLTYPE Release() noexcept override
            {
                auto left = _references.fetch_sub(1, std::memory_order_acq_rel) - 1;
                if (!left)
                {
                    ModuleLifetime tail;
                    auto owner = _owner;
                    auto slot = _slot;
                    this->~ReadOnlyStream();
                    NativeDiagramWorker::ReleaseStreamStorage(owner, slot);
                }
                return left;
            }
            HRESULT STDMETHODCALLTYPE Read(void *output, ULONG count, ULONG *written) noexcept override
            {
                if (written)
                    *written = 0;
                if (!output && count)
                    return STG_E_INVALIDPOINTER;
                AcquireSRWLockExclusive(&_lock);
                auto available = _count - (std::min)(_position, _count);
                auto take = static_cast<ULONG>((std::min)(available, static_cast<uint64_t>(count)));
                if (take)
                    std::memcpy(output, _bytes + static_cast<size_t>(_position), take);
                _position += take;
                ReleaseSRWLockExclusive(&_lock);
                if (written)
                    *written = take;
                return take == count ? S_OK : S_FALSE;
            }
            HRESULT STDMETHODCALLTYPE Seek(LARGE_INTEGER movement, DWORD origin,
                                           ULARGE_INTEGER *position) noexcept override
            {
                AcquireSRWLockExclusive(&_lock);
                uint64_t base{};
                if (origin == STREAM_SEEK_CUR)
                    base = _position;
                else if (origin == STREAM_SEEK_END)
                    base = _count;
                else if (origin != STREAM_SEEK_SET)
                {
                    ReleaseSRWLockExclusive(&_lock);
                    return STG_E_INVALIDFUNCTION;
                }
                auto delta = movement.QuadPart;
                uint64_t next{};
                if (delta >= 0)
                {
                    if (static_cast<uint64_t>(delta) > UINT64_MAX - base)
                    {
                        ReleaseSRWLockExclusive(&_lock);
                        return STG_E_INVALIDFUNCTION;
                    }
                    next = base + static_cast<uint64_t>(delta);
                }
                else
                {
                    auto back = static_cast<uint64_t>(-(delta + 1)) + 1;
                    if (back > base)
                    {
                        ReleaseSRWLockExclusive(&_lock);
                        return STG_E_INVALIDFUNCTION;
                    }
                    next = base - back;
                }
                _position = next;
                ReleaseSRWLockExclusive(&_lock);
                if (position)
                    position->QuadPart = next;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE Write(void const *, ULONG, ULONG *) noexcept override
            {
                return STG_E_ACCESSDENIED;
            }
            HRESULT STDMETHODCALLTYPE SetSize(ULARGE_INTEGER) noexcept override
            {
                return STG_E_ACCESSDENIED;
            }
            HRESULT STDMETHODCALLTYPE Commit(DWORD) noexcept override
            {
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE Revert() noexcept override
            {
                return STG_E_ACCESSDENIED;
            }
            HRESULT STDMETHODCALLTYPE LockRegion(ULARGE_INTEGER, ULARGE_INTEGER, DWORD) noexcept override
            {
                return STG_E_INVALIDFUNCTION;
            }
            HRESULT STDMETHODCALLTYPE UnlockRegion(ULARGE_INTEGER, ULARGE_INTEGER, DWORD) noexcept override
            {
                return STG_E_INVALIDFUNCTION;
            }
            HRESULT STDMETHODCALLTYPE Stat(STATSTG *output, DWORD) noexcept override
            {
                if (!output)
                    return STG_E_INVALIDPOINTER;
                *output = {};
                output->type = STGTY_STREAM;
                output->cbSize.QuadPart = _count;
                output->grfMode = STGM_READ;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE Clone(IStream **output) noexcept override
            {
                AcquireSRWLockShared(&_lock);
                auto position = _position;
                ReleaseSRWLockShared(&_lock);
                return NewStream(_owner, _bytes, _count, position, output);
            }
            HRESULT STDMETHODCALLTYPE CopyTo(IStream *target, ULARGE_INTEGER wanted, ULARGE_INTEGER *read,
                                             ULARGE_INTEGER *written) noexcept override
            {
                if (read)
                    read->QuadPart = 0;
                if (written)
                    written->QuadPart = 0;
                if (!target)
                    return STG_E_INVALIDPOINTER;
                uint64_t consumed{}, produced{};
                uint8_t batch[4096];
                while (consumed < wanted.QuadPart)
                {
                    auto request = static_cast<ULONG>((std::min)(uint64_t{sizeof(batch)}, wanted.QuadPart - consumed));
                    ULONG got{}, sent{};
                    auto status = Read(batch, request, &got);
                    if (FAILED(status))
                        return status;
                    if (!got)
                        return S_FALSE;
                    consumed += got;
                    if (read)
                        read->QuadPart = consumed;
                    auto result = target->Write(batch, got, &sent);
                    produced += sent;
                    if (written)
                        written->QuadPart = produced;
                    if (FAILED(result))
                        return result;
                    if (sent != got)
                        return STG_E_MEDIUMFULL;
                    if (status == S_FALSE)
                        return S_FALSE;
                }
                return S_OK;
            }

          private:
            std::atomic<uint32_t> _references{1};
            NativeDiagramWorker::PreparedNode *_owner{};
            uint8_t const *_bytes{};
            uint64_t _count{}, _position{};
            SRWLOCK _lock = SRWLOCK_INIT;
            uint32_t _slot{};
        };
        static_assert(sizeof(ReadOnlyStream) <= NativeDiagramArtifact::StreamBytes);
        class InlineObject final : public IDWriteInlineObject
        {
          public:
            InlineObject(NativeDiagramWorker::PreparedNode *owner, uint32_t slot, float width, float height,
                         float baseline, bool terminal) noexcept
                : _owner(owner), _slot(slot), _metrics{width, height, baseline, FALSE}, _terminal(terminal)
            {
            }
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID id, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (id != __uuidof(IUnknown) && id != __uuidof(IDWriteInlineObject) && id != __uuidof(IAgileObject))
                    return E_NOINTERFACE;
                *output = static_cast<IDWriteInlineObject *>(this);
                AddRef();
                return S_OK;
            }
            ULONG STDMETHODCALLTYPE AddRef() noexcept override
            {
                return _references.fetch_add(1) + 1;
            }
            ULONG STDMETHODCALLTYPE Release() noexcept override
            {
                auto left = _references.fetch_sub(1, std::memory_order_acq_rel) - 1;
                if (!left)
                {
                    ModuleLifetime tail;
                    auto owner = _owner;
                    auto slot = _slot;
                    this->~InlineObject();
                    NativeDiagramWorker::ReleaseStreamStorage(owner, slot);
                }
                return left;
            }
            HRESULT STDMETHODCALLTYPE Draw(void *, IDWriteTextRenderer *, FLOAT, FLOAT, BOOL, BOOL,
                                           IUnknown *) noexcept override
            {
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetMetrics(DWRITE_INLINE_OBJECT_METRICS *output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = _metrics;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetOverhangMetrics(DWRITE_OVERHANG_METRICS *output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = {};
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetBreakConditions(DWRITE_BREAK_CONDITION *before,
                                                         DWRITE_BREAK_CONDITION *after) noexcept override
            {
                if (!before || !after)
                    return E_POINTER;
                *before = DWRITE_BREAK_CONDITION_MUST_BREAK;
                *after = _terminal ? DWRITE_BREAK_CONDITION_CAN_BREAK : DWRITE_BREAK_CONDITION_MUST_BREAK;
                return S_OK;
            }

          private:
            std::atomic<uint32_t> _references{1};
            NativeDiagramWorker::PreparedNode *_owner{};
            uint32_t _slot{};
            DWRITE_INLINE_OBJECT_METRICS _metrics;
            bool _terminal{};
        };
        static_assert(sizeof(InlineObject) <= NativeDiagramArtifact::StreamBytes);
        HRESULT NewStream(NativeDiagramWorker::PreparedNode *owner, uint8_t const *bytes, uint64_t count,
                          uint64_t position, IStream **output) noexcept
        {
            if (!output)
                return E_POINTER;
            *output = nullptr;
            uint32_t slot{};
            auto storage = NativeDiagramWorker::AcquireStreamStorage(owner, sizeof(ReadOnlyStream),
                                                                     alignof(ReadOnlyStream), &slot);
            if (!storage)
                return E_OUTOFMEMORY;
            *output = new (storage) ReadOnlyStream(owner, bytes, count, position, slot);
            return S_OK;
        }
    }
    HRESULT NativeDiagramArtifact::OpenReadStream(DiagramPresentationHandle const &payload, IStream **output) noexcept
    {
        if (!output)
            return E_POINTER;
        *output = nullptr;
        if (!payload || payload->State() != NativeDiagramState::Ready || !payload->Bytes() || !payload->ByteCount())
            return E_INVALIDARG;
        return NewStream(static_cast<NativeDiagramWorker::PreparedNode *>(payload->Owner()), payload->Bytes(),
                         payload->ByteCount(), 0, output);
    }
    HRESULT NativeDiagramArtifact::CreateInlineObject(DiagramPresentationHandle const &payload, double width,
                                                      double height, double baseline, bool terminal,
                                                      IDWriteInlineObject **output) noexcept
    {
        if (!output)
            return E_POINTER;
        *output = nullptr;
        if (!payload || !std::isfinite(width) || !std::isfinite(height) || !std::isfinite(baseline) || width <= 0 ||
            height <= 0 || width > 1.0e7 || height > 1.0e7 || baseline < 0 || baseline > height)
            return E_INVALIDARG;
        auto owner = static_cast<NativeDiagramWorker::PreparedNode *>(payload->Owner());
        uint32_t slot{};
        auto storage =
            NativeDiagramWorker::AcquireStreamStorage(owner, sizeof(InlineObject), alignof(InlineObject), &slot);
        if (!storage)
            return E_OUTOFMEMORY;
        *output = new (storage) InlineObject(owner, slot, static_cast<float>(width), static_cast<float>(height),
                                             static_cast<float>(baseline), terminal);
        return S_OK;
    }
}
