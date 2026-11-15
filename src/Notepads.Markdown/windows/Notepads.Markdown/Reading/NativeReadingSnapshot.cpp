// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Reading/NativeReadingSnapshot.h"
#include "Core/ModuleLifetime.h"
#include <atomic>
#include <cstring>
#include <new>
#include <utility>

namespace Notepads::Markdown::Native
{
    namespace
    {
        template <typename T> T CorePod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }

        void Check(int32_t status)
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw ReadingFailure{status};
        }

        void MatchIdentity(notepads_markdown_core_identity const &expected, notepads_markdown_core_identity const &actual)
        {
            if (std::memcmp(expected.document, actual.document, sizeof(expected.document)) != 0)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_WRONG_DOCUMENT};
            if (expected.revision != actual.revision)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_WRONG_REVISION};
            if (expected.generation != actual.generation)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_WRONG_GENERATION};
            if (expected.profile != actual.profile || expected.extensions != actual.extensions)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_STALE_RESULT};
        }

        struct alignas(std::max_align_t) ReadingPrefix
        {
            ModuleLifetime module;
            notepads_markdown_preview_allocation *lease;
            ReadingFreeAction action;
            size_t requestedTotal;
        };
        static_assert(sizeof(ReadingPrefix) % alignof(std::max_align_t) == 0);

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        thread_local ReadingMemoryTestHooks *memoryTestHooks{};

        void ObserveMemory(void *memory, ReadingMemoryTestEvent event) noexcept
        {
            if (memoryTestHooks && memoryTestHooks->observe &&
                (!memoryTestHooks->target || memoryTestHooks->target == memory))
                memoryTestHooks->observe(memoryTestHooks->context, memory, event);
        }
#endif

        void *AllocatePrepared(notepads_markdown_core_model *model, notepads_markdown_core_cancel *flag, size_t bytes,
                               PreparedReadingAllocation &prepared)
        {
            if (!model || !flag || !prepared.lease || !prepared.action.context || !prepared.action.afterFree ||
                !prepared.bindPhysical)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            auto total = ReadingMemory::AllocationRequestBytes(bytes);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            bool fail = memoryTestHooks && std::exchange(memoryTestHooks->failNextAllocate, false);
            auto memory = fail ? nullptr : ::HeapAlloc(::GetProcessHeap(), 0, total);
#else
            auto memory = ::HeapAlloc(::GetProcessHeap(), 0, total);
#endif
            if (!memory)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY};
            auto lease = prepared.lease;
            auto action = prepared.action;
            auto bind = prepared.bindPhysical;
            auto prefix = ::new (memory) ReadingPrefix{{}, lease, action, total};
            // From this instruction the complete physical owner is in prefix.
            // No fallible work may precede this ownership transfer and binding.
            prepared = {};
            bind(action.context);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            ObserveMemory(prefix + 1, ReadingMemoryTestEvent::Allocated);
#endif
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            bool failSize = memoryTestHooks && std::exchange(memoryTestHooks->failNextSize, false);
            size_t actual = failSize ? static_cast<SIZE_T>(-1) : ::HeapSize(::GetProcessHeap(), 0, memory);
#else
            size_t actual = ::HeapSize(::GetProcessHeap(), 0, memory);
#endif
            if (actual == static_cast<SIZE_T>(-1) || actual < total)
            {
                ReadingMemory::Free(prefix + 1);
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            }
            auto error = CorePod<notepads_markdown_core_error>();
            auto status = notepads_markdown_preview_model_admit(model, flag, lease, actual, &error);
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
            {
                ReadingMemory::Free(prefix + 1);
                throw ReadingFailure{status};
            }
            return prefix + 1;
        }

        void *Allocate(notepads_markdown_core_model *model, notepads_markdown_core_cancel *flag, size_t bytes)
        {
            if (!model || !flag)
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            auto total = ReadingMemory::AllocationRequestBytes(bytes);
            auto error = CorePod<notepads_markdown_core_error>();
            notepads_markdown_preview_allocation *lease{};
            Check(notepads_markdown_preview_model_reserve(model, flag, total, &lease, &error));
            auto memory = ::HeapAlloc(::GetProcessHeap(), 0, total);
            if (!memory)
            {
                (void)notepads_markdown_preview_allocation_destroy(lease);
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY};
            }
            auto prefix = ::new (memory) ReadingPrefix{{}, lease, {}, total};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            ObserveMemory(prefix + 1, ReadingMemoryTestEvent::Allocated);
