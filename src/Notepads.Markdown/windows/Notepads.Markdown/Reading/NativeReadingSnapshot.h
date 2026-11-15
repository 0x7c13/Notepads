// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Core/CoreController.h"
#include "Core/NativeFailure.h"
#include "notepads/markdown/preview.h"
#include <cstddef>
#include <cstdint>
#include <limits>
#include <memory>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace Notepads::Markdown::Native
{
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    namespace SvgRetainedTests
    {
        class MemoryAccess;
    }
#endif
    struct ReadingFailure : NativeFailure
    {
        using NativeFailure::NativeFailure;
    };

    // One checked, process-wide token per source state. Layout/theme changes reuse it.
    uint64_t NextReadingTextToken();

    struct ReadingFreeAction
    {
        void *context{};
        int32_t (*afterFree)(void *) noexcept {};
    };

    // A caller may prepay an allocation through an upper resource policy. The
    // descriptor transfers only after HeapAlloc succeeds; a failed allocation
    // leaves it intact so the caller can retire its never-allocated reservation.
    struct PreparedReadingAllocation
    {
        notepads_markdown_preview_allocation *lease{};
        ReadingFreeAction action{};
        void (*bindPhysical)(void *) noexcept {};
    };

    struct ReadingAllocationInfo
    {
        notepads_markdown_preview_allocation *lease{};
        size_t requestedTotal{};
        bool hasAction{};
        ReadingFreeAction action{};
    };

    struct ReadingFreeReceipt
    {
        uint64_t attempted{}, physicalFreed{}, rootReleased{}, physicalFailures{}, rootFailures{}, callbackFailures{};
        int32_t firstStatus{NOTEPADS_MARKDOWN_CORE_OK};
        // Diagnostic addresses only. A failed root release can follow an
        // actual native free; neither address authorizes a cleanup retry.
        void *firstFailedPayload{};
        notepads_markdown_preview_allocation *firstFailedRoot{};
        void *firstFailedCallback{};
        bool Succeeded() const noexcept
        {
            return physicalFailures == 0 && rootFailures == 0 && callbackFailures == 0 && firstStatus == NOTEPADS_MARKDOWN_CORE_OK;
        }
    };

    // Synchronous thread-local observation only. Receipts and scope links are
    // call-local; nothing is installed into a successful retained allocation.
    // Nested scopes observe their own work and the enclosing receipt also sees
    // it. No scope may cross an await or escape its thread/receipt lifetime.
    class ReadingFreeScope final
    {
      public:
        explicit ReadingFreeScope(ReadingFreeReceipt &receipt) noexcept;
        ~ReadingFreeScope();
        ReadingFreeScope(ReadingFreeScope const &) = delete;
        ReadingFreeScope &operator=(ReadingFreeScope const &) = delete;
        static void ObserveRootRelease(notepads_markdown_preview_allocation *root, int32_t status, void *payload = nullptr) noexcept;

      private:
        friend class ReadingMemory;
        enum class Event : uint8_t
        {
            Attempted,
            PhysicalFreed,
            PhysicalFailure
        };
        static void ObserveCallbackStatus(void *context, int32_t status, void *payload) noexcept;
        static void ObservePhysical(void *payload, Event event, notepads_markdown_preview_allocation *root = nullptr) noexcept;
        ReadingFreeReceipt &_receipt;
        ReadingFreeScope *_previous{};
        static thread_local ReadingFreeScope *_current;
    };

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    enum class ReadingMemoryTestEvent : uint8_t
    {
        Allocated,
        BeforeFree,
        FreeFailed,
        PhysicalFreed,
        RootReleased,
        CallbackCompleted,
        CallbackFailed
    };

    // Optional thread-local test seam. A failed-free injection never calls the
    // real HeapFree, so the original valid pointer remains available for retry.
    struct ReadingMemoryTestHooks
    {
        void *target{};
        bool failNextAllocate{};
        bool failNextSize{};
        bool failNextFree{};
        void *context{};
        void (*observe)(void *, void *, ReadingMemoryTestEvent) noexcept {};
    };

#endif
    // Owns a model and an independent same-root allocation flag. This flag funds
    // immutable storage/exports after source cancellation; it never authorizes
    // source/layout work. Builders must still use their original work flag and
    // preserve their source first-fault/current-key publication gates.
    class ReadingMemory final
    {
        struct ConstructionKey
        {
        };

      public:
        static std::shared_ptr<ReadingMemory> Create(CoreModelPin model, notepads_markdown_core_cancel *sourceFlag,
                                                     notepads_markdown_core_identity const &expected);
        ReadingMemory(ConstructionKey, CoreModelPin model, notepads_markdown_core_cancel *allocationFlag,
                      notepads_markdown_core_identity const &identity) noexcept;
        ~ReadingMemory();
        ReadingMemory(ReadingMemory const &) = delete;
        ReadingMemory &operator=(ReadingMemory const &) = delete;

        void *Allocate(size_t bytes) const;
        void *AllocatePrepared(size_t bytes, PreparedReadingAllocation &prepared) const;
        static size_t AllocationRequestBytes(size_t bytes);
        // Internal allocator provenance: memory must be a live allocation from
        // this class, exclusively borrowed until inspection/binding completes.
        static ReadingAllocationInfo InspectAllocation(void *memory) noexcept;
        static void BindFreeAction(void *memory, ReadingFreeAction action) noexcept;
        // The prefix owns the independent lease; final free needs no live model/flag.
        static void Free(void *memory) noexcept;
        static bool TryFree(void *memory) noexcept;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        static ReadingMemoryTestHooks *SetTestHooks(ReadingMemoryTestHooks *hooks) noexcept;
#endif
        CoreModelPin const &ModelOwner() const noexcept;
        notepads_markdown_core_identity const &Identity() const noexcept;

      private:
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        friend class SvgRetainedTests::MemoryAccess;
#endif
        notepads_markdown_core_model *AllocationModel() const noexcept;
        CoreModelPin _model;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        // Only the explicit test access can set this borrowed real handle. It
        // does not manufacture a CoreModelPin/ledger or any UI ownership claim.
        notepads_markdown_core_model *_borrowedTestModel{};
#endif
        notepads_markdown_core_cancel *_allocationFlag{};
        notepads_markdown_core_identity _identity{};
    };

    struct ReadingAllocationFactory
    {
        void *context{};
        void *(*allocate)(void *, ReadingMemory const &, size_t){};
    };

    template <typename T> struct ReadingAllocator
    {
        using value_type = T;
        std::shared_ptr<ReadingMemory> memory;
        // Borrowed for allocation only. Deallocation always uses the immutable
        // prefix, so retained storage never pins a completed resource job.
        ReadingAllocationFactory factory{};
        ReadingAllocator() noexcept = default;
        ReadingAllocator(ReadingAllocator const &) noexcept = default;
        ReadingAllocator &operator=(ReadingAllocator const &) noexcept = default;
        // A moved-from container can retain and reuse its allocator. Keep its
        // funding root valid when strings/descriptors are moved during sorting.
        ReadingAllocator(ReadingAllocator &&other) noexcept : memory(other.memory), factory(other.factory)
        {
        }
        ReadingAllocator &operator=(ReadingAllocator &&other) noexcept
        {
            memory = other.memory;
            factory = other.factory;
            return *this;
        }
        explicit ReadingAllocator(std::shared_ptr<ReadingMemory> owner) noexcept : memory(std::move(owner))
        {
        }
        ReadingAllocator(std::shared_ptr<ReadingMemory> owner, ReadingAllocationFactory value) noexcept
            : memory(std::move(owner)), factory(value)
        {
        }
        template <typename U>
        ReadingAllocator(ReadingAllocator<U> const &other) noexcept : memory(other.memory), factory(other.factory)
        {
        }
        T *allocate(size_t count)
        {
            static_assert(alignof(T) <= alignof(std::max_align_t));
            if (count > std::numeric_limits<size_t>::max() / sizeof(T))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            if (!memory)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
            auto bytes = count * sizeof(T);
            if (factory.allocate)
                return static_cast<T *>(factory.allocate(factory.context, *memory, bytes));
            return static_cast<T *>(memory->Allocate(bytes));
        }
        void deallocate(T *value, size_t) noexcept
        {
            ReadingMemory::Free(value);
        }
        template <typename U> bool operator==(ReadingAllocator<U> const &other) const noexcept
        {
            return memory == other.memory && factory.context == other.factory.context &&
                   factory.allocate == other.factory.allocate;
        }
    };
    template <typename T> using ReadingVector = std::vector<T, ReadingAllocator<T>>;
    using ReadingString = std::basic_string<wchar_t, std::char_traits<wchar_t>, ReadingAllocator<wchar_t>>;

    // Text only. SemanticDescriptors are a separate immutable, same-token result
    // built by the renderer from complete valid presentation batches.
    class MainTextSnapshot final
    {
        struct ConstructionKey
        {
        };

      public:
        static std::shared_ptr<MainTextSnapshot const> Build(std::shared_ptr<ReadingMemory> memory,
                                                             notepads_markdown_core_cancel *sourceFlag, uint64_t sourceGeneration,
                                                             uint64_t textGeneration);
        static std::shared_ptr<MainTextSnapshot const> Build(CoreModelPin model, notepads_markdown_core_cancel *sourceFlag,
                                                             notepads_markdown_core_identity const &expected,
                                                             uint64_t sourceGeneration, uint64_t textGeneration);
        MainTextSnapshot(ConstructionKey, std::shared_ptr<ReadingMemory> memory, ReadingVector<wchar_t> text,
                         uint64_t sourceGeneration, uint64_t textGeneration) noexcept;
        MainTextSnapshot(MainTextSnapshot const &) = delete;
        MainTextSnapshot &operator=(MainTextSnapshot const &) = delete;

        std::wstring_view Text() const noexcept;
        uint32_t Length() const noexcept;
        notepads_markdown_core_identity const &Identity() const noexcept;
        uint64_t SourceGeneration() const noexcept;
        uint64_t TextGeneration() const noexcept;
        CoreModelPin const &ModelOwner() const noexcept;
        std::shared_ptr<ReadingMemory> const &Memory() const noexcept;
        bool IsBoundary(uint32_t offset) const noexcept;
        void ValidateRange(uint32_t start, uint32_t length) const;
        // Main text only, with checked scalar endpoints and funded output capacity.
        // Clipboard object-to-alt/checkbox replacement belongs to the semantic layer.
        ReadingString CopyText(uint32_t start, uint32_t length) const;

      private:
        std::shared_ptr<ReadingMemory> _memory;
        ReadingVector<wchar_t> _text;
        uint64_t _sourceGeneration{};
        uint64_t _textGeneration{};
    };
} // namespace Notepads::Markdown::Native
