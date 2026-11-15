// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Diagnostics/NativeStartupTrace.h"
#include "Rendering/NativeDocumentRenderer.h"
#include "Rendering/NativeTextBitmapCache.h"
#include <limits>

namespace Notepads::Markdown::Native::StartupTrace
{
    // Numeric UI draw diagnostics only: no text, retained frame, or per-cell trace.
    // QPC intervals are wall time, not CPU samples. Emit one slowest block per paint.
    class DrawDetails final
    {
      public:
        enum class OrdinaryPart : size_t
        {
            Setup,
            DirectDraw,
            CacheTry
        };

      private:
        static constexpr uint64_t MaximumJsonInteger = 9007199254740991ull;
        enum ContentFlag : uint32_t
        {
            ScriptDrawingPresent = 1u << 0,
            ImagesPresent = 1u << 1,
            MathPresent = 1u << 2,
            DiagramsPresent = 1u << 3,
            LinksPresent = 1u << 4,
            InlineCodePresent = 1u << 5,
            AuthoredHighlightsPresent = 1u << 6,
            MetadataOverflow = 1u << 30,
            PretextInvalid = 1u << 31
        };
        struct PretextMetrics
        {
            uint64_t ticks{}, utf16Units{}, codeLines{}, highlightSpans{};
            uint32_t flags{};
        };
        struct OrdinaryMetrics
        {
            std::array<uint64_t, 3> ticks{}, calls{};
            bool valid{true};
        };
        struct DirectMetrics
        {
            uint64_t cpu100ns{}, cpuCalls{}, callsProvenNoCountedPrewarm{};
        };
        struct CacheMetrics
        {
            std::array<uint64_t, 4> ticks{}, calls{};
            bool valid{true};
        };
        NativeTextBitmapCache::DiagnosticPrewarmTarget _prewarmTarget;
        PretextMetrics _slowPretext;
        OrdinaryMetrics _slowOrdinary;
        DirectMetrics _slowDirect;
        CacheMetrics _slowCache;
        NativeTextBitmapCache::DiagnosticBitmapReplay _slowBitmapReplay;
        uint64_t _trace{}, _prepareStart{}, _prepareTicks{}, _slowTicks{};
        PreviewBlockVisual const *_slow{};
        NativeTextBitmapCache *_cache{};
        NativeTextBitmapCache::Statistics _before{};

        static uint64_t Tick() noexcept
        {
            LARGE_INTEGER value{};
            return ::QueryPerformanceCounter(&value) ? static_cast<uint64_t>(value.QuadPart) : 0;
        }

