// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Math/NativeMathMemory.h"
#include "Resources/Math/NativeMathCoordinator.h"
#include <bit>
#include <cstring>
#include <new>

namespace Notepads::Markdown::Native
{
    namespace
    {
        constexpr uint64_t PrefixMagic = 0x4d534d4154484d45;
        constexpr uint32_t HeapStorage = UINT32_MAX;

        notepads_markdown_core_error Error() noexcept
        {
            return {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        }

        int32_t Fail(notepads_markdown_core_error *error, int32_t status, uint64_t requested = 0, uint64_t limit = 0) noexcept
        {
            if (error)
            {
                *error = Error();
                error->status = status;
                error->requested = requested;
                error->limit = limit;
            }
            return status;
        }

        bool ErrorStorage(notepads_markdown_core_error *error) noexcept
        {
            return error && error->header.abi_version == NOTEPADS_MARKDOWN_CORE_ABI_VERSION &&
                   error->header.struct_size == sizeof(notepads_markdown_core_error);
        }

        bool Alignment(size_t alignment) noexcept
        {
            return alignment != 0 && std::has_single_bit(alignment);
        }

        bool Family(MathMemoryFamily family) noexcept
        {
            return family == MathMemoryFamily::Path || family == MathMemoryFamily::Mirror ||
                   family == MathMemoryFamily::Metadata;
        }

        class Exclusive final
        {
          public:
            explicit Exclusive(SRWLOCK &lock) noexcept : _lock(lock)
            {
                AcquireSRWLockExclusive(&_lock);
            }
            ~Exclusive()
            {
                ReleaseSRWLockExclusive(&_lock);
            }

          private:
            SRWLOCK &_lock;
        };
        class Shared final
        {
          public:
            explicit Shared(SRWLOCK &lock) noexcept : _lock(lock)
            {
                AcquireSRWLockShared(&_lock);
            }
            ~Shared()
            {
                ReleaseSRWLockShared(&_lock);
            }

          private:
            SRWLOCK &_lock;
        };
    }

    struct alignas(std::max_align_t) NativeMathMemory::Prefix
    {
        uint64_t magic{PrefixMagic};
        NativeMathMemory *owner{};
        void *base{};
        uint64_t requested{};
        uint32_t slot{HeapStorage};
    };

    MathMemoryHandle::MathMemoryHandle(NativeMathMemory *value) noexcept : _value(value)
    {
    }
    MathMemoryHandle::MathMemoryHandle(MathMemoryHandle const &other) noexcept : _value(other._value)
    {
        if (_value)
            _value->AddRef();
    }
    MathMemoryHandle::MathMemoryHandle(MathMemoryHandle &&other) noexcept : _value(std::exchange(other._value, nullptr))
    {
    }
    MathMemoryHandle &MathMemoryHandle::operator=(MathMemoryHandle const &other) noexcept
    {
        if (this != &other)
        {
            auto copy = other;
            std::swap(_value, copy._value);
        }
        return *this;
    }
    MathMemoryHandle &MathMemoryHandle::operator=(MathMemoryHandle &&other) noexcept
    {
        if (this != &other)
        {
            Reset();
            _value = std::exchange(other._value, nullptr);
        }
        return *this;
    }
    MathMemoryHandle::~MathMemoryHandle()
    {
        Reset();
    }
    NativeMathMemory *MathMemoryHandle::Get() const noexcept
    {
        return _value;
    }
    NativeMathMemory *MathMemoryHandle::operator->() const noexcept
    {
        return _value;
    }
    MathMemoryHandle::operator bool() const noexcept
    {
        return _value != nullptr;
    }
    void MathMemoryHandle::Reset() noexcept
    {
        if (auto value = std::exchange(_value, nullptr))
            value->Release();
    }

    NativeMathMemory::NativeMathMemory(notepads_markdown_math_job *job, notepads_markdown_math_summary const &summary) noexcept
        : _job(job), _summary(summary), _liveBytes(sizeof(NativeMathMemory))
    {
        static_assert(sizeof(InlineSlot) == InlineSlotBytes);
        static_assert(sizeof(_inlineArena) == InlineSlots * InlineSlotBytes);
        static_assert(sizeof(Prefix) % alignof(Prefix) == 0);
    }

