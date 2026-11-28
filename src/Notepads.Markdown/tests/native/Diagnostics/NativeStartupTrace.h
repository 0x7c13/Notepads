// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once

#include <array>
#include <atomic>
#include <mutex>
#include <optional>
#include <utility>

namespace Notepads::Markdown::Native::StartupTrace
{
    enum class Phase : uint64_t
    {
        Submit, Submitted, ParseObserved, LayoutEnqueued, WorkerStarted, ReadingMain, ReadingMainReady,
        Highlights, HighlightsReady, MarkerProbe, MarkerProbeReady, Measure, MeasureReady, PrefixFrame,
        FinalFrame, FinalFrameReady, Draw, DrawReady, Published,
        AsyncHighlights, AsyncHighlightsReady, AsyncHighlightsApplied, TextFormatsCreated, TextFormatsReused,
        DerivedOffscreenBatch, InitialViewportDeviceLoss,
        UiInstall, UiInstallEnd, UiStartLayout, UiStartLayoutEnd, UiLayoutUpdated, UiLayoutUpdatedEnd,
        UiViewChanged, UiViewChangedEnd, UiDraw, UiDrawEnd, UiImageRefresh, UiImageRefreshEnd,
        UiDerivedRefresh, UiDerivedRefreshEnd, UiScrollProgress, UiScrollProgressEnd, TraceDropped,
        UiDispatchViewport, UiDispatchViewportEnd,
        UiResourceAdmission, UiResourceAdmissionEnd,
        UiImageComplete, UiImageCompleteEnd,
        UiImageAnimation, UiImageAnimationEnd,
        UiNotifyPublished, UiNotifyPublishedEnd,
        UiRaiseViewport, UiRaiseViewportEnd,
        UiUpdateNotes, UiUpdateNotesEnd,
        UiRasterPrepare, UiRasterPrepareEnd,
        UiRasterSurface, UiRasterSurfaceEnd,
        UiRasterPaint, UiRasterPaintEnd,
        UiRasterCommit, UiRasterCommitEnd,
        UiRasterAttach, UiRasterAttachEnd,
        UiRasterRecovery, UiRasterRecoveryEnd,
        DrawSlowBlockNode,
        DrawSlowBlockKind,
        DrawSlowBlockRows,
        DrawSlowBlockTexts,
        DrawSlowBlockTicks,
        DrawCachePrepareTicks,
        DrawCacheHits,
        DrawCacheBuilds,
        DrawCacheUploads,
        DrawCacheFailures,
        UiDiagramCreate, UiDiagramCreateEnd,
        UiDiagramPaint, UiDiagramPaintEnd,
        DiagramCacheMiss,
        UiRasterSurfaceNativeRelease,
        UiRasterSurfaceNativeReleaseEnd,
        UiRasterSurfaceCreateReplace,
        UiRasterSurfaceCreateReplaceEnd,
        UiRasterSurfaceInterfaceCopy,
        UiRasterSurfaceInterfaceCopyEnd,
        UiRasterSurfaceSetDevice,
        UiRasterSurfaceSetDeviceEnd,
        UiRasterSurfaceBeginDraw,
        UiRasterSurfaceBeginDrawEnd,
        // Reserved legacy pool experiment IDs 94-95; no longer emitted.
        SurfaceSpareReused, SurfaceSpareBytes,
        DrawSlowBlockOrdinarySetupTicks,
        DrawSlowBlockOrdinaryDirectTicks,
        DrawSlowBlockOrdinaryCacheTicks,
        DrawSlowBlockOrdinarySetupCalls,
        DrawSlowBlockOrdinaryDirectCalls,
        DrawSlowBlockOrdinaryCacheCalls,
        DrawSlowBlockOrdinaryValid,
        // Reserved legacy pool reason IDs 103-106; no longer emitted.
        SurfaceSpareReasons, SurfaceSpareRequestedSize, SurfaceSpareBorrowedSize, SurfaceSpareDonorSize,
        DrawSlowBlockCacheTakeTicks,
        DrawSlowBlockCacheBuildTicks,
        DrawSlowBlockCacheUploadTicks,
        DrawSlowBlockCacheReplayTicks,
        DrawSlowBlockCacheTakeCalls,
        DrawSlowBlockCacheBuildCalls,
        DrawSlowBlockCacheUploadCalls,
        DrawSlowBlockCacheReplayCalls,
        DrawSlowBlockCacheValid,
        DrawSlowBlockPretextTicks,
        DrawSlowBlockUtf16Units,
        DrawSlowBlockCodeLines,
        DrawSlowBlockHighlightSpans,
        DrawSlowBlockContentFlags,
        DrawSlowBlockOrdinaryDirectCpu100ns,
        DrawSlowBlockOrdinaryDirectCpuCalls,
        DrawSlowBlockOrdinaryDirectCallsProvenNoCountedPrewarm,
        DrawPrewarmTargetObservations,
        DrawPrewarmTargetOmitted,
        DrawPrewarmTargetDuplicates,
        DrawPrewarmTargetFlags,
        DrawPrewarmTargetNode,
        DrawPrewarmTargetTextIndex,
        DrawPrewarmTargetLayoutGeneration,
        DrawPrewarmTargetModelRevision,
        DrawPrewarmTargetModelGeneration,
        DrawPrewarmTargetLogicalStart,
        DrawPrewarmTargetLogicalEnd,
        DrawPrewarmTargetStoreFlags,
        DrawPrewarmTargetOccupancy,
        DrawPrewarmTargetMatchedSlots,
        DrawPrewarmTargetItemFlags,
        DrawPrewarmTargetItemKeyComparison,
        DrawPrewarmTargetReadyKeyComparison,
        DrawPrewarmTargetStoreEpoch,
        DrawPrewarmTargetGeometryVersion,
        DrawTableBitmapReplayCalls,
        DrawTableBitmapReplayAccepted,
        DrawTableBitmapReplayFlags,
        DrawTableBitmapReplayMaxTicks,
        DrawTableBitmapReplayPrefixTicks,
        DrawTableBitmapReplaySuffixTicks,
        DrawTableBitmapReplayTextIndex,
        DrawTableBitmapReplayLogicalStart,
        DrawTableBitmapReplayLogicalEnd,
        DrawTableBitmapReplayOrdinal,
        DrawTableBitmapReplaySource,
        DrawTableBitmapReplayStartQpc,
        DrawTableBitmapReplayMidpointQpc,
        DrawTableBitmapReplayEndQpc,
        DrawTableBitmapReplayApiEndQpc,
        DrawTableBitmapReplayDrawTicks,
        DrawTableBitmapReplayTailTicks,
        DrawTableBitmapReplayCpu100ns,
        DrawTableBitmapReplayCpuAccepted,
        DrawTableBitmapReplayProvenNoCountedPrewarm
    };