#endif
            size_t actual = ::HeapSize(::GetProcessHeap(), 0, memory);
            if (actual == static_cast<SIZE_T>(-1) || actual < total)
            {
                ReadingMemory::Free(prefix + 1);
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            }
            auto status = notepads_markdown_preview_model_admit(model, flag, lease, actual, &error);
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
            {
                ReadingMemory::Free(prefix + 1);
                throw ReadingFailure{status};
            }
            return prefix + 1;
        }

        // The bootstrap control block is funded by the caller's work flag. Its
        // final deallocation uses only the independently owned allocation prefix.
        template <typename T> struct BootstrapAllocator
        {
            using value_type = T;
            notepads_markdown_core_model *model;
            notepads_markdown_core_cancel *flag;
            BootstrapAllocator(notepads_markdown_core_model *value, notepads_markdown_core_cancel *work) noexcept : model(value), flag(work)
            {
            }
            template <typename U>
            BootstrapAllocator(BootstrapAllocator<U> const &other) noexcept : model(other.model), flag(other.flag)
            {
            }
            T *allocate(size_t count)
            {
                static_assert(alignof(T) <= alignof(std::max_align_t));
                if (count > std::numeric_limits<size_t>::max() / sizeof(T))
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
                return static_cast<T *>(Allocate(model, flag, count * sizeof(T)));
            }
            void deallocate(T *value, size_t) noexcept
            {
                ReadingMemory::Free(value);
            }
            template <typename U> bool operator==(BootstrapAllocator<U> const &other) const noexcept
            {
                return model == other.model && flag == other.flag;
            }
        };

        class CancelOwner
        {
          public:
            notepads_markdown_core_cancel *value{};
            ~CancelOwner()
            {
                if (value)
                    (void)notepads_markdown_core_cancel_destroy(value);
            }
        };

        class StreamOwner
        {
          public:
            notepads_markdown_core_stream *value{};
            ~StreamOwner()
            {
                if (value)
                    (void)notepads_markdown_core_stream_destroy(value);
            }
            void Close()
            {
                Check(notepads_markdown_core_stream_destroy(std::exchange(value, nullptr)));
            }
        };

        bool HighSurrogate(wchar_t value) noexcept
        {
            return value >= 0xD800 && value <= 0xDBFF;
        }

        bool LowSurrogate(wchar_t value) noexcept
        {
            return value >= 0xDC00 && value <= 0xDFFF;
        }
    }

    uint64_t NextReadingTextToken()
    {
        static std::atomic<uint64_t> next{};
        auto previous = next.load(std::memory_order_relaxed);
        for (;;)
        {
            if (previous == std::numeric_limits<uint64_t>::max())
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            if (next.compare_exchange_weak(previous, previous + 1, std::memory_order_relaxed))
                return previous + 1;
        }
    }

    thread_local ReadingFreeScope *ReadingFreeScope::_current{};
    ReadingFreeScope::ReadingFreeScope(ReadingFreeReceipt &receipt) noexcept
        : _receipt(receipt), _previous(std::exchange(_current, this))
    {
    }
    ReadingFreeScope::~ReadingFreeScope()
    {
        _current = _previous;
    }
    void ReadingFreeScope::ObservePhysical(void *payload, Event event, notepads_markdown_preview_allocation *root) noexcept
    {
        for (auto scope = _current; scope; scope = scope->_previous)
        {
            auto &receipt = scope->_receipt;
            switch (event)
            {
            case Event::Attempted:
                ++receipt.attempted;
                break;
            case Event::PhysicalFreed:
                ++receipt.physicalFreed;
                break;
            case Event::PhysicalFailure:
                ++receipt.physicalFailures;
                if (receipt.firstStatus == NOTEPADS_MARKDOWN_CORE_OK)
                {
                    receipt.firstStatus = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                    receipt.firstFailedPayload = payload;
                    receipt.firstFailedRoot = root;
                }
                break;
            }
        }
    }
    void ReadingFreeScope::ObserveRootRelease(notepads_markdown_preview_allocation *root, int32_t status, void *payload) noexcept
    {
        for (auto scope = _current; scope; scope = scope->_previous)
        {
            auto &receipt = scope->_receipt;
            if (status == NOTEPADS_MARKDOWN_CORE_OK)
                ++receipt.rootReleased;
            else
            {
                ++receipt.rootFailures;
                if (receipt.firstStatus == NOTEPADS_MARKDOWN_CORE_OK)
                {
                    receipt.firstStatus = status;
                    receipt.firstFailedPayload = payload;
                    receipt.firstFailedRoot = root;
                }
            }
        }
    }

    void ReadingFreeScope::ObserveCallbackStatus(void *context, int32_t status, void *payload) noexcept
    {
        if (status == NOTEPADS_MARKDOWN_CORE_OK)
            return;
        for (auto scope = _current; scope; scope = scope->_previous)
        {
            auto &receipt = scope->_receipt;
            ++receipt.callbackFailures;
            if (receipt.firstStatus == NOTEPADS_MARKDOWN_CORE_OK)
            {
                receipt.firstStatus = status;
                receipt.firstFailedPayload = payload;
                receipt.firstFailedCallback = context;
            }
        }
    }

    std::shared_ptr<ReadingMemory> ReadingMemory::Create(CoreModelPin model, notepads_markdown_core_cancel *sourceFlag,
                                                         notepads_markdown_core_identity const &expected)
    {
        if (!model || !sourceFlag)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        auto summary = CorePod<notepads_markdown_core_model_summary_result>();
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_core_model_summary(model.Get(), &summary, &error));
        MatchIdentity(expected, summary.identity);
        CancelOwner allocation;
        Check(notepads_markdown_core_model_cancel_create(model.Get(), &allocation.value, &error));
        auto allocator = BootstrapAllocator<ReadingMemory>{model.Get(), sourceFlag};
        auto result = std::allocate_shared<ReadingMemory>(allocator, ConstructionKey{}, std::move(model),
                                                          allocation.value, summary.identity);
        allocation.value = nullptr;
        return result;
    }

    ReadingMemory::ReadingMemory(ConstructionKey, CoreModelPin model, notepads_markdown_core_cancel *allocationFlag,
                                 notepads_markdown_core_identity const &identity) noexcept
        : _model(std::move(model)), _allocationFlag(allocationFlag), _identity(identity)
    {
    }

    ReadingMemory::~ReadingMemory()
    {
        if (_allocationFlag)
            (void)notepads_markdown_core_cancel_destroy(_allocationFlag);
        _model.Reset();
    }

    void *ReadingMemory::Allocate(size_t bytes) const
    {
        return Native::Allocate(AllocationModel(), _allocationFlag, bytes);
    }

    void *ReadingMemory::AllocatePrepared(size_t bytes, PreparedReadingAllocation &prepared) const
    {
        return Native::AllocatePrepared(AllocationModel(), _allocationFlag, bytes, prepared);
    }

    notepads_markdown_core_model *ReadingMemory::AllocationModel() const noexcept
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        return _borrowedTestModel ? _borrowedTestModel : _model.Get();
#else
        return _model.Get();