    MathMemoryHandle NativeMathMemory::Create(notepads_markdown_math_job *job)
    {
        MathMemoryHandle result;
        auto error = Error();
        auto status = TryCreate(job, &result, &error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            throw MathMemoryFailure{status, error};
        return result;
    }

    int32_t NativeMathMemory::TryCreate(notepads_markdown_math_job *job, MathMemoryHandle *output, notepads_markdown_core_error *error) noexcept
    {
        if (!output || !ErrorStorage(error))
            return NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
        if (*output)
            return Fail(error, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
        if (!job)
            return Fail(error, NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT);
        notepads_markdown_math_summary summary{};
        summary.header = {NOTEPADS_MARKDOWN_MATH_ABI_VERSION, sizeof(summary)};
        auto status = notepads_markdown_math_job_summary(job, &summary, error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            return status;
        if (summary.state != NOTEPADS_MARKDOWN_MATH_COMPUTED || summary.prepaid_metadata_bytes < NOTEPADS_MARKDOWN_MATH_NATIVE_JOB_HEADER_BYTES)
            return Fail(error, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
        // Includes the complete owner and the entire fixed late-inline arena.
        // The existing 4096-byte worker node is already in summary.prepaid.
        status = notepads_markdown_math_job_charge_native(job, NOTEPADS_MARKDOWN_MATH_NATIVE_METADATA, sizeof(NativeMathMemory), 0, error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            return status;
        status = notepads_markdown_math_job_retain(job, error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            return status;
        auto allocation = HeapAlloc(GetProcessHeap(), 0, sizeof(NativeMathMemory));
        if (!allocation)
        {
            auto releaseError = Error();
            (void)notepads_markdown_math_job_release(job, &releaseError);
            return Fail(error, NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY);
        }
        auto value = ::new (allocation) NativeMathMemory(job, summary);
        *output = MathMemoryHandle(value);
        *error = Error();
        return NOTEPADS_MARKDOWN_CORE_OK;
    }

    void NativeMathMemory::AddRef() noexcept
    {
        auto previous = _references.fetch_add(1, std::memory_order_relaxed);
        if (previous == 0 || previous == std::numeric_limits<size_t>::max())
            std::terminate();
    }

    void NativeMathMemory::Release() noexcept
    {
        if (_references.fetch_sub(1, std::memory_order_acq_rel) != 1)
            return;
        // Keep the DLL loaded across destruction, actual header deallocation
        // and the final Rust lease release, including its cleanup instructions.
        ModuleLifetime tail;
        auto job = _job;
        this->~NativeMathMemory();
        if (!HeapFree(GetProcessHeap(), 0, this))
        {
            // A heap-integrity failure cannot truthfully return the charge.
            // Keep the job and a permanent module pin rather than underfunding
            // still-live storage or executing a later cleanup in an unloaded DLL.
            ++winrt::get_module_lock();
            return;
        }
        auto error = Error();
        (void)notepads_markdown_math_job_release(job, &error);
        NativeMathCoordinator::NotifyResourcesAvailable();
    }

    void *NativeMathMemory::Allocate(size_t bytes, size_t alignment, MathMemoryFamily family, uint64_t slots)
    {
        void *result{};
        auto error = Error();
        auto status = TryAllocate(bytes, alignment, family, &result, &error, slots);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            throw MathMemoryFailure{status, error};
        return result;
    }

    int32_t NativeMathMemory::TryAllocate(size_t bytes, size_t alignment, MathMemoryFamily family, void **output,
                                          notepads_markdown_core_error *error, uint64_t slots) noexcept
    {
        if (!output || !ErrorStorage(error))
            return NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
        *output = nullptr;
        if (bytes == 0 || !Alignment(alignment) || !Family(family))
            return Fail(error, NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT);
        alignment = std::max(alignment, alignof(Prefix));
        if (bytes > std::numeric_limits<size_t>::max() - sizeof(Prefix) ||
            bytes + sizeof(Prefix) > std::numeric_limits<size_t>::max() - (alignment - 1))
            return Fail(error, NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW);
        auto total = bytes + sizeof(Prefix) + alignment - 1;
        Exclusive lock(_lock);
        if (_state != State::Building || _freeFailures != 0)
            return Fail(error, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
        if (total > std::numeric_limits<uint64_t>::max() - _liveBytes)
            return Fail(error, NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW);
        auto status = notepads_markdown_math_job_charge_native(_job, static_cast<uint32_t>(family), total, slots, error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            return status;
        auto base = HeapAlloc(GetProcessHeap(), 0, total);
        if (!base)
            return Fail(error, NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY);
        auto first = reinterpret_cast<uintptr_t>(base) + sizeof(Prefix);
        auto aligned = (first + alignment - 1) & ~(static_cast<uintptr_t>(alignment) - 1);
        auto prefix = reinterpret_cast<Prefix *>(aligned) - 1;
        ::new (prefix) Prefix{PrefixMagic, this, base, total, HeapStorage};
        AddRef();
        _liveBytes += total;
        *output = reinterpret_cast<void *>(aligned);
        *error = Error();
        return NOTEPADS_MARKDOWN_CORE_OK;
    }

    bool NativeMathMemory::Free(void *value) noexcept
    {
        if (!value)
            return true;
        auto prefix = reinterpret_cast<Prefix *>(value) - 1;
        if (prefix->magic != PrefixMagic || !prefix->owner)
            return false;
        auto owner = prefix->owner;
        if (prefix->slot != HeapStorage)
        {
            auto slot = prefix->slot;
            if (slot >= InlineSlots)
                return false;
            {
                Exclusive lock(owner->_lock);
                auto bit = static_cast<uint16_t>(1u << slot);
                if ((owner->_inlineUsed & bit) == 0)
                    return false;
                prefix->magic = 0;
                owner->_inlineUsed &= static_cast<uint16_t>(~bit);
            }
            owner->Release();
            return true;
        }
        auto base = prefix->base;
        auto requested = prefix->requested;
        {
            // Serialize actual free and gauge update with Finalize, so its
            // snapshot describes the same complete set of live allocations.
            Exclusive lock(owner->_lock);
            if (!HeapFree(GetProcessHeap(), 0, base))
            {
                if (owner->_freeFailures != UINT32_MAX)
                    ++owner->_freeFailures;
                if (owner->_state == State::Building)
                    owner->_state = State::Failed;
                return false;
            }
            owner->_liveBytes -= requested;
        }
        owner->Release();
        return true;
    }

    void *NativeMathMemory::AllocateInline(size_t bytes, size_t alignment)
    {
        void *result{};
        auto error = Error();
        auto status = TryAllocateInline(bytes, alignment, &result, &error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            throw MathMemoryFailure{status, error};
        return result;
    }

    int32_t NativeMathMemory::TryAllocateInline(size_t bytes, size_t alignment, void **output,
                                                notepads_markdown_core_error *error) noexcept
    {
        if (!output || !ErrorStorage(error))
            return NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
        *output = nullptr;
        if (bytes == 0 || !Alignment(alignment) || alignment > alignof(std::max_align_t))
            return Fail(error, NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT);
        alignment = std::max(alignment, alignof(Prefix));
        if (bytes > InlineSlotBytes - sizeof(Prefix) - (alignment - 1))
            return Fail(error, NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT, bytes, InlineSlotBytes - sizeof(Prefix) - (alignment - 1));
        Exclusive lock(_lock);
        if (_state == State::Failed || _freeFailures != 0)
            return Fail(error, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
        for (uint32_t slot = 0; slot < InlineSlots; ++slot)
        {
            auto bit = static_cast<uint16_t>(1u << slot);
            if ((_inlineUsed & bit) != 0)
                continue;
            auto base = _inlineArena[slot].bytes;
            auto first = reinterpret_cast<uintptr_t>(base) + sizeof(Prefix);
            auto aligned = (first + alignment - 1) & ~(static_cast<uintptr_t>(alignment) - 1);
            auto prefix = reinterpret_cast<Prefix *>(aligned) - 1;
            ::new (prefix) Prefix{PrefixMagic, this, base, InlineSlotBytes, slot};
            AddRef();
            _inlineUsed |= bit;
            *output = reinterpret_cast<void *>(aligned);
            *error = Error();
            return NOTEPADS_MARKDOWN_CORE_OK;
        }
        return Fail(error, NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT, InlineSlots + 1, InlineSlots);
    }

    int32_t NativeMathMemory::Finalize(notepads_markdown_core_error *error) noexcept
    {
        if (!ErrorStorage(error))
            return NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT;
        Exclusive lock(_lock);
        if (_state != State::Building || _freeFailures != 0)
            return Fail(error, NOTEPADS_MARKDOWN_CORE_INVALID_STATE);
        // Mark before any FFI action: a failed finish is a terminal attempt.
        _state = State::Failed;
        notepads_markdown_math_summary current{};
        current.header = {NOTEPADS_MARKDOWN_MATH_ABI_VERSION, sizeof(current)};
        auto status = notepads_markdown_math_job_summary(_job, &current, error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            return status;
        if (std::memcmp(&current.identity, &_summary.identity, sizeof(current.identity)) != 0 ||
            current.object != _summary.object || current.prepaid_metadata_bytes != _summary.prepaid_metadata_bytes)
            return Fail(error, NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE);
        if (_liveBytes > std::numeric_limits<uint64_t>::max() - current.prepaid_metadata_bytes)
            return Fail(error, NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW);
        auto actual = current.prepaid_metadata_bytes + _liveBytes;
        status = notepads_markdown_math_job_finalize_native(_job, actual, error);
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
            return status;
        _finalActual = actual;
        _state = State::Finalized;
        return NOTEPADS_MARKDOWN_CORE_OK;
    }

    uint64_t NativeMathMemory::LiveBytes() const noexcept
    {
        Shared lock(_lock);
        return _liveBytes;
    }
    uint64_t NativeMathMemory::FinalActualBytes() const noexcept
    {
        Shared lock(_lock);
        return _finalActual;
    }
    uint32_t NativeMathMemory::InlineInUse() const noexcept
    {
        Shared lock(_lock);
        return static_cast<uint32_t>(std::popcount(_inlineUsed));
    }
    uint32_t NativeMathMemory::FreeFailures() const noexcept
    {
        Shared lock(_lock);
        return _freeFailures;
    }
    bool NativeMathMemory::IsFinalized() const noexcept
    {
        Shared lock(_lock);
        return _state == State::Finalized;
    }
    notepads_markdown_math_summary NativeMathMemory::Summary() const noexcept
    {
        Shared lock(_lock);
        return _summary;
    }
}