    inline std::atomic<bool> enabled{};
    inline std::atomic<bool> synchronousHighlights{};
    inline std::atomic<bool> fullHighlightBaseline{};
    inline std::atomic<uint64_t> generation{};
    inline std::mutex mutex;
    inline constexpr size_t Capacity = 24576;
    // Reserve one additional row so overflow is always observable at Stop.
    inline std::array<uint64_t, 3 * (Capacity + 1)> samples{};
    inline size_t count{};
    inline uint64_t dropped{};
    inline std::atomic<DWORD> uiThread{};

    inline void Start(bool synchronous)
    {
        std::lock_guard guard(mutex);
        count = 0;
        dropped = 0;
        uiThread.store(::GetCurrentThreadId(), std::memory_order_release);
        generation.fetch_add(1, std::memory_order_release);
        synchronousHighlights.store(synchronous);
        enabled.store(true, std::memory_order_release);
    }

    inline uint64_t Capture() noexcept
    {
        return enabled.load(std::memory_order_acquire) ? generation.load(std::memory_order_acquire) : 0;
    }

    inline thread_local std::optional<uint64_t> workerGeneration;

    // A render request runs synchronously on its worker. Its nested renderer
    // phases belong to the trace that enqueued it, even after a new run starts.
    struct WorkerTrace
    {
        std::optional<uint64_t> previous;
        explicit WorkerTrace(uint64_t current) noexcept : previous(std::exchange(workerGeneration, current)) {}
        ~WorkerTrace() { workerGeneration = previous; }
        WorkerTrace(WorkerTrace const &) = delete;
        WorkerTrace &operator=(WorkerTrace const &) = delete;
    };

    inline void MarkFor(uint64_t sampleGeneration, Phase phase, uint64_t layout = 0)
    {
        if (!sampleGeneration || !enabled.load(std::memory_order_acquire))
            return;
        LARGE_INTEGER time{};
        ::QueryPerformanceCounter(&time);
        std::lock_guard guard(mutex);
        if (!enabled.load(std::memory_order_relaxed) ||
            generation.load(std::memory_order_relaxed) != sampleGeneration)
            return;
        if (count >= 3 * Capacity)
        {
            ++dropped;
            return;
        }
        samples[count++] = static_cast<uint64_t>(phase);
        samples[count++] = layout;
        samples[count++] = static_cast<uint64_t>(time.QuadPart);
    }

    inline void Mark(Phase phase, uint64_t layout = 0)
    {
        MarkFor(workerGeneration ? *workerGeneration : Capture(), phase, layout);
    }