#endif
    }

    size_t ReadingMemory::AllocationRequestBytes(size_t bytes)
    {
        if (bytes > std::numeric_limits<size_t>::max() - sizeof(ReadingPrefix))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
        return bytes + sizeof(ReadingPrefix);
    }

    ReadingAllocationInfo ReadingMemory::InspectAllocation(void *memory) noexcept
    {
        if (!memory)
            return {};
        auto prefix = static_cast<ReadingPrefix *>(memory) - 1;
        return {prefix->lease, prefix->requestedTotal, prefix->action.context != nullptr, prefix->action};
    }

    void ReadingMemory::BindFreeAction(void *memory, ReadingFreeAction action) noexcept
    {
        auto prefix = static_cast<ReadingPrefix *>(memory) - 1;
        // The exclusive caller validated the live prefix before completing its
        // infallible attribution commit; no other caller can bind or free here.
        prefix->action = action;
    }

    void ReadingMemory::Free(void *memory) noexcept
    {
        (void)TryFree(memory);
    }

    bool ReadingMemory::TryFree(void *memory) noexcept
    {
        if (!memory)
            return true;
        ModuleLifetime module;
        auto prefix = static_cast<ReadingPrefix *>(memory) - 1;
        auto lease = prefix->lease;
        auto action = prefix->action;
        auto requested = prefix->requestedTotal;
        ReadingFreeScope::ObservePhysical(memory, ReadingFreeScope::Event::Attempted);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        ObserveMemory(memory, ReadingMemoryTestEvent::BeforeFree);
#endif
        prefix->~ReadingPrefix();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        bool fail = memoryTestHooks && (!memoryTestHooks->target || memoryTestHooks->target == memory) &&
                    std::exchange(memoryTestHooks->failNextFree, false);
        if (!fail && ::HeapFree(::GetProcessHeap(), 0, prefix))
#else
        if (::HeapFree(::GetProcessHeap(), 0, prefix))
#endif
        {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            ObserveMemory(memory, ReadingMemoryTestEvent::PhysicalFreed);
#endif
            ReadingFreeScope::ObservePhysical(memory, ReadingFreeScope::Event::PhysicalFreed);
            // Native bytes are gone before the Rust reservation is retired.
            auto rootStatus = notepads_markdown_preview_allocation_destroy(lease);
            ReadingFreeScope::ObserveRootRelease(lease, rootStatus, memory);
            if (rootStatus != NOTEPADS_MARKDOWN_CORE_OK)
            {
                // The payload is already gone. Preserve module code for the
                // uncertain root owner, report failure and never claim that
                // afterFree's root-destroyed precondition was satisfied.
                ++winrt::get_module_lock();
                return true;
            }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            ObserveMemory(memory, ReadingMemoryTestEvent::RootReleased);
#endif
            // The callback owns its canonical record independently of prefix.
            // Its P+L policy attribution can retire only after the original
            // root allocation record itself was actually deallocated above.
            if (action.afterFree)
            {
                auto callbackStatus = action.afterFree(action.context);
                ReadingFreeScope::ObserveCallbackStatus(action.context, callbackStatus, memory);
                if (callbackStatus != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    // Native/root bytes may already be gone. The address is a
                    // diagnostic only; never retry free or access its context.
                    ++winrt::get_module_lock();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    ObserveMemory(memory, ReadingMemoryTestEvent::CallbackFailed);
#endif
                    return true;
                }
            }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            ObserveMemory(memory, ReadingMemoryTestEvent::CallbackCompleted);
#endif
            return true;
        }
        else
        {
            // A failed native free must retain its funding and module ownership.
            ::new (prefix) ReadingPrefix{{}, lease, action, requested};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            ObserveMemory(memory, ReadingMemoryTestEvent::FreeFailed);
#endif
            ReadingFreeScope::ObservePhysical(memory, ReadingFreeScope::Event::PhysicalFailure, lease);
            return false;
        }
    }

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    ReadingMemoryTestHooks *ReadingMemory::SetTestHooks(ReadingMemoryTestHooks *hooks) noexcept
    {
        return std::exchange(memoryTestHooks, hooks);
    }
