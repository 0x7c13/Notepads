// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeMeasurementCacheTests.h"
#include "Rendering/NativeMeasurementCache.h"
#include "Rendering/NativeDocumentRenderer.h"
#include <bit>
#include <thread>

namespace Notepads::Markdown::Native::MeasurementCacheTests
{
    namespace
    {
        void Require(bool condition, wchar_t const *message)
        {
            if (!condition)
                throw winrt::hresult_error(E_FAIL, message);
        }
        template <typename T> T Pod()
        {
            T result{};
            result.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(T)};
            return result;
        }
        void Word(std::span<uint8_t> bytes, size_t offset, uint32_t value)
        {
            for (size_t i = 0; i < 4; ++i)
                bytes[offset + i] = static_cast<uint8_t>(value >> (i * 8));
        }
        void Compare(PreviewFrame const &fresh, PreviewFrame const &warm, Results &results)
        {
            Require(fresh.totalHeight == warm.totalHeight && fresh.Blocks().size() == warm.Blocks().size(),
                    L"cache hit preserves exact block extent");
            auto const &a = *fresh.navigation;
            auto const &b = *warm.navigation;
            Require(a.LineCount() == b.LineCount(), L"cache hit line count");
            bool sawBacktrack{}, sawPlateau{};
            double envelope = -std::numeric_limits<double>::infinity();
            for (size_t i = 0; i < a.LineCount(); ++i)
            {
                auto const &x = a.Line(i);
                auto const &y = b.Line(i);
                Require(x.text.start == y.text.start && x.text.end == y.text.end && x.block == y.block && x.x == y.x &&
                            x.localY == y.localY && x.y == y.y && x.height == y.height && x.width == y.width &&
                            x.tableCell == y.tableCell && x.followY == y.followY,
                        L"cache hit exact current line geometry and offsets");
                envelope = std::max(envelope, x.y);
                Require(std::isfinite(x.followY) && x.followY == envelope,
                        L"follow coordinate equals the independent current-layout y envelope");
                if (i)
                {
                    auto const &previous = a.Line(i - 1);
                    sawBacktrack |= x.tableCell && previous.tableCell && x.y < previous.y;
                    sawPlateau |= x.tableCell && x.y < x.followY && x.followY == previous.followY;
                }
                ++results.lines;
                for (bool forward : {false, true})
                    for (double distance : {0.0, 150.0})
                        for (float preferred : {x.x, x.x + x.width / 2, x.x + x.width})
                            Require(a.VisualLine(i, forward, distance, preferred) ==
                                        b.VisualLine(i, forward, distance, preferred),
                                    L"cache hit exact Up/Down/Page navigation including table columns");
            }
            Require(sawBacktrack && sawPlateau,
                    L"real long table crosses cell y backwards while viewport follow has an explicit plateau");
            double previousFollow = -std::numeric_limits<double>::infinity();
            for (uint32_t i = 0; i <= fresh.main->Length(); ++i)
            {
                Require(a.Snap(i, false) == b.Snap(i, false) && a.Snap(i, true) == b.Snap(i, true),
                        L"cache hit exact caret/cluster boundary oracle");
                auto caret = a.Snap(i, false);
                Require(a.LineAt(caret) == b.LineAt(caret), L"cache hit exact current caret line");
                auto follow = a.FollowY(i);
                Require(follow == b.FollowY(i) && follow >= previousFollow &&
                            a.RevealY(i) == a.Line(a.LineAt(i)).y,
                        L"follow is monotone through repeated starts/EOF while old RevealY stays exact");
                previousFollow = follow;
            }
            for (size_t i = 0; i < fresh.Blocks().size(); ++i)
            {
                auto const &x = fresh.Blocks()[i];
                auto const &y = warm.Blocks()[i];
                Require(x.block.y == y.block.y && x.measuredHeight == y.measuredHeight &&
                            x.texts.size() == y.texts.size(),
                        L"cache hit table and paragraph heights");
                for (size_t j = 0; j < x.texts.size(); ++j)
                {
                    auto const &tx = x.texts[j];
                    auto const &ty = y.texts[j];
                    Require(tx.layout && ty.layout && tx.layout.get() != ty.layout.get() && !tx.metricToken &&
                                !ty.metricToken && tx.x == ty.x && tx.y == ty.y && tx.width == ty.width &&
                                tx.height == ty.height && tx.logicalStart == ty.logicalStart &&
                                tx.logicalEnd == ty.logicalEnd,
                            L"every Frame owns fresh layout with exact cached measurement geometry");
                    UINT32 count{};
                    auto status = ty.layout->GetClusterMetrics(nullptr, 0, &count);
                    if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                        winrt::check_hresult(status);
                    std::vector<DWRITE_CLUSTER_METRICS> clusters(count);
                    if (count)
                        winrt::check_hresult(ty.layout->GetClusterMetrics(clusters.data(), count, &count));
                    auto position = static_cast<uint32_t>(ty.logicalStart);
                    for (auto const &cluster : clusters)
                    {
                        position += cluster.length;
                        Require(b.Snap(position, false) == position && b.Snap(position, true) == position,
                                L"cached cluster endpoints equal independently fresh DWrite clusters");
                        ++results.clusters;
                    }
                    Require(position == ty.logicalEnd, L"all fresh DWrite clusters cover the current text");
                }
            }
        }
    }

    Results Run(std::shared_ptr<ReadingMemory> const &memory)
    {
        Results results;
        auto cache = NativeMeasurementCache::Create();
        Require(static_cast<bool>(cache), L"native metric cache enabled for oracle");
        auto summary = Pod<notepads_markdown_core_model_summary_result>();
        auto error = Pod<notepads_markdown_core_error>();
        Require(notepads_markdown_core_model_summary(memory->ModelOwner().Get(), &summary, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                L"metric fixture model summary");
        notepads_markdown_preview_key key{};
        key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
        key.model = summary.identity;
        key.layout_generation = key.theme_generation = key.measurement_generation = 1;
        key.width = 215;
        key.indent = 24;
        key.gap = 12;
        std::atomic<uint64_t> latest{1};
        auto make = [&](std::shared_ptr<NativeMeasurementCache> const &selected, double width, float fontSize)
        {
            auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                                                                    memory, cancel, 23, NextReadingTextToken());
            auto actual = key;
            actual.width = width;
            PreviewTheme theme;
            theme.fontSize = fontSize;
            auto renderer = NativeDocumentRenderer::Create({}, cancel, actual, theme, reading, {}, {}, {}, selected);
            auto frame = renderer->CreateFrame(0, 4000, 1, latest);
            Require(frame && frame->main->Text() == FixtureMain, L"metric oracle exact authored fixture Main");
            return frame;
        };
        auto fresh = make({}, 215, 18);
        auto cold = make(cache, 215, 18);
        auto before = cache->Stats();
        auto warm = make(cache, 215, 18);
        auto after = cache->Stats();
        Require(after.hits > before.hits && after.puts > 0, L"second full measurement really used cached metrics");
        Compare(*fresh, *cold, results);
        Compare(*fresh, *warm, results);
        results.geometryOracle = results.freshFrames = true;
        auto const &links = warm->descriptors->links;
        Require(links.size() == 2 && links[0].destination == L"first" && links[1].destination == L"second" &&
                    links[0].text.start != links[1].text.start &&
                    links[0].text.end - links[0].text.start == links[1].text.end - links[1].text.start,
                L"same geometry rebinds both current offsets and distinct link destinations");
        results.currentLinks = true;
        auto widthFresh = make({}, 187, 18);
        auto widthCache = make(cache, 187, 18);
        Require(cache->Stats().misses > after.misses, L"changed effective width misses");
        Compare(*widthFresh, *widthCache, results);
        before = cache->Stats();
        auto fontFresh = make({}, 187, 21);
        auto fontCache = make(cache, 187, 21);
        Require(cache->Stats().misses > before.misses, L"changed actual font size misses");
        Compare(*fontFresh, *fontCache, results);
        results.configurationMiss = true;

        // Inspect the actual renderer-owned line vector, not a second test
        // allocation. Capacity (including the new field) uses the original root.
        auto payload = const_cast<ReadingLine *>(&fresh->navigation->Line(0));
        auto allocation = ReadingMemory::InspectAllocation(payload);
        auto prefixBytes = ReadingMemory::AllocationRequestBytes(0);
        Require(fresh->navigation->Memory() == memory && allocation.lease &&
                    allocation.requestedTotal >= prefixBytes,
                L"current navigation lines carry the original ReadingMemory allocation lease");
        auto lineBytes = allocation.requestedTotal - prefixBytes;
        Require(lineBytes % sizeof(ReadingLine) == 0 &&
                    lineBytes / sizeof(ReadingLine) >= fresh->navigation->LineCount(),
                L"line vector accounts its full allocated capacity including followY");
        results.lineSize = sizeof(ReadingLine);
        results.lineCapacity = lineBytes / sizeof(ReadingLine);
        results.lineAllocationBytes = allocation.requestedTotal;
        struct Retirement
        {
            void *payload;
            uint32_t physical{}, root{};
        } retired{payload};
        ReadingMemoryTestHooks hooks;
        hooks.context = &retired;
        hooks.observe = [](void *context, void *value, ReadingMemoryTestEvent event) noexcept
        {
            auto &observed = *static_cast<Retirement *>(context);
            if (value == observed.payload)
            {
                observed.physical += event == ReadingMemoryTestEvent::PhysicalFreed;
                observed.root += event == ReadingMemoryTestEvent::RootReleased;
            }
        };
        auto priorHooks = ReadingMemory::SetTestHooks(&hooks);
        auto mainLength = fresh->main->Length();
        fresh.reset();
        ReadingMemory::SetTestHooks(priorHooks);
        Require(retired.physical == 1 && retired.root == 1,
                L"the actual larger line allocation retires physical bytes and its original root lease once");

        // Real copied payload and current navigation exercise token authority.
        auto workspace = cache->TryWorkspace();
        Require(static_cast<bool>(workspace), L"fixed workspace available");
        bool contentionMiss{};
        std::thread competing([&] { contentionMiss = !cache->TryWorkspace(); });
        competing.join();
        Require(contentionMiss, L"busy workspace immediately misses on another worker");
        workspace.Key()[0] = 0xA7;
        auto bytes = workspace.Payload().first(56);
        Word(bytes, 0, TextMetricView::Magic);
        Word(bytes, 4, 2);
        Word(bytes, 8, 56);
        Word(bytes, 12, 1);
        Word(bytes, 16, 1);
        Word(bytes, 20, 1);
        Word(bytes, 24, std::bit_cast<uint32_t>(27.0f));
        Word(bytes, 28, std::bit_cast<uint32_t>(100.0f));
        Word(bytes, 32, std::bit_cast<uint32_t>(3.0f));
        Word(bytes, 36, std::bit_cast<uint32_t>(5.0f));
        Word(bytes, 40, 1);
        Word(bytes, 44, 1);
        Word(bytes, 48, 0);
        Word(bytes, 52, std::bit_cast<uint32_t>(27.0f));
        TextMetricView view{bytes};
        Require(view.Valid(1) && !view.Valid(2), L"copied local metric validates exact text length");
        for (auto offset : {32u, 36u})
        {
            for (auto invalid : {0xBF800000u, 0x7F800000u, 0x7FC00000u})
            {
                Word(bytes, offset, invalid);
                Require(!view.Valid(1), L"negative, infinite and NaN visual overflow rejected");
            }
            Word(bytes, offset, std::bit_cast<uint32_t>(offset == 32 ? 3.0f : 5.0f));
        }
        Word(bytes, 4, 1);
        Require(!view.Valid(1), L"old metric payload cannot omit visual overflow");
        Word(bytes, 4, 2);
        Word(bytes, 12, 0);
        Word(bytes, 40, 0);
        Word(bytes, 44, 0);
        Require(!TextMetricView{bytes}.Valid(0), L"malformed zero-length cluster rejected even for empty text");
        Word(bytes, 12, 1);
        Word(bytes, 40, 1);
        Word(bytes, 44, 1);
        ReadingNavigationIndex first{memory, key, mainLength};
        ReadingNavigationIndex second{memory, key, mainLength};
        auto token = first.AppendTextMetrics(0, 1, view);
        auto rejects = [](auto action)
        {
            try
            {
                action();
            }
            catch (ReadingFailure const &)
            {
                return true;
            }
            return false;
        };
        Require(rejects([&] { second.BindTextMetrics(token, 0, 0, 1, 0, 0); }), L"wrong navigation token rejected");
        Require(rejects([&] { first.BindTextMetrics(token, 0, 1, 2, 0, 0); }), L"wrong occurrence range rejected");
        first.BindTextMetrics(token, 0, 0, 1, 4, 9);
        Require(rejects([&] { first.BindTextMetrics(token, 0, 0, 1, 4, 9); }), L"used metric token rejected");
        results.tokenGuards = true;
        auto oldEpoch = cache->Stats().epoch;
        workspace.Put(1, 56, oldEpoch);
        Require(workspace.Lookup(1, oldEpoch).disposition == NOTEPADS_MARKDOWN_METRIC_CACHE_HIT, L"native copy-out actually hits");
        cache->Clear();
        workspace.Put(1, 56, oldEpoch);
        Require(workspace.Lookup(1, oldEpoch).disposition == NOTEPADS_MARKDOWN_METRIC_CACHE_STALE_EPOCH,
                L"old renderer epoch cannot adopt the cleared cache epoch");
        Require(workspace.Lookup(1, cache->Stats().epoch).disposition == NOTEPADS_MARKDOWN_METRIC_CACHE_MISS &&
                    cache->Stats().epoch != oldEpoch,
                L"clear rejects late writer without waiting for workspace");
        results.epochAndContention = true;
        auto stats = cache->Stats();
        results.hits = stats.hits;
        results.misses = stats.misses;
        results.nativeBytes = cache->NativeBytes();
        results.cacheBytes = stats.allocated_bytes;
        Require(results.nativeBytes <= 1024 * 1024 && results.cacheBytes <= NOTEPADS_MARKDOWN_METRIC_CACHE_TOTAL_BYTES,
                L"actual cache plus native wrapper policy stays within nine MiB");
        results.budgets = true;
        return results;
    }
}