      public:
        explicit DrawDetails(NativeTextBitmapCache *cache) noexcept : _cache(cache)
        {
            if (enabled.load(std::memory_order_acquire) &&
                uiThread.load(std::memory_order_acquire) == ::GetCurrentThreadId())
                _trace = Capture();
            if (_trace)
            {
                if (_cache) _before = _cache->Stats();
                _prepareStart = Tick();
            }
        }
        void Prepared() noexcept
        {
            if (_trace)
            {
                const auto end = Tick();
                if (_prepareStart && end >= _prepareStart) _prepareTicks = end - _prepareStart;
            }
        }
        class Block final
        {
            DrawDetails &_owner;
            PreviewBlockVisual const &_visual;
            uint64_t _start{}, _lastBoundary{};
            uint32_t _activeParts{};
            OrdinaryMetrics _ordinary;
            DirectMetrics _direct;
            PretextMetrics _pretext;
            uint64_t _pretextBoundary{};
            bool _pretextObserved{};
            void SetMetadataCount(uint64_t value, uint64_t &target) noexcept
            {
                if (value > MaximumJsonInteger) _pretext.flags |= MetadataOverflow;
                else target = value;
            }
            NativeTextBitmapCache::Statistics _cacheBefore{};
            NativeTextBitmapCache::DiagnosticBranchScope _cacheScope;
          public:
            // Cached metadata only, during the existing decoration traversal.
            void ObserveText(PreviewTextVisual const &text) noexcept
            {
                if (!_owner._trace) return;
                if (!(_pretext.flags & MetadataOverflow))
                {
                    if (text.utf16Length > MaximumJsonInteger - _pretext.utf16Units)
                        _pretext.flags |= MetadataOverflow;
                    else
                        _pretext.utf16Units += text.utf16Length;
                }
                if (text.scriptDrawing) _pretext.flags |= ScriptDrawingPresent;
                if (!text.images.empty()) _pretext.flags |= ImagesPresent;
                if (!text.math.empty()) _pretext.flags |= MathPresent;
                if (!text.diagrams.empty()) _pretext.flags |= DiagramsPresent;
                if (!text.links.empty()) _pretext.flags |= LinksPresent;
                if (!text.inlineCode.empty()) _pretext.flags |= InlineCodePresent;
                if (!text.highlights.empty()) _pretext.flags |= AuthoredHighlightsPresent;
            }
            void PretextDone(size_t validatedHighlightSpans) noexcept
            {
                if (!_owner._trace) return;
                if (_pretextObserved)
                {
                    _pretext.flags |= PretextInvalid;
                    return; // A duplicate never takes a second boundary QPC.
                }
                _pretextObserved = true;
                _pretextBoundary = Tick(); // The only additional QPC per block.
                if (!_pretextBoundary || !_start || _pretextBoundary < _start)
                    _pretext.flags |= PretextInvalid;
                SetMetadataCount(validatedHighlightSpans, _pretext.highlightSpans);
            }
            // Stack-only partitions. Stop before the next category without
            // changing the lexical lifetime of drawing-effect/underline guards.
            class Part final
            {
                Block *_block{};
                OrdinaryPart _part;
                uint64_t _start{};
                uint64_t _prewarmSequence{}, _cpuKernel{}, _cpuUser{};
                DWORD _cpuThread{};
                bool _quietPrewarm{}, _cpuBeginValid{};
                static bool ReadCpu(uint64_t &kernel, uint64_t &user) noexcept
                {
                    FILETIME creation{}, exit{}, kernelTime{}, userTime{};
                    if (!::GetThreadTimes(::GetCurrentThread(), &creation, &exit, &kernelTime, &userTime))
                        return false;
                    kernel = (static_cast<uint64_t>(kernelTime.dwHighDateTime) << 32) | kernelTime.dwLowDateTime;
                    user = (static_cast<uint64_t>(userTime.dwHighDateTime) << 32) | userTime.dwLowDateTime;
                    return true;
                }
              public:
                Part(Block &block, OrdinaryPart part) noexcept : _part(part)
                {
                    if (!block._owner._trace) return;
                    _block = &block;
                    ++block._ordinary.calls[static_cast<size_t>(part)];
                    if (part == OrdinaryPart::DirectDraw)
                    {
                        _prewarmSequence = NativeTextBitmapCache::DiagnosticPrewarmSequence();
                        const auto active = NativeTextBitmapCache::DiagnosticPrewarmWorkers();
                        const auto sequence = NativeTextBitmapCache::DiagnosticPrewarmSequence();
                        _quietPrewarm = !active && sequence == _prewarmSequence &&
                            sequence != (std::numeric_limits<uint64_t>::max)();
                        _cpuThread = ::GetCurrentThreadId();
                        // CodeBlock only. Other kinds retain wall/prewarm evidence
                        // with no accepted CPU pair (cpuCalls == 0 means missing).
                        if (block._visual.block.kind == 3)
                            _cpuBeginValid = ReadCpu(_cpuKernel, _cpuUser);
                    }
                    _start = Tick();
                    if (!_start || _start < block._lastBoundary || block._activeParts)
                        block._ordinary.valid = false;
                    if (_start > block._lastBoundary) block._lastBoundary = _start;
                    ++block._activeParts;
                }
                void Stop() noexcept
                {
                    auto block = std::exchange(_block, nullptr);
                    if (!block) return;
                    const auto end = Tick();
                    bool provenNoCountedPrewarm{};
                    if (_part == OrdinaryPart::DirectDraw)
                    {
                        uint64_t kernel{}, user{};
                        const auto sameThread = _cpuThread == ::GetCurrentThreadId();
                        const auto cpuEndValid = block->_visual.block.kind == 3 && ReadCpu(kernel, user);
                        const auto sequence = NativeTextBitmapCache::DiagnosticPrewarmSequence();
                        provenNoCountedPrewarm = sameThread && _quietPrewarm && sequence == _prewarmSequence;
                        // FILETIME uses 100 ns units, not a promise of resolution.
                        // This query window encloses the wall interval. Do not
                        // clamp CPU to wall time or subtract it to infer waiting.
                        if (sameThread && _cpuBeginValid && cpuEndValid && kernel >= _cpuKernel && user >= _cpuUser)
                        {
                            const auto kernelDelta = kernel - _cpuKernel, userDelta = user - _cpuUser;
                            auto &metrics = block->_direct;
                            if (kernelDelta <= MaximumJsonInteger && userDelta <= MaximumJsonInteger - kernelDelta)
                            {
                                const auto delta = kernelDelta + userDelta;
                                if (delta <= MaximumJsonInteger - metrics.cpu100ns && metrics.cpuCalls < MaximumJsonInteger)
                                {
                                    metrics.cpu100ns += delta;
                                    ++metrics.cpuCalls; // Only an accepted complete pair.
                                }
                            }
                        }
                    }
                    if (!block->_activeParts || !end || end < block->_lastBoundary)
                        block->_ordinary.valid = false;
                    if (block->_activeParts) --block->_activeParts;
                    if (end > block->_lastBoundary) block->_lastBoundary = end;
                    if (!_start || !end || end < _start)
                    {
                        block->_ordinary.valid = false;
                        return;
                    }
                    auto &total = block->_ordinary.ticks[static_cast<size_t>(_part)];
                    const auto elapsed = end - _start;
                    if (elapsed > (std::numeric_limits<uint64_t>::max)() - total)
                    {
                        block->_ordinary.valid = false;
                        return;
                    }
                    total += elapsed;
                    // Only excludes counted Run Build/readback. Queued captures,
                    // repaint callbacks and later COM teardown are outside it.
                    if (provenNoCountedPrewarm && block->_ordinary.valid &&
                        block->_direct.callsProvenNoCountedPrewarm < MaximumJsonInteger)
                        ++block->_direct.callsProvenNoCountedPrewarm;
                }
                ~Part() noexcept { Stop(); }
                Part(Part const &) = delete;
                Part &operator=(Part const &) = delete;
            };
            template <typename Action> bool MeasureOrdinaryCache(Action &&action) noexcept
            {
                Part part{*this, OrdinaryPart::CacheTry};
                return std::forward<Action>(action)();
            }
            Block(DrawDetails &owner, PreviewBlockVisual const &visual) noexcept : _owner(owner), _visual(visual),
                _cacheScope(owner._trace ? owner._cache : nullptr, &owner._prewarmTarget)
            {
                if (_owner._trace)
                {
                    if (_owner._cache) _cacheBefore = _owner._cache->Stats();
                    _start = _lastBoundary = Tick();
                    _cacheScope.Begin(_start);
                    SetMetadataCount(_visual.codeLines.size(), _pretext.codeLines);
                }
            }
            ~Block() noexcept
            {
                const auto end = _start ? Tick() : uint64_t{};
                _cacheScope.Stop(end);
                // Independent of the slowest-block winner, outside the store mutex.
                auto target = _cacheScope.PrewarmTarget();
                if (target.observations)
                {
                    const auto after = _owner._cache ? _owner._cache->Stats() : NativeTextBitmapCache::Statistics{};
                    if (!_cacheScope.Valid() || _activeParts || !end || end < _lastBoundary ||
                        after.branchOverflow || after.branchEpoch != _cacheBefore.branchEpoch)
                        target.flags |= NativeTextBitmapCache::DiagnosticPrewarmTarget::ScopeInvalid;
                    if (!_owner._prewarmTarget.observations) _owner._prewarmTarget = target;
                    else _owner._prewarmTarget.AddCounts(target);
                }
                if (!_start) return;
                if (_activeParts || !end || end < _lastBoundary) _ordinary.valid = false;
                if (end >= _start && (!_owner._slow || end - _start > _owner._slowTicks))
                {
                    _owner._slowTicks = end - _start;
                    _owner._slow = &_visual;
                    if (!_pretextObserved || _pretextBoundary > end ||
                        _pretextBoundary < _start || _pretextBoundary - _start > MaximumJsonInteger)
                        _pretext.flags |= PretextInvalid;
                    if (!(_pretext.flags & PretextInvalid))
                        _pretext.ticks = _pretextBoundary - _start;
                    if (_pretext.flags & MetadataOverflow)
                        _pretext.utf16Units = _pretext.codeLines = _pretext.highlightSpans = 0;
                    _owner._slowOrdinary = _ordinary;
                    _owner._slowDirect = _direct;
                    _owner._slowCache = {};
                    const auto after = _owner._cache ? _owner._cache->Stats() : NativeTextBitmapCache::Statistics{};
                    _owner._slowCache.valid = _cacheScope.Valid() && !after.branchOverflow &&
                        after.branchEpoch == _cacheBefore.branchEpoch;
                    uint64_t cachePartitioned{};
                    for (size_t i = 0; i < _owner._slowCache.ticks.size(); ++i)
                    {
                        if (after.branchTicks[i] < _cacheBefore.branchTicks[i] ||
                            after.branchCalls[i] < _cacheBefore.branchCalls[i])
                        {
                            _owner._slowCache.valid = false;
                            continue;
                        }
                        const auto ticks = after.branchTicks[i] - _cacheBefore.branchTicks[i];
                        _owner._slowCache.ticks[i] = ticks;
                        _owner._slowCache.calls[i] = after.branchCalls[i] - _cacheBefore.branchCalls[i];
                        if (ticks > _owner._slowTicks - cachePartitioned) _owner._slowCache.valid = false;
                        else cachePartitioned += ticks;
                    }
                    _owner._slowBitmapReplay = _cacheScope.BitmapReplay();
                    if (!_owner._slowCache.valid || _owner._slowBitmapReplay.calls > _owner._slowCache.calls[3])
                        _owner._slowBitmapReplay.flags |= NativeTextBitmapCache::DiagnosticBitmapReplay::ScopeInvalid;
                    uint64_t partitioned{};
                    for (auto ticks : _ordinary.ticks)
                    {
                        if (ticks > _owner._slowTicks - partitioned)
                        {
                            _owner._slowOrdinary.valid = false;
                            break;
                        }
                        partitioned += ticks;
                    }
                    // A separate boundary can reveal a QPC backstep or a
                    // misplaced observation even when each timer family is valid.
                    // Preserve those families; only this prefix becomes invalid.
                    if (!(_pretext.flags & PretextInvalid) &&
                        ((_owner._slowOrdinary.valid && _pretext.ticks > _owner._slowTicks - partitioned) ||
                         (_owner._slowCache.valid && _pretext.ticks > _owner._slowTicks - cachePartitioned)))
                    {
                        _pretext.flags |= PretextInvalid;
                        _pretext.ticks = 0;
                    }
                    _owner._slowPretext = _pretext;
                }
            }
            Block(Block const &) = delete;
            Block &operator=(Block const &) = delete;
        };
        void Report() noexcept
        {
            if (!_trace) return;
            try
            {
                if (_slow)
                {
                    MarkFor(_trace, Phase::DrawSlowBlockNode, _slow->block.node);
                    MarkFor(_trace, Phase::DrawSlowBlockKind, _slow->block.kind);
                    MarkFor(_trace, Phase::DrawSlowBlockRows, _slow->rowHeights.size());
                    MarkFor(_trace, Phase::DrawSlowBlockTexts, _slow->texts.size());
                    MarkFor(_trace, Phase::DrawSlowBlockTicks, _slowTicks);
                    // These are aggregates of this same winning block, not the
                    // paint-wide cache statistics below. Residual wall time is
                    // intentionally left unassigned; never clamp it to zero.
                    MarkFor(_trace, Phase::DrawSlowBlockOrdinarySetupTicks, _slowOrdinary.ticks[0]);
                    MarkFor(_trace, Phase::DrawSlowBlockOrdinaryDirectTicks, _slowOrdinary.ticks[1]);
                    MarkFor(_trace, Phase::DrawSlowBlockOrdinaryCacheTicks, _slowOrdinary.ticks[2]);
                    MarkFor(_trace, Phase::DrawSlowBlockOrdinarySetupCalls, _slowOrdinary.calls[0]);
                    MarkFor(_trace, Phase::DrawSlowBlockOrdinaryDirectCalls, _slowOrdinary.calls[1]);
                    MarkFor(_trace, Phase::DrawSlowBlockOrdinaryCacheCalls, _slowOrdinary.calls[2]);
                    MarkFor(_trace, Phase::DrawSlowBlockOrdinaryValid, _slowOrdinary.valid ? 1 : 0);
                    // These deltas belong to the same winning block. Lookup,
                    // admission, slot/eviction and command installation remain
                    // unassigned; worker-side Build is never timed here.
                    MarkFor(_trace, Phase::DrawSlowBlockCacheTakeTicks, _slowCache.ticks[0]);
                    MarkFor(_trace, Phase::DrawSlowBlockCacheBuildTicks, _slowCache.ticks[1]);
                    MarkFor(_trace, Phase::DrawSlowBlockCacheUploadTicks, _slowCache.ticks[2]);
                    MarkFor(_trace, Phase::DrawSlowBlockCacheReplayTicks, _slowCache.ticks[3]);
                    MarkFor(_trace, Phase::DrawSlowBlockCacheTakeCalls, _slowCache.calls[0]);
                    MarkFor(_trace, Phase::DrawSlowBlockCacheBuildCalls, _slowCache.calls[1]);
                    MarkFor(_trace, Phase::DrawSlowBlockCacheUploadCalls, _slowCache.calls[2]);
                    MarkFor(_trace, Phase::DrawSlowBlockCacheReplayCalls, _slowCache.calls[3]);
                    MarkFor(_trace, Phase::DrawSlowBlockCacheValid, _slowCache.valid ? 1 : 0);
                    // Fixed-size once-per-Paint packet, including small individual calls.
                    // Absence means unobserved, not zero; no frequency query in this probe.
                    if (_slow->block.kind == 6 && _slowCache.calls[3] >= 16)
                    {
                        const auto &replay = _slowBitmapReplay;
                        MarkFor(_trace, Phase::DrawTableBitmapReplayCalls, replay.calls);
                        MarkFor(_trace, Phase::DrawTableBitmapReplayAccepted, replay.accepted);
                        MarkFor(_trace, Phase::DrawTableBitmapReplayFlags, replay.flags);
                        if (replay.accepted)
                        {
                            MarkFor(_trace, Phase::DrawTableBitmapReplayMaxTicks, replay.totalTicks);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayPrefixTicks, replay.prefixTicks);
                            MarkFor(_trace, Phase::DrawTableBitmapReplaySuffixTicks, replay.suffixTicks);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayTextIndex, replay.textIndex);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayLogicalStart, replay.logicalStart);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayLogicalEnd, replay.logicalEnd);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayOrdinal, replay.ordinal);
                            MarkFor(_trace, Phase::DrawTableBitmapReplaySource, replay.source);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayStartQpc, replay.startQpc);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayMidpointQpc, replay.midpointQpc);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayEndQpc, replay.endQpc);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayApiEndQpc, replay.apiEndQpc);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayDrawTicks, replay.drawTicks);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayTailTicks, replay.tailTicks);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayCpu100ns, replay.cpu100ns);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayCpuAccepted, replay.cpuAccepted);
                            MarkFor(_trace, Phase::DrawTableBitmapReplayProvenNoCountedPrewarm, replay.provenNoCountedPrewarm);
                        }
                    }
                    // Pretext is a separate prefix, not an additional nested Part.
                    // Flags distinguish invalid/missing timing from valid zero.
                    MarkFor(_trace, Phase::DrawSlowBlockPretextTicks, _slowPretext.ticks);
                    MarkFor(_trace, Phase::DrawSlowBlockUtf16Units, _slowPretext.utf16Units);
                    MarkFor(_trace, Phase::DrawSlowBlockCodeLines, _slowPretext.codeLines);
                    MarkFor(_trace, Phase::DrawSlowBlockHighlightSpans, _slowPretext.highlightSpans);
                    MarkFor(_trace, Phase::DrawSlowBlockContentFlags, _slowPretext.flags);
                    MarkFor(_trace, Phase::DrawSlowBlockOrdinaryDirectCpu100ns, _slowDirect.cpu100ns);
                    MarkFor(_trace, Phase::DrawSlowBlockOrdinaryDirectCpuCalls, _slowDirect.cpuCalls);
                    MarkFor(_trace, Phase::DrawSlowBlockOrdinaryDirectCallsProvenNoCountedPrewarm,
                        _slowDirect.callsProvenNoCountedPrewarm);
                }
                if (_prewarmTarget.observations)
                {
                    const auto jsonValue = [&](uint64_t value) noexcept
                    {
                        if (value <= MaximumJsonInteger) return value;
                        _prewarmTarget.flags |= NativeTextBitmapCache::DiagnosticPrewarmTarget::MetadataOverflow;
                        return uint64_t{};
                    };
                    (void)jsonValue(_prewarmTarget.layoutGeneration);
                    (void)jsonValue(_prewarmTarget.revision);
                    (void)jsonValue(_prewarmTarget.generation);
                    (void)jsonValue(_prewarmTarget.start);
                    (void)jsonValue(_prewarmTarget.end);
                    (void)jsonValue(_prewarmTarget.storeEpoch);
                    (void)jsonValue(_prewarmTarget.geometryVersion);
                    MarkFor(_trace, Phase::DrawPrewarmTargetObservations, _prewarmTarget.observations);
                    MarkFor(_trace, Phase::DrawPrewarmTargetOmitted, _prewarmTarget.observations - 1u);
                    MarkFor(_trace, Phase::DrawPrewarmTargetDuplicates, _prewarmTarget.duplicates);
                    MarkFor(_trace, Phase::DrawPrewarmTargetFlags, _prewarmTarget.flags);
                    MarkFor(_trace, Phase::DrawPrewarmTargetNode, _prewarmTarget.node);
                    MarkFor(_trace, Phase::DrawPrewarmTargetTextIndex, _prewarmTarget.textIndex);
                    MarkFor(_trace, Phase::DrawPrewarmTargetLayoutGeneration, jsonValue(_prewarmTarget.layoutGeneration));
                    MarkFor(_trace, Phase::DrawPrewarmTargetModelRevision, jsonValue(_prewarmTarget.revision));
                    MarkFor(_trace, Phase::DrawPrewarmTargetModelGeneration, jsonValue(_prewarmTarget.generation));
                    MarkFor(_trace, Phase::DrawPrewarmTargetLogicalStart, jsonValue(_prewarmTarget.start));
                    MarkFor(_trace, Phase::DrawPrewarmTargetLogicalEnd, jsonValue(_prewarmTarget.end));
                    MarkFor(_trace, Phase::DrawPrewarmTargetStoreFlags, _prewarmTarget.storeFlags);
                    MarkFor(_trace, Phase::DrawPrewarmTargetOccupancy, _prewarmTarget.occupancy);
                    MarkFor(_trace, Phase::DrawPrewarmTargetMatchedSlots, _prewarmTarget.matchedSlots);
                    MarkFor(_trace, Phase::DrawPrewarmTargetItemFlags, _prewarmTarget.itemFlags);
                    MarkFor(_trace, Phase::DrawPrewarmTargetItemKeyComparison, _prewarmTarget.itemKeyComparison);
                    MarkFor(_trace, Phase::DrawPrewarmTargetReadyKeyComparison, _prewarmTarget.readyKeyComparison);
                    MarkFor(_trace, Phase::DrawPrewarmTargetStoreEpoch, jsonValue(_prewarmTarget.storeEpoch));
                    MarkFor(_trace, Phase::DrawPrewarmTargetGeometryVersion, jsonValue(_prewarmTarget.geometryVersion));
                }
                MarkFor(_trace, Phase::DrawCachePrepareTicks, _prepareTicks);
                const auto after = _cache ? _cache->Stats() : NativeTextBitmapCache::Statistics{};
                MarkFor(_trace, Phase::DrawCacheHits, after.hits - _before.hits);
                MarkFor(_trace, Phase::DrawCacheBuilds, after.builds - _before.builds);
                MarkFor(_trace, Phase::DrawCacheUploads, after.prewarmUploads - _before.prewarmUploads);
                MarkFor(_trace, Phase::DrawCacheFailures, after.failures - _before.failures);
            }
            catch (...) { }
        }
    };
}