#endif

    CoreModelPin const &ReadingMemory::ModelOwner() const noexcept
    {
        return _model;
    }

    notepads_markdown_core_identity const &ReadingMemory::Identity() const noexcept
    {
        return _identity;
    }

    std::shared_ptr<MainTextSnapshot const> MainTextSnapshot::Build(std::shared_ptr<ReadingMemory> memory,
                                                                    notepads_markdown_core_cancel *sourceFlag,
                                                                    uint64_t sourceGeneration, uint64_t textGeneration)
    {
        static_assert(sizeof(wchar_t) == sizeof(uint16_t));
        if (!memory || !sourceFlag || sourceGeneration == 0 || textGeneration == 0)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        auto model = memory->ModelOwner().Get();
        auto summary = CorePod<notepads_markdown_core_model_summary_result>();
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_core_model_summary(model, &summary, &error));
        MatchIdentity(memory->Identity(), summary.identity);
        if (summary.main_utf16 > static_cast<uint64_t>(std::numeric_limits<int32_t>::max()))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT};

        StreamOwner stream;
        Check(notepads_markdown_core_stream_create(model, sourceFlag, &stream.value, &error));
        ReadingVector<wchar_t> text{ReadingAllocator<wchar_t>{memory}};
        text.resize(static_cast<size_t>(summary.main_utf16));
        ReadingVector<uint8_t> batch{ReadingAllocator<uint8_t>{memory}};
        batch.resize(65536);
        size_t offset{};
        for (;;)
        {
            auto read = CorePod<notepads_markdown_core_read_result>();
            Check(notepads_markdown_core_stream_read(stream.value, batch.data(), batch.size(), &read, &error));
            if (read.written > batch.size() || read.next > summary.main_utf16 || read.reserved != 0 || read.done > 1 ||
                (read.written == 0 && read.done == 0))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            int converted{};
            if (read.written != 0)
            {
                converted =
                    ::MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, reinterpret_cast<char const *>(batch.data()),
                                          static_cast<int>(read.written), nullptr, 0);
                if (converted == 0)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_UTF8};
                if (static_cast<size_t>(converted) > text.size() - offset)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            }
            if (read.next != offset + static_cast<size_t>(converted))
                throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
            if (converted != 0)
            {
                auto written =
                    ::MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, reinterpret_cast<char const *>(batch.data()),
                                          static_cast<int>(read.written), text.data() + offset, converted);
                if (written != converted)
                    throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_UTF8};
                offset += static_cast<size_t>(converted);
            }
            if (read.done != 0)
                break;
        }
        if (offset != text.size())
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        stream.Close();

        // Funding the immutable record with the original source flag is the
        // final same-root/cancel checkpoint after decoding; nothing fallible
        // follows its construction. Old-data exports use ReadingMemory instead.
        auto allocator = BootstrapAllocator<MainTextSnapshot>{model, sourceFlag};
        return std::allocate_shared<MainTextSnapshot>(allocator, ConstructionKey{}, std::move(memory), std::move(text),
                                                      sourceGeneration, textGeneration);
    }

    std::shared_ptr<MainTextSnapshot const> MainTextSnapshot::Build(CoreModelPin model, notepads_markdown_core_cancel *sourceFlag,
                                                                    notepads_markdown_core_identity const &expected,
                                                                    uint64_t sourceGeneration, uint64_t textGeneration)
    {
        return Build(ReadingMemory::Create(std::move(model), sourceFlag, expected), sourceFlag, sourceGeneration,
                     textGeneration);
    }

    MainTextSnapshot::MainTextSnapshot(ConstructionKey, std::shared_ptr<ReadingMemory> memory,
                                       ReadingVector<wchar_t> text, uint64_t sourceGeneration,
                                       uint64_t textGeneration) noexcept
        : _memory(std::move(memory)), _text(std::move(text)), _sourceGeneration(sourceGeneration),
          _textGeneration(textGeneration)
    {
    }

    std::wstring_view MainTextSnapshot::Text() const noexcept
    {
        return _text.empty() ? std::wstring_view{} : std::wstring_view{_text.data(), _text.size()};
    }

    uint32_t MainTextSnapshot::Length() const noexcept
    {
        return static_cast<uint32_t>(_text.size());
    }

    notepads_markdown_core_identity const &MainTextSnapshot::Identity() const noexcept
    {
        return _memory->Identity();
    }

    uint64_t MainTextSnapshot::SourceGeneration() const noexcept
    {
        return _sourceGeneration;
    }

    uint64_t MainTextSnapshot::TextGeneration() const noexcept
    {
        return _textGeneration;
    }

    CoreModelPin const &MainTextSnapshot::ModelOwner() const noexcept
    {
        return _memory->ModelOwner();
    }

    std::shared_ptr<ReadingMemory> const &MainTextSnapshot::Memory() const noexcept
    {
        return _memory;
    }

    bool MainTextSnapshot::IsBoundary(uint32_t offset) const noexcept
    {
        return offset <= _text.size() && (offset == 0 || offset == _text.size() || !HighSurrogate(_text[offset - 1]) ||
                                          !LowSurrogate(_text[offset]));
    }

    void MainTextSnapshot::ValidateRange(uint32_t start, uint32_t length) const
    {
        if (start > _text.size() || length > _text.size() - start)
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_RANGE};
        if (!IsBoundary(start) || !IsBoundary(start + length))
            throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_INVALID_BOUNDARY};
    }

    ReadingString MainTextSnapshot::CopyText(uint32_t start, uint32_t length) const
    {
        ValidateRange(start, length);
        auto allocator = ReadingAllocator<wchar_t>{_memory};
        if (length == 0)
            return ReadingString{allocator};
        return ReadingString{_text.data() + start, length, allocator};
    }
}
