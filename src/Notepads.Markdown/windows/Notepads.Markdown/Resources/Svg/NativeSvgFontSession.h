// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <windows.h>
#include "notepads/markdown/svg_fonts.h"
#include "notepads/markdown/svg_resources.h"
#include "Resources/Svg/NativeSvgCoverageSession.h"
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS) && !defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
#error SVG runtime hooks require the isolated quality build.
#endif
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
#include <array>
#include <span>
#include <string>
#include <vector>
#endif

namespace Notepads::Markdown::Native
{
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
    // Probe-owned immutable input, frozen before the actual task starts. The
    // session copies forced records with its existing resources allocator.
    struct NativeSvgFontQualityForcedInput
    {
        std::string elementId;
        std::wstring baseFamily;
        std::array<uint8_t, 32> fileSha256{};
        uint32_t faceIndex{}, simulations{};
    };

#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
    enum class NativeSvgFontRuntimePoint : uint32_t
    {
        None,
        FragmentRead,
        SinkCommand,
        DefaultAxes,
        MirrorGeometryX,
        CorruptOwnedFont,
        SplitClusterPlan,
        VisibleOutline,
        NonFiniteAxis,
        CapacityShort,
        HeldGeometry,
        Count
    };

    struct NativeSvgFontRuntimeEvent
    {
        NativeSvgFontRuntimePoint point{};
        uint64_t ordinal{}, progress{}, capacity{};
        HRESULT hr{S_OK};
        bool injected{};
    };

    // Immutable worker input. The observer may only write probe-owned fixed
    // atomics, and its context must survive actual worker retirement.
    struct NativeSvgFontRuntimeHook
    {
        NativeSvgFontRuntimePoint point{};
        uint64_t ordinal{};
        HRESULT failureHr{E_FAIL};
        void (*observer)(void *, NativeSvgFontRuntimeEvent const &) noexcept{};
        void *context{};
    };
#endif
    struct NativeSvgFontQualityConfig
    {
        std::array<uint8_t, 32> sourceSha256{};
        std::vector<NativeSvgFontQualityForcedInput> forced;
        std::wstring fallbackFamily;
        uint32_t rangeFirst{}, rangeLast{};
        float fallbackScale{1};
        // Zero preserves the existing Native-plan-only test mode.
        uint64_t engineTraceCapacity{};
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
        NativeSvgFontRuntimeHook runtimeHook{};
#endif
    };

    struct NativeSvgFontQualityRange
    {
        uint32_t start{}, count{};
    };

    struct NativeSvgFontQualityPlan
    {
        uint64_t draftId{}, generation{};
        NativeSvgFontQualityRange text, elementId, strings, files, faces, axes, runs;
    };

    // This large POD lives in the already-paid task, never on the worker stack
    // or in its frequently copied retirement receipt. Only the worker writes;
    // the probe reads after actual worker/reaper retirement. Counts commit last.
    struct NativeSvgFontQualityTrace
    {
        std::array<NativeSvgFontQualityPlan, 64> plans{};
        std::array<MsFrFile, 256> files{};
        std::array<MsFrFace, 512> faces{};
        std::array<MsFrAxis, 8192> axes{};
        std::array<MsFrLogicalRun, 8192> runs{};
        std::array<uint8_t, 65536> bytes{};
        std::array<uint8_t, 32> actualSourceSha256{};
        uint32_t planCount{}, fileCount{}, faceCount{}, axisCount{}, runCount{}, byteCount{};
        uint32_t forcedCount{}, forcedBound{};
        HRESULT firstError{S_OK};
        bool sourceVerified{}, completeValidated{};
    };
#endif

    struct NativeSvgFontStatistics
    {
        uint64_t plans{}, fontFills{}, metricsFills{}, geometryPrepares{}, geometryFills{};
        uint64_t fragmentReads{}, fragmentReleases{}, geometryRetirements{};
        uint64_t lateCalls{}, wrongGenerations{}, invalidReleases{}, failedReleases{};
        HRESULT firstFault{S_OK};
    };

    // Stack-owned receipt survives the actual Impl and its final allocation.
    // firstWorkFault is independent of cleanupError: an unsuccessful font plan
    // can still retire every object and return a successful cleanup receipt.
    struct NativeSvgFontRetirement
    {
        uint64_t session{};
        uint32_t workerThread{}, completedStages{};
        HRESULT firstWorkFault{S_OK}, cleanupError{S_OK};
        uint64_t fragmentReads{}, fragmentReleases{}, remainingNativeBytes{}, failedReleases{};
        notepads_markdown_svg_work_token unreleasedObject{};
        bool retired{};
    };

    class NativeSvgFontSession final
    {
      public:
        struct Impl;
        NativeSvgFontSession() noexcept = default;
        ~NativeSvgFontSession();
        NativeSvgFontSession(NativeSvgFontSession const &) = delete;
        NativeSvgFontSession &operator=(NativeSvgFontSession const &) = delete;
        NativeSvgFontSession(NativeSvgFontSession &&) = delete;
        NativeSvgFontSession &operator=(NativeSvgFontSession &&) = delete;

        // The canonical task pins resources until this session and all engine
        // owners retire. Create and every font call require the same worker.
        static HRESULT Create(notepads_markdown_svg_resources *resources, NativeSvgFontSession *output) noexcept;
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
        // The caller owns source/config through this synchronous call, and the
        // paid trace through FinishOnWorker. No oracle pixels enter this API.
        static HRESULT CreateForQuality(notepads_markdown_svg_resources *resources, NativeSvgFontQualityConfig const &config,
                                        std::span<uint8_t const> actualSource, NativeSvgFontQualityTrace *trace,
                                        NativeSvgFontSession *output) noexcept;
        // Work-success gate after successful engine run and before publication;
        // never overload the cleanup-only return of FinishOnWorker.
        HRESULT ValidateQualityComplete() noexcept;
#endif
        MsSvgFontCallbacks Callbacks() const noexcept;
        MsSvgCoverageCallbacks CoverageCallbacks() const noexcept;
        SvgCoverageStatistics CoverageStatistics() const noexcept;
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
        void SetCoverageTestHooks(SvgCoverageTestHooks const &) noexcept;
        IUnknown *TakeRetainedCoverageTestOwner() noexcept;
#endif
        NativeSvgFontStatistics Statistics() const noexcept;
        // Call once after Rust finish, on the creating worker. Always attempts
        // independent actual cleanup without a cancellation/work checkpoint.
        // A failed valid Create attempt also yields its saved cleanup receipt.
        // Wrong-thread or repeated calls keep output unchanged. A valid receipt
        // reports the first work error separately and returns cleanupError.
        HRESULT FinishOnWorker(NativeSvgFontRetirement *output) noexcept;
        static uint64_t ModuleLateCalls() noexcept;

      private:
        static HRESULT CreateInternal(notepads_markdown_svg_resources *resources, NativeSvgFontSession *output
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
                                      ,
                                      NativeSvgFontQualityConfig const *config = nullptr,
                                      std::span<uint8_t const> actualSource = {},
                                      NativeSvgFontQualityTrace *trace = nullptr
#endif
                                      ) noexcept;
        Impl *_impl{};
        NativeSvgFontStatistics _finalStatistics{};
        SvgCoverageStatistics _finalCoverageStatistics{};
        NativeSvgFontRetirement _creationFailure{};
        bool _attempted{}, _creationFailed{}, _finished{};
    };
}
