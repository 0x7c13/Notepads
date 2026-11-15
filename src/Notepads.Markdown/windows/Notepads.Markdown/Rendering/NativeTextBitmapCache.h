// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include <d2d1_1.h>
#include <cstdint>
#include <memory>
#include <span>
#include <functional>
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
#include <atomic>
#include <array>
#include <limits>
#endif
#include "notepads/markdown/preview.h"

namespace Notepads::Markdown::Native
{
    struct PreviewFrame;
    struct PreviewTheme;
    struct NativeTextPrewarm;
    class MainTextSnapshot;
    struct PreviewBlockVisual;
    struct PreviewTextVisual;

    // UI-thread, per-view replay of eligible static text: ordinary text keeps
    // final bitmap pixels; inline code and links keep closed drawing commands so color
    // glyphs are not flattened through transparent BGRA8. Dynamic effects bypass it.
    // Entries never retain a MainTextSnapshot, model, layout, or measured block.
    class NativeTextBitmapCache final
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        class DiagnosticBranchTimer;
#endif
      public:
        using RequestRepaint = std::function<void()>;
        struct Statistics
        {
            uint64_t hits{};
            uint64_t builds{};
            uint64_t failures{};
            uint64_t bytes{}; // Known bitmap pixel bytes only; command-list heap usage is opaque.
            uint32_t entries{};
            uint32_t commandEntries{};
            uint64_t commandTextUnits{};
            uint64_t prewarmUploads{};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            // UI DrawDetails scopes only, in Take/Build/Upload/Replay order.
            // Cumulative counters survive Clear, like hits/builds above.
            std::array<uint64_t, 4> branchTicks{}, branchCalls{};
            uint64_t branchEpoch{};
            bool branchOverflow{};
#endif
        };
        NativeTextBitmapCache() noexcept;
        ~NativeTextBitmapCache();
        NativeTextBitmapCache(NativeTextBitmapCache const &) = delete;
        NativeTextBitmapCache &operator=(NativeTextBitmapCache const &) = delete;

