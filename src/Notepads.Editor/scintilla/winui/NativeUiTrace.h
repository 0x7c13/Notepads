// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once

#if defined(NOTEPADS_EDITOR_DIAGNOSTICS)
#include <windows.h>
#include <array>
#include <atomic>
#include <cstdint>
#include <limits>
#include <utility>

namespace Notepads::Editor::Diagnostics::UiTrace
{
    inline constexpr uint32_t AbiVersion = 1;
    inline constexpr uint32_t Capacity = 8192;
    inline constexpr uint32_t WordsPerRecord = 6;
    inline constexpr uint32_t MaximumWords = Capacity * WordsPerRecord;
    enum class Phase : uint64_t { IdleTick = 1, UpdatesNeeded = 2 };

    // One completed interval per record, in completion order:
    // phase, scope id, parent scope id, thread id, begin QPC, end QPC.
    using Record = std::array<uint64_t, WordsPerRecord>;
    inline std::array<Record, Capacity> records{};
    inline SRWLOCK mutex = SRWLOCK_INIT;
    inline std::atomic<uint64_t> activeGeneration{};
    inline std::atomic<DWORD> ownerThread{};
    // The remaining shared fields are accessed only under mutex.
    inline uint64_t generationCounter{}, nextScope{}, dropped{}, inFlight{};
    inline uint32_t count{};
    inline thread_local uint64_t threadGeneration{}, currentScope{};

    class Lock final
    {
      public:
        Lock() noexcept { ::AcquireSRWLockExclusive(&mutex); }
        ~Lock() noexcept { ::ReleaseSRWLockExclusive(&mutex); }
        Lock(Lock const &) = delete;
        Lock &operator=(Lock const &) = delete;
    };
    inline uint64_t Sum(uint64_t left, uint64_t right) noexcept
    {
        constexpr auto maximum = (std::numeric_limits<uint64_t>::max)();
        return right > maximum - left ? maximum : left + right;
    }
    inline void Drop() noexcept { dropped = Sum(dropped, 1); }
    inline bool Tick(uint64_t &value) noexcept
    {
        LARGE_INTEGER time{};
        if (!::QueryPerformanceCounter(&time) || time.QuadPart < 0) return false;
        value = static_cast<uint64_t>(time.QuadPart);
        return true;
    }

    // The caller binds an inactive trace to its current UI thread. The editor
    // does not query CoreWindow or retain a control merely to prove UI affinity.
    inline uint32_t Start(uint32_t abi, uint64_t *generation, uint64_t *frequency) noexcept
    {
        if (!generation || !frequency) return ERROR_INVALID_PARAMETER;
        *generation = *frequency = 0;
        if (abi != AbiVersion) return ERROR_REVISION_MISMATCH;
        const auto caller = ::GetCurrentThreadId();
        Lock guard;
        if (activeGeneration.load(std::memory_order_relaxed))
            return ownerThread.load(std::memory_order_relaxed) == caller ? ERROR_BUSY : ERROR_ACCESS_DENIED;
        if (generationCounter == (std::numeric_limits<uint64_t>::max)()) return ERROR_ARITHMETIC_OVERFLOW;
        LARGE_INTEGER rate{};
        if (!::QueryPerformanceFrequency(&rate) || rate.QuadPart <= 0) return ERROR_GEN_FAILURE;
        const auto next = ++generationCounter;
        count = 0;
        dropped = inFlight = nextScope = 0;
        threadGeneration = next;
        currentScope = 0;
        ownerThread.store(caller, std::memory_order_release);
        activeGeneration.store(next, std::memory_order_release);
        *generation = next;
        *frequency = static_cast<uint64_t>(rate.QuadPart);
        return ERROR_SUCCESS;
    }

    // An insufficient buffer/query does not end or reset the active trace.
    // Successful Stop counts unfinished scopes as dropped, then disables the
    // generation. Their later destructors cannot append to a subsequent trace.
    inline uint32_t Stop(uint32_t abi, uint64_t generation, uint64_t *words, uint32_t capacityWords,
                         uint32_t *writtenWords, uint64_t *droppedScopes) noexcept
    {
        if (!writtenWords || !droppedScopes || (!words && capacityWords)) return ERROR_INVALID_PARAMETER;
        *writtenWords = 0;
        *droppedScopes = 0;
        if (abi != AbiVersion) return ERROR_REVISION_MISMATCH;
        const auto caller = ::GetCurrentThreadId();
        Lock guard;
        if (ownerThread.load(std::memory_order_relaxed) && ownerThread.load(std::memory_order_relaxed) != caller)
            return ERROR_ACCESS_DENIED;
        if (!generation || activeGeneration.load(std::memory_order_relaxed) != generation) return ERROR_INVALID_STATE;
        const auto required = count * WordsPerRecord;
        *writtenWords = required;
        *droppedScopes = Sum(dropped, inFlight);
        if (!words || capacityWords < required) return ERROR_INSUFFICIENT_BUFFER;
        activeGeneration.store(0, std::memory_order_release);
        threadGeneration = currentScope = 0;
        for (uint32_t index = 0; index < count; ++index)
            for (uint32_t word = 0; word < WordsPerRecord; ++word)
                words[index * WordsPerRecord + word] = records[index][word];
        count = 0;
        dropped = inFlight = 0;
        return ERROR_SUCCESS;
    }

    class Scope final
    {
        uint64_t _generation{}, _id{}, _parent{}, _begin{};
        DWORD _thread{};
        Phase _phase;
      public:
        explicit Scope(Phase phase) noexcept : _phase(phase)
        {
            const auto generation = activeGeneration.load(std::memory_order_acquire);
            if (!generation) return;
            const auto caller = ::GetCurrentThreadId();
            if (ownerThread.load(std::memory_order_acquire) != caller) return;
            Lock guard;
            if (activeGeneration.load(std::memory_order_relaxed) != generation ||
                ownerThread.load(std::memory_order_relaxed) != caller) return;
            if (nextScope == (std::numeric_limits<uint64_t>::max)() || !Tick(_begin)) { Drop(); return; }
            _generation = generation;
            _thread = caller;
            _id = ++nextScope;
            if (threadGeneration != generation) { threadGeneration = generation; currentScope = 0; }
            _parent = std::exchange(currentScope, _id);
            ++inFlight;
        }
        ~Scope() noexcept
        {
            if (!_generation) return;
            uint64_t end{};
            const bool clock = Tick(end);
            const bool stack = threadGeneration == _generation && currentScope == _id;
            if (stack) currentScope = _parent;
            Lock guard;
            if (activeGeneration.load(std::memory_order_relaxed) != _generation) return;
            if (inFlight) --inFlight;
            else { Drop(); return; }
            if (::GetCurrentThreadId() != _thread || !stack || !clock || end < _begin || count >= Capacity)
            {
                Drop();
                return;
            }
            records[count++] = {static_cast<uint64_t>(_phase), _id, _parent, _thread, _begin, end};
        }
        Scope(Scope const &) = delete;
        Scope &operator=(Scope const &) = delete;
    };
}
#endif
