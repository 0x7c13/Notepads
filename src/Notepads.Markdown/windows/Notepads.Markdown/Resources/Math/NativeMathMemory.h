// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Core/NativeFailure.h"
#include "Core/ModuleLifetime.h"
#include "notepads/markdown/math.h"
#include <atomic>
#include <cstddef>
#include <cstdint>
#include <limits>
#include <memory>
#include <utility>
#include <vector>

namespace Notepads::Markdown::Native
{
    class NativeMathMemory;

    struct MathMemoryFailure : NativeFailure
    {
        notepads_markdown_core_error diagnostic;
        MathMemoryFailure(int32_t code, notepads_markdown_core_error const &error) noexcept : NativeFailure(code), diagnostic(error)
        {
        }
        explicit MathMemoryFailure(int32_t code) noexcept
            : MathMemoryFailure(code, {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error), code})
        {
        }
    };

    enum class MathMemoryFamily : uint32_t
    {
        Path = NOTEPADS_MARKDOWN_MATH_NATIVE_PATH,
        Mirror = NOTEPADS_MARKDOWN_MATH_NATIVE_MIRROR,
        Metadata = NOTEPADS_MARKDOWN_MATH_NATIVE_METADATA
    };

    // Intrusive owner: constructing/copying this handle allocates no control
    // block. Each actual allocation prefix independently keeps the owner alive.
    class MathMemoryHandle final
    {
      public:
        MathMemoryHandle() noexcept = default;
        MathMemoryHandle(MathMemoryHandle const &other) noexcept;
        MathMemoryHandle(MathMemoryHandle &&other) noexcept;
        MathMemoryHandle &operator=(MathMemoryHandle const &other) noexcept;
        MathMemoryHandle &operator=(MathMemoryHandle &&other) noexcept;
        ~MathMemoryHandle();
        NativeMathMemory *Get() const noexcept;
        NativeMathMemory *operator->() const noexcept;
        explicit operator bool() const noexcept;
        void Reset() noexcept;
        bool operator==(MathMemoryHandle const &other) const noexcept = default;

      private:
        friend class NativeMathMemory;
        explicit MathMemoryHandle(NativeMathMemory *value) noexcept;
        NativeMathMemory *_value{};
    };

    class NativeMathMemory final
    {
      public:
        static constexpr size_t InlineSlots = 16;
        // Total slot capacity includes its prefix and alignment, not 1024 bytes
        // of payload in addition to an uncharged allocation header.
        static constexpr size_t InlineSlotBytes = 1024;

        // Exactly one allocation authority per job. The private coordinator has
        // observed the actual thread handle signaled and exclusively owns all
        // post-join job calls. No other authority/FFI allocation path may build
        // native payloads for this same job in parallel.
        static MathMemoryHandle Create(notepads_markdown_math_job *job);
        static int32_t TryCreate(notepads_markdown_math_job *job, MathMemoryHandle *output, notepads_markdown_core_error *error) noexcept;
        NativeMathMemory(NativeMathMemory const &) = delete;
        NativeMathMemory &operator=(NativeMathMemory const &) = delete;

        void *Allocate(size_t bytes, size_t alignment, MathMemoryFamily family, uint64_t slots = 1);
        int32_t TryAllocate(size_t bytes, size_t alignment, MathMemoryFamily family, void **output,
                            notepads_markdown_core_error *error, uint64_t slots = 1) noexcept;
        // A pointer returned by this authority. Do not use this for arbitrary
        // memory. Free actual storage before the allocation's owner reference.
        static bool Free(void *value) noexcept;

        // Late DirectWrite inline wrappers use this prepaid fixed arena only.
        // Slot reuse after finalize cannot grow the heap or its reservation.
        void *AllocateInline(size_t bytes, size_t alignment);
        int32_t TryAllocateInline(size_t bytes, size_t alignment, void **output, notepads_markdown_core_error *error) noexcept;

        // Free all native construction temporary buffers first. One attempt;
        // success closes all ordinary heap allocation permanently. Failed
        // finalization leaves the real full job grant and worker scope held.
        int32_t Finalize(notepads_markdown_core_error *error) noexcept;
        // Sum of live HeapAlloc request sizes, including all prefixes/padding
        // and the entire arena. Allocator internals/HeapSize/RSS are separate.
        uint64_t LiveBytes() const noexcept;
        uint64_t FinalActualBytes() const noexcept;
        uint32_t InlineInUse() const noexcept;
        uint32_t FreeFailures() const noexcept;
        bool IsFinalized() const noexcept;
        notepads_markdown_math_summary Summary() const noexcept;

      private:
        friend class MathMemoryHandle;
        struct Prefix;
        struct alignas(std::max_align_t) InlineSlot
        {
            std::byte bytes[InlineSlotBytes];
        };
        enum class State : uint32_t
        {
            Building,
            Finalized,
            Failed
        };

        NativeMathMemory(notepads_markdown_math_job *job, notepads_markdown_math_summary const &summary) noexcept;
        ~NativeMathMemory() = default;
        void AddRef() noexcept;
        void Release() noexcept;

        std::atomic<size_t> _references{1};
        ModuleLifetime _module;
        uint16_t _inlineUsed{};
        notepads_markdown_math_job *_job{};
        mutable SRWLOCK _lock = SRWLOCK_INIT;
        notepads_markdown_math_summary _summary{};
        uint64_t _liveBytes{};
        uint64_t _finalActual{};
        State _state{State::Building};
        uint32_t _freeFailures{};
        InlineSlot _inlineArena[InlineSlots];
    };

    // allocate_shared's rebound combined object/control block follows this same
    // prefix and funding route. The allocator's own handle is independent of the
    // allocation being destroyed; the final deallocate never calls a dead owner.
    template <typename T> struct MathAllocator
    {
        using value_type = T;
        MathMemoryHandle memory;
        MathMemoryFamily family{MathMemoryFamily::Mirror};
        MathAllocator() noexcept = default;
        MathAllocator(MathAllocator const &) noexcept = default;
        MathAllocator &operator=(MathAllocator const &) noexcept = default;
        MathAllocator(MathAllocator &&other) noexcept : memory(other.memory), family(other.family)
        {
        }
        MathAllocator &operator=(MathAllocator &&other) noexcept
        {
            memory = other.memory;
            family = other.family;
            return *this;
        }
        explicit MathAllocator(MathMemoryHandle owner, MathMemoryFamily value = MathMemoryFamily::Mirror) noexcept
            : memory(std::move(owner)), family(value)
        {
        }
        template <typename U>
        MathAllocator(MathAllocator<U> const &other) noexcept : memory(other.memory), family(other.family)
        {
        }
        T *allocate(size_t count)
        {
            if (count == 0)
                return nullptr;
            if (count > std::numeric_limits<size_t>::max() / sizeof(T))
                throw MathMemoryFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            if (!memory)
                throw MathMemoryFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
            return static_cast<T *>(memory->Allocate(count * sizeof(T), alignof(T), family, count));
        }
        void deallocate(T *value, size_t) noexcept
        {
            (void)NativeMathMemory::Free(value);
        }
        template <typename U> bool operator==(MathAllocator<U> const &other) const noexcept
        {
            return memory == other.memory && family == other.family;
        }
    };
    template <typename T> using MathVector = std::vector<T, MathAllocator<T>>;
}