        // Capture the actual source target state once for this synchronous draw.
        // Only these uniform DPI values have pixel-equivalent script rasters.
        static constexpr bool SupportsScriptDpi(float dpiX, float dpiY) noexcept
        {
            return dpiX == dpiY && (dpiX == 96.0f || dpiX == 192.0f);
        }
        void Begin(ID2D1DeviceContext *context) noexcept;
        // First call with allowBuild=false, before installing effects. On a miss,
        // reset the original layout's effects, apply static link colors, then retry with true.
        // Selection/hover intersection is excluded by the caller. A nearby
        // script miss may use an exact loading-stage raster or queue optional worker preparation.
        // Any optional
        // cache failure returns false and leaves the source context untouched.
        // With both optional outputs, an exact queued/in-flight raster instead
        // registers one weak-view repaint callback and returns pending=true.
        // Completion, failure and invalidation notify outside the cache lock.
        bool TryDraw(ID2D1DeviceContext *context, PreviewFrame const &frame,
                     PreviewBlockVisual const &block, PreviewTextVisual const &text,
                     uint32_t textIndex, D2D1_POINT_2F origin,
                     ID2D1SolidColorBrush *foreground, bool allowBuild, bool promote = true,
                     RequestRepaint const *repaint = nullptr, bool *pending = nullptr,
                     ID2D1SolidColorBrush *scriptLink = nullptr, bool allowPreparation = false) noexcept;
        // The serialized layout worker supplies independent, static measurement
        // layouts. The UI starts optional work only after capturing its real device.
        static std::shared_ptr<NativeTextPrewarm> CreatePrewarm() noexcept;
        static void BeginPrewarm(std::shared_ptr<NativeTextPrewarm> const &prewarm,
                                 notepads_markdown_preview_key const &key, PreviewTheme const &theme,
                                 std::shared_ptr<MainTextSnapshot const> const &main) noexcept;
        static void CollectPrewarm(std::shared_ptr<NativeTextPrewarm> const &prewarm,
                                   PreviewBlockVisual const &block) noexcept;
        static void PositionPrewarm(std::shared_ptr<NativeTextPrewarm> const &prewarm,
                                    std::span<notepads_markdown_preview_block const> blocks) noexcept;
        void PrepareFrame(PreviewFrame const &frame, D2D1_POINT_2F drawOrigin) noexcept;
        static uint32_t PreparedCount(std::shared_ptr<NativeTextPrewarm> const &prewarm) noexcept;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        // Samples of the existing global admission budget, not per-cache state.
        // Max-value sentinels mean unknown; no non-lock-free diagnostic loads.
        static uint64_t DiagnosticPrewarmSequence() noexcept;
        static uint32_t DiagnosticPrewarmWorkers() noexcept;
        // Fixed first qualifying false/false miss. No payload ownership or timers.
        struct DiagnosticPrewarmTarget
        {
            enum Flag : uint32_t
            {
                FailedEpoch = 1u << 0, ObservationComplete = 1u << 1,
                ObservationFailed = 1u << 2, MetadataOverflow = 1u << 3,
                MultiplicityOverflow = 1u << 4, ScopeInvalid = 1u << 5,
                ReadyLookupAttempted = 1u << 6, ReadyPayloadAcquired = 1u << 7,
                BitmapUploaded = 1u << 8, ReadyReplayed = 1u << 9
            };
            std::array<uint8_t, 16> document{}; // Identity only; not serialized.
            uint64_t layoutGeneration{}, revision{}, generation{}, start{}, end{};
            uint64_t storeEpoch{}, geometryVersion{};
            uint32_t node{}, textIndex{}, profile{}, extensions{};
            uint32_t observations{}, duplicates{}, flags{}, storeFlags{}, occupancy{}, matchedSlots{};
            uint32_t itemFlags{}, itemKeyComparison{}, readyKeyComparison{};
            bool SameTarget(DiagnosticPrewarmTarget const &other) const noexcept
            {
                return document == other.document && revision == other.revision && generation == other.generation &&
                    profile == other.profile && extensions == other.extensions && layoutGeneration == other.layoutGeneration &&
                    node == other.node && textIndex == other.textIndex && start == other.start && end == other.end;
            }
            void AddCounts(DiagnosticPrewarmTarget const &other) noexcept
            {
                const auto maximum = (std::numeric_limits<uint32_t>::max)();
                if (other.observations > maximum - observations || other.duplicates > maximum - duplicates)
                {
                    flags |= MultiplicityOverflow;
                    observations = maximum;
                    return;
                }
                observations += other.observations;
                duplicates += other.duplicates;
                flags |= other.flags & MultiplicityOverflow;
            }
        };
        // One maximum call, never independently selected component maxima.
        // All values are numeric, stack-owned diagnostics. Zero is valid when accepted > 0.
        struct DiagnosticBitmapReplay
        {
            static constexpr uint64_t MaximumJsonInteger = 9007199254740991ull;
            enum Flag : uint32_t
            {
                InvalidTiming = 1u << 0, MissingMidpoint = 1u << 1,
                ScopeInvalid = 1u << 2, Overflow = 1u << 3, MetadataInvalid = 1u << 4, MissingApiEnd = 1u << 5
            };
            uint64_t calls{}, accepted{}, totalTicks{}, prefixTicks{}, suffixTicks{};
            uint64_t startQpc{}, midpointQpc{}, endQpc{}, logicalStart{}, logicalEnd{}, ordinal{};
            uint64_t apiEndQpc{}, drawTicks{}, tailTicks{}, cpu100ns{};
            uint32_t cpuAccepted{}, provenNoCountedPrewarm{};
            uint32_t textIndex{}, source{}, flags{}; // source: 1 resident, 2 newly installed (build/upload).
            void Complete(uint64_t first, uint64_t midpoint, uint64_t last, uint32_t index,
                uint64_t start, uint64_t end, uint32_t origin, bool scopeValid, uint64_t apiEnd,
                uint64_t cpu, bool cpuValid, bool noCountedPrewarm) noexcept
            {
                if (calls == MaximumJsonInteger) { flags |= Overflow; return; }
                ++calls;
                uint32_t invalid{};
                if (!scopeValid) invalid |= ScopeInvalid;
                if (!midpoint) invalid |= MissingMidpoint;
                if (!apiEnd) invalid |= MissingApiEnd;
                if (apiEnd && (apiEnd < midpoint || apiEnd > last)) invalid |= InvalidTiming;
                if (!first || !last || last < first || (midpoint && (midpoint < first || midpoint > last)))
                    invalid |= InvalidTiming;
                if (apiEnd > MaximumJsonInteger) invalid |= Overflow;
                if (first > MaximumJsonInteger || midpoint > MaximumJsonInteger || last > MaximumJsonInteger ||
                    start > MaximumJsonInteger || end > MaximumJsonInteger) invalid |= Overflow;
                if (start > end || (origin != 1 && origin != 2)) invalid |= MetadataInvalid;
                flags |= invalid;
                if (invalid) return;
                const auto elapsed = last - first;
                ++accepted; // Bounded by calls, including a valid all-zero duration.
                if (accepted != 1 && elapsed <= totalTicks) return; // Stable first tie.
                totalTicks = elapsed;
                prefixTicks = midpoint - first;
                suffixTicks = last - midpoint;
                startQpc = first;
                midpointQpc = midpoint;
                endQpc = last;
                textIndex = index;
                logicalStart = start;
                logicalEnd = end;
                ordinal = calls;
                source = origin;
                apiEndQpc = apiEnd;
                drawTicks = apiEnd - midpoint;
                tailTicks = last - apiEnd;
                // CPU query window encloses API wall; it has coarse resolution.
                // Missing CPU does not invalidate wall timing; never clamp/subtract.
                cpuAccepted = cpuValid && cpu <= MaximumJsonInteger ? 1u : 0u;
                cpu100ns = cpuAccepted ? cpu : 0;
                provenNoCountedPrewarm = noCountedPrewarm ? 1u : 0u;
            }
        };
        // A stack-only gate owned by the existing diagnostic block scope.
        // No QPC calls occur in the cache without an active gate.
        class DiagnosticBranchScope final
        {
            friend class DiagnosticBranchTimer;
            friend class NativeTextBitmapCache;
            NativeTextBitmapCache *_cache{};
            DiagnosticBranchScope *_previous{};
            uint64_t _lastBoundary{};
            uint32_t _active{};
            bool _valid{true};
            DiagnosticPrewarmTarget _prewarmTarget;
            DiagnosticBitmapReplay _bitmapReplay;
            DiagnosticPrewarmTarget const *_priorPrewarmTarget{};
            bool RecordPrewarmTarget(DiagnosticPrewarmTarget const &target) noexcept;
          public:
            explicit DiagnosticBranchScope(NativeTextBitmapCache *cache,
                DiagnosticPrewarmTarget const *priorPrewarmTarget = nullptr) noexcept;
            DiagnosticPrewarmTarget const &PrewarmTarget() const noexcept { return _prewarmTarget; }
            void Begin(uint64_t start) noexcept;
            void Stop(uint64_t end) noexcept;
            bool Valid() const noexcept { return _valid; }
            DiagnosticBitmapReplay const &BitmapReplay() const noexcept { return _bitmapReplay; }
            ~DiagnosticBranchScope() noexcept;
            DiagnosticBranchScope(DiagnosticBranchScope const &) = delete;
            DiagnosticBranchScope &operator=(DiagnosticBranchScope const &) = delete;
        };
        struct PreparationGate
        {
            std::atomic<bool> entered{};
            std::atomic<bool> released{};
            std::atomic<bool> failBeforePublish{};
        };
        // Diagnostic-only publication hold: timeout is five seconds and
        // cancellation or replacement never waits for the test to release it.
        static void SetPreparationGate(PreviewFrame const &frame, std::shared_ptr<PreparationGate> gate) noexcept;
#endif
        void Clear() noexcept;
        Statistics Stats() const noexcept;

      private:
        friend struct NativeTextPrewarm;
        struct State;
        std::unique_ptr<State> _state;
        Statistics _statistics;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        DiagnosticBranchScope *_diagnosticBranchScope{};
#endif
    };
}