    // These intervals are synchronous UI wall time, not sampled CPU time.
    // Capture both identities once: the body may start a new layout generation.
    struct UiScope
    {
        uint64_t trace{}, layout{};
        Phase end;
        UiScope(Phase begin, Phase finish, uint64_t key) noexcept : layout(key), end(finish)
        {
            try
            {
                if (enabled.load(std::memory_order_acquire) &&
                    uiThread.load(std::memory_order_acquire) == ::GetCurrentThreadId())
                {
                    trace = Capture();
                    MarkFor(trace, begin, layout);
                }
            }
            catch (...) { trace = 0; }
        }
        ~UiScope() noexcept
        {
            try { MarkFor(trace, end, layout); }
            catch (...) { }
        }
        UiScope(UiScope const &) = delete;
        UiScope &operator=(UiScope const &) = delete;
    };

    // Capture call boundaries without taking the collector mutex. Append all
    // complete pairs once afterward; the containing UI phase still includes
    // instrumentation/reporting cost. This object retains numeric stack data only.
    template <size_t MaximumCalls> class UiCallBatch final
    {
        static_assert(MaximumCalls > 0 && MaximumCalls <= Capacity / 2);
        std::array<uint64_t, 6 * MaximumCalls> _samples{};
        uint64_t _trace{}, _layout{}, _lost{};
        DWORD _thread{};
        size_t _words{};

        static uint64_t Tick() noexcept
        {
            LARGE_INTEGER time{};
            return ::QueryPerformanceCounter(&time) && time.QuadPart > 0 ? static_cast<uint64_t>(time.QuadPart) : 0;
        }
        void Complete(uint64_t trace, Phase begin, Phase end, uint64_t first, uint64_t last) noexcept
        {
            if (_trace != trace || _thread != ::GetCurrentThreadId())
                return;
            if (!first || !last || last < first || _words + 6 > _samples.size())
            {
                _lost += 2;
                return;
            }
            _samples[_words++] = static_cast<uint64_t>(begin);
            _samples[_words++] = _layout;
            _samples[_words++] = first;
            _samples[_words++] = static_cast<uint64_t>(end);
            _samples[_words++] = _layout;
            _samples[_words++] = last;
        }
        class Call final
        {
            UiCallBatch &_owner;
            Phase _begin, _end;
            uint64_t _trace{}, _first{};
          public:
            Call(UiCallBatch &owner, Phase begin, Phase end) noexcept : _owner(owner), _begin(begin), _end(end)
            {
                if (owner._trace && owner._thread == ::GetCurrentThreadId())
                {
                    _trace = owner._trace;
                    _first = Tick();
                }
            }
            ~Call() noexcept
            {
                if (_trace)
                {
                    const auto last = Tick();
                    _owner.Complete(_trace, _begin, _end, _first, last);
                }
            }
            Call(Call const &) = delete;
            Call &operator=(Call const &) = delete;
        };

      public:
        explicit UiCallBatch(uint64_t layout) noexcept : _layout(layout), _thread(::GetCurrentThreadId())
        {
            if (enabled.load(std::memory_order_acquire) && uiThread.load(std::memory_order_acquire) == _thread)
                _trace = Capture();
        }
        ~UiCallBatch() noexcept { Flush(); }
        UiCallBatch(UiCallBatch const &) = delete;
        UiCallBatch &operator=(UiCallBatch const &) = delete;

        template <typename Function> decltype(auto) Measure(Phase begin, Phase end, Function &&operation)
            noexcept(noexcept(std::forward<Function>(operation)()))
        {
            Call interval{*this, begin, end};
            return std::forward<Function>(operation)();
        }
        void Flush() noexcept
        {
            const auto trace = std::exchange(_trace, 0);
            if (!trace || _thread != ::GetCurrentThreadId())
                return;
            try
            {
                std::lock_guard guard(mutex);
                if (!enabled.load(std::memory_order_relaxed) || generation.load(std::memory_order_relaxed) != trace ||
                    uiThread.load(std::memory_order_relaxed) != _thread)
                    return;
                // Keep pairs atomic at the collector boundary. An incomplete
                // batch never creates unmatched child intervals at capacity.
                dropped += _lost;
                if (count > 3 * Capacity || _words > 3 * Capacity - count)
                    dropped += _words / 3;
                else
                    for (size_t index = 0; index < _words; ++index)
                        samples[count++] = _samples[index];
            }
            catch (...) { /* Diagnostics must not replace the operation's outcome. */ }
        }
    };

    inline winrt::com_array<uint64_t> Stop()
    {
        std::lock_guard guard(mutex);
        const bool active = enabled.exchange(false, std::memory_order_acq_rel);
        synchronousHighlights.store(false);
        if (!active)
            return {};
        LARGE_INTEGER time{};
        ::QueryPerformanceCounter(&time);
        samples[count++] = static_cast<uint64_t>(Phase::TraceDropped);
        samples[count++] = dropped;
        samples[count++] = static_cast<uint64_t>(time.QuadPart);
        const auto written = std::exchange(count, 0);
        return {samples.begin(), samples.begin() + written};
    }
}
