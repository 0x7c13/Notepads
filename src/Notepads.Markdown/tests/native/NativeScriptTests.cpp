// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeScriptTests.h"
#include "NativeTextBitmapCacheTests.h"
#include "Rendering/NativeDocumentRenderer.h"
#include "Text/NativeScriptLayout.h"
#include "Text/NativeScriptFonts.h"
#include "Reading/NativeReadingPreparation.h"
#include "Rendering/NativeMeasurementCache.h"

namespace Notepads::Markdown::Native::ScriptTests
{
    namespace
    {
        void Require(bool value, wchar_t const *message)
        {
            if (!value)
                throw winrt::hresult_error(E_FAIL, message);
        }

        uint32_t Find(std::wstring_view text)
        {
            auto at = FixtureMain.find(text);
            Require(at != std::wstring_view::npos && FixtureMain.find(text, at + 1) == std::wstring_view::npos,
                    L"script fixture token has one authored location");
            return static_cast<uint32_t>(at);
        }

        PreviewTextVisual const &Text(PreviewFrame const &frame, uint32_t position)
        {
            for (auto const &block : frame.Blocks())
                for (auto const &text : block.texts)
                    if (position >= text.logicalStart && position < text.logicalEnd)
                        return text;
            throw winrt::hresult_error(E_FAIL, L"script fixture position has materialized text");
        }

        uint16_t ClusterLength(PreviewTextVisual const &text, uint32_t logical)
        {
            auto position = static_cast<uint32_t>(logical - text.logicalStart);
            UINT32 count{};
            auto status = text.layout->GetClusterMetrics(nullptr, 0, &count);
            if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                winrt::check_hresult(status);
            std::vector<DWRITE_CLUSTER_METRICS> rows(count);
            if (count)
                winrt::check_hresult(text.layout->GetClusterMetrics(rows.data(), count, &count));
            uint32_t start{};
            for (auto const &row : rows)
            {
                if (position >= start && position - start < row.length)
                    return row.length;
                start += row.length;
            }
            throw winrt::hresult_error(E_FAIL, L"script authored position has a real cluster");
        }

        void CancelledFontPreparation(std::shared_ptr<ReadingMemory> const &memory)
        {
            auto model = memory->ModelOwner().Get();
            auto sourceCancel = std::make_shared<PreviewCancellation>(model);
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                memory, sourceCancel, 29, NextReadingTextToken());
            reading->EnsureMain();
            auto originalMain = reading->Main();
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = memory->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = 300;
            key.indent = 24;
            key.gap = 12;
            auto oldCancel = std::make_shared<PreviewCancellation>(model);
            struct LayoutLease
            {
                notepads_markdown_preview_layout *value{};
                ~LayoutLease() { if (value) (void)notepads_markdown_preview_layout_destroy(value); }
            } owner;
            notepads_markdown_core_error error{};
            error.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(error)};
            Require(notepads_markdown_preview_layout_create(model, oldCancel->Get(), &key, nullptr, 0,
                        NOTEPADS_MARKDOWN_PREVIEW_LAYOUT_VISIBLE, 0, &owner.value, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                    L"cancelled-font diagnostic creates an independently cancellable real layout");
            winrt::com_ptr<IDWriteFactory> factory;
            winrt::check_hresult(DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED, __uuidof(IDWriteFactory),
                reinterpret_cast<IUnknown **>(factory.put())));
            auto fonts = NativeScriptFonts::Create(owner.value, factory.get());
            winrt::com_ptr<IDWriteTextFormat> format;
            winrt::check_hresult(factory->CreateTextFormat(L"Consolas", nullptr, DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 18, L"en-US", format.put()));
            constexpr wchar_t text[] = L"👩‍💻";
            constexpr uint32_t length = static_cast<uint32_t>(std::size(text) - 1);
            winrt::com_ptr<IDWriteTextLayout> layout;
            winrt::check_hresult(factory->CreateTextLayout(text, length, format.get(), 30, 1000, layout.put()));
            notepads_markdown_preview_run run{};
            run.end = length;
            run.link = NOTEPADS_MARKDOWN_PREVIEW_NONE;
            run.size_scale = 1;
            auto binding = fonts->Bind(layout.get(), {&run, 1}, length);
            // No metrics, cluster or Draw query has resolved the fallback yet.
            oldCancel->Cancel();
            HRESULT callbackStatus = S_OK;
            HRESULT surfaced = S_OK;
            int32_t nativeStatus = NOTEPADS_MARKDOWN_CORE_OK;
            bool ordinaryHresult = false;
            try
            {
                PrepareReadingSource(reading, [&]
                {
                    DWRITE_TEXT_METRICS metrics{};
                    callbackStatus = layout->GetMetrics(&metrics);
                    winrt::check_hresult(callbackStatus);
                });
            }
            catch (PreviewFailure const &failure)
            {
                nativeStatus = failure.status;
                surfaced = failure.code();
            }
            catch (winrt::hresult_error const &failure)
            {
                ordinaryHresult = true;
                surfaced = failure.code();
            }
            Require(callbackStatus == HRESULT_FROM_WIN32(ERROR_CANCELLED) && surfaced == callbackStatus,
                    L"real DWrite fallback returns the translated cancellation HRESULT");
            if (reading->fault != NOTEPADS_MARKDOWN_CORE_OK)
                throw winrt::hresult_error(E_FAIL, winrt::hstring{L"real cancelled font callback poisoned the shared reading source: HRESULT=" +
                    std::to_wstring(callbackStatus) + L"; ordinaryHresult=" + std::to_wstring(ordinaryHresult) +
                    L"; nativeStatus=" + std::to_wstring(nativeStatus) + L"; fault=" + std::to_wstring(reading->fault)});
            Require(!ordinaryHresult && nativeStatus == NOTEPADS_MARKDOWN_CORE_CANCELLED,
                    L"COM cancellation is restored to its native status for worker arbitration");
            reading->EnsureMain();
            Require(reading->Main() == originalMain, L"cancelled layout preserves the exact same readable Main");
            auto freshCancel = std::make_shared<PreviewCancellation>(model);
            PreviewTheme theme;
            theme.fontFamily = theme.codeFontFamily = L"Consolas";
            theme.fontSize = 18;
            theme.lineHeight = 27;
            auto renderer = NativeDocumentRenderer::Create({}, freshCancel, key, theme, reading, {}, {}, {});
            std::atomic<uint64_t> latest{1};
            auto recovered = renderer->CreateFrame(0, 10000, 1, latest);
            Require(recovered && recovered->main == originalMain && !reading->fault &&
                        recovered->main->Text() == std::wstring{FixtureMain} + std::wstring{NearbySuffixMain},
                    L"the same source recovers a real new root frame with only a fresh layout cancellation flag");

            auto permanentCancel = std::make_shared<PreviewCancellation>(model);
            auto permanent = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                memory, permanentCancel, 31, NextReadingTextToken());
            permanent->EnsureMain();
            HRESULT permanentStatus = S_OK;
            try
            {
                PrepareReadingSource(permanent, [] { throw winrt::hresult_error(E_FAIL); });
            }
            catch (winrt::hresult_error const &failure) { permanentStatus = failure.code(); }
            Require(permanentStatus == E_FAIL && permanent->fault == E_FAIL,
                    L"unclassified HRESULT failures retain their original permanent source-fault semantics");
            bool remainsFaulted = false;
            try { permanent->EnsureMain(); }
            catch (ReadingFailure const &failure) { remainsFaulted = failure.status == E_FAIL; }
            Require(remainsFaulted, L"the cancellation correction does not hide a genuine recorded source fault");
        }

        void Draw(PreviewFrame const &frame)
        {
            winrt::com_ptr<ID3D11Device> d3d;
            D3D_FEATURE_LEVEL feature{};
            winrt::check_hresult(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION, d3d.put(), &feature, nullptr));
            auto dxgi = d3d.as<IDXGIDevice>();
            winrt::com_ptr<ID2D1Factory1> factory;
            winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, factory.put()));
            winrt::com_ptr<ID2D1Device> device;
            winrt::check_hresult(factory->CreateDevice(dxgi.get(), device.put()));
            winrt::com_ptr<ID2D1DeviceContext> context;
            winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, context.put()));
            winrt::com_ptr<ID2D1Bitmap1> target;
            auto height = static_cast<uint32_t>(std::ceil(frame.totalHeight + 32));
            auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_TARGET,
                D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED));
            winrt::check_hresult(context->CreateBitmap({400, height}, nullptr, 0, properties, target.put()));
            context->SetTarget(target.get());
            for (auto selected : {false, true})
            {
                context->BeginDraw();
                context->Clear(D2D1::ColorF(D2D1::ColorF::White));
                NativeDocumentRenderer::Draw(frame, context.get(), 8, 8, 0,
                    selected ? static_cast<uint32_t>(frame.main->Length()) : 0);
                winrt::check_hresult(context->EndDraw());
            }
            context->SetTarget(nullptr);
        }
    }

    std::wstring Run(std::shared_ptr<ReadingMemory> const &memory)
    {
        Require(memory && memory->ModelOwner(), L"script tests use a real current model owner");
        std::atomic<uint64_t> latest{1};
        const auto expectedMain = std::wstring{FixtureMain} + std::wstring{NearbySuffixMain};
        auto make = [&](std::shared_ptr<NativeMeasurementCache> const &cache, double width, bool systemBody = false)
        {
            auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                                                                    memory, cancel, 23, NextReadingTextToken());
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = memory->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = width;
            key.indent = 24;
            key.gap = 12;
            PreviewTheme theme;
            theme.fontFamily = systemBody ? L"Segoe UI" : L"Consolas";
            theme.codeFontFamily = L"Consolas";
            theme.fontSize = 18;
            theme.lineHeight = 27;
            theme.foreground = {255, 20, 20, 20};
            theme.linkForeground = {255, 40, 90, 220};
            auto renderer = NativeDocumentRenderer::Create({}, cancel, key, theme, reading, {}, {}, {}, cache);
            auto frame = renderer->CreateFrame(0, 10000, 1, latest);
            Require(frame && frame->main && frame->navigation && !reading->fault && frame->main->Text() == expectedMain,
                    L"script frame preserves the independently authored Main with no reading fault");
            return frame;
        };
        auto fresh = make({}, 300);
        auto cache = NativeMeasurementCache::Create();
        auto cold = make(cache, 300);
        auto before = cache->Stats();
        auto warm = make(cache, 300);
        auto after = cache->Stats();
        Require(after.hits > before.hits, L"script repeated model actually consumes cached line metrics");
        Require(fresh->totalHeight == cold->totalHeight && fresh->totalHeight == warm->totalHeight &&
                    fresh->navigation->LineCount() == warm->navigation->LineCount(),
                L"script fresh, cold and cached frame heights and navigation agree");
        for (size_t index = 0; index < fresh->navigation->LineCount(); ++index)
        {
            auto const &a = fresh->navigation->Line(index);
            auto const &b = warm->navigation->Line(index);
            Require(a.text.start == b.text.start && a.text.end == b.text.end && a.y == b.y && a.height == b.height,
                    L"script cached line intervals and final physical rows agree");
        }

        auto base = Find(L"pre á post.") + 4;
        auto const &mixed = Text(*fresh, base);
        Require(mixed.scripts && mixed.scriptDrawing && mixed.scriptFonts && mixed.scriptFonts->Frozen(),
                L"current root publishes resolved scripts and frozen fonts with immutable paint");
        auto local = static_cast<uint32_t>(base - mixed.logicalStart);
        Require(ClusterLength(mixed, base) == 1 && ClusterLength(mixed, base + 1) == 1 &&
                    mixed.scripts->Caret(local + 1).metrics.textPosition == local + 1 &&
                    std::abs(mixed.scripts->BaselineDeltaAt(local) + 7) < 0.01f &&
                    std::abs(mixed.scripts->BaselineDeltaAt(local + 1) - 4.6f) < 0.01f,
                L"different scripts keep independently shaped base and combining mark with both real baselines");
        auto siblings = Find(L"same á mono á.") + 5;
        auto mono = siblings + 8;
        auto const &same = Text(*fresh, siblings);
        Require(ClusterLength(same, siblings) == 1 && ClusterLength(same, siblings + 1) == 1 &&
                    ClusterLength(same, mono) == 2,
                L"same-geometry sibling scopes split while one authored container preserves composition");
        auto entity = Find(L"entity á emoji 👩‍💻");
        auto const &entities = Text(*fresh, entity);
        Require(ClusterLength(entities, entity + 7) == 2 && ClusterLength(entities, entity + 16) == 5,
                L"entity-created IR fragments inside one semantic container preserve Latin and ZWJ shaping");
        auto linkedEntity = Find(L"link-entity 👩‍💻");
        auto const &linkedText = Text(*fresh, linkedEntity);
        Require(linkedText.links.size() == 1 && linkedText.links.front().node != NOTEPADS_MARKDOWN_PREVIEW_NONE &&
                    ClusterLength(linkedText, linkedEntity + 12) == 5,
                L"one real link owns all entity fragments without introducing extra paint boundaries");
        auto const &twoLinks = Text(*fresh, Find(L"two-links leftright"));
        Require(twoLinks.links.size() == 2 && twoLinks.links[0].node != twoLinks.links[1].node,
                L"two actual adjacent same-URL links retain separate node identities");

        uint64_t images{}, math{}, code{};
        for (auto const &block : fresh->Blocks())
            for (auto const &text : block.texts)
            {
                if (!text.scripts)
                    continue;
                for (auto const &image : text.images)
                {
                    Require(text.height >= 27 && std::abs(text.scripts->FontSizeAt(image.offset) - 15) < 0.01f,
                            L"small image placeholder keeps themed minimum row and inherited script font size");
                    ++images;
                }
                for (auto const &formula : text.math)
                {
                    Require(text.height >= 27 && formula.label,
                            L"math placeholder keeps a readable label and themed minimum row");
                    ++math;
                }
                code += text.inlineCode.size();
            }
        Require(images == 1 && math == 1 && code >= 3,
                L"actual core objects and separate code/kbd/samp badges enter the script root path");
        auto tail = Find(L"tail note\n");
        auto const &last = Text(*fresh, tail);
        Require(last.scripts && last.scripts->Lines().size() == 1 && last.height == 27 &&
                    last.scripts->Caret(last.utf16Length).caretY < 27,
                L"terminal HTML break has one visible row and a caret on that row");
        auto narrow = make({}, 30);
        auto emoji = Find(L"\n👩‍💻\nA\nB\n") + 1;
        auto const &wrapped = Text(*narrow, emoji);
        Require(wrapped.scripts && wrapped.scripts->Lines().size() == 2 && wrapped.height >= 54,
                L"true script boundaries produce the final two-row emoji wrap before publication");
        auto checkGeometry = [&](PreviewFrame const &frame, uint32_t expectedNegative)
        {
            auto cross = Find(L"\nA\nB\ņ́\ņ́\ņ́\ntail") + 1;
            auto const &pair = Text(frame, cross);
            Require(pair.scripts && pair.scripts->Lines().size() == 2,
                    L"cross-line selection fixture has real sub and sup rows");
            auto localA = static_cast<uint32_t>(cross - pair.logicalStart);
            FLOAT x{}, y{};
            DWRITE_HIT_TEST_METRICS nativeA{};
            winrt::check_hresult(pair.layout->HitTestTextPosition(localA, FALSE, &x, &y, &nativeA));
            ReadingVector<NativeScriptRangeMetric> selectedA{ReadingAllocator<NativeScriptRangeMetric>{memory}};
            pair.scripts->Range(localA, 1, selectedA);
            Require(selectedA.size() == 1 && selectedA[0].line == 0 &&
                        std::abs(selectedA[0].metrics.height - nativeA.height) < 0.001f &&
                        double(selectedA[0].metrics.top) + selectedA[0].metrics.height <=
                            double(pair.scripts->Lines()[1].finalY) + 0.001,
                    L"first-row selection uses its actual font height and stops before the next physical row");
            auto b = pair.scripts->Caret(localA + 2).metrics;
            auto hitB = pair.scripts->HitTestPoint(b.left + b.width * 0.5f, b.top + b.height * 0.5f);
            Require(hitB.line == 1 && hitB.metrics.textPosition == localA + 2,
                    L"shifted second-row letter retains its original owner during global hit testing");

            uint32_t negative{};
            for (uint32_t index = 0; index != 3; ++index)
            {
                auto start = cross + 4 + index * 3;
                auto const &text = Text(frame, start);
                Require((text.scripts != nullptr) == (index != 1),
                        L"plain code exercises the actual nonscript root path; sup containers exercise scripted geometry");
                auto local = static_cast<uint32_t>(start - text.logicalStart);
                FLOAT leading{}, trailing{}, leadingY{}, trailingY{};
                DWRITE_HIT_TEST_METRICS raw{}, ignored{};
                winrt::check_hresult(text.layout->HitTestTextPosition(local, FALSE, &leading, &leadingY, &raw));
                winrt::check_hresult(text.layout->HitTestTextPosition(local, TRUE, &trailing, &trailingY, &ignored));
                auto rectangles = NativeDocumentRenderer::Rectangles(frame, start, start + 2);
                Require(rectangles.size() == 1 && rectangles[0].width >= 0 && rectangles[0].height > 0,
                        L"actual root returns a nonnegative physical rectangle for leading combining marks");
                auto caret = NativeDocumentRenderer::Rectangles(frame, start, start);
                Require(caret.size() == 1, L"leading combining marks retain a root caret");
                D2D1_POINT_2F origin{};
                bool located{};
                for (auto const &block : frame.Blocks())
                    for (auto const &candidate : block.texts)
                        if (&candidate == &text)
                        {
                            origin = {static_cast<float>(block.block.x) + text.x,
                                      static_cast<float>(block.block.y - frame.viewportY) + text.y};
                            located = true;
                        }
                auto nativeCaretTop = text.scripts ? text.scripts->Caret(local).caretY : leadingY;
                Require(located && std::abs(double(caret[0].x) - origin.x - leading) < 0.001 &&
                            std::abs(double(caret[0].y) - origin.y - nativeCaretTop) < 0.001,
                        L"root collapsed caret preserves native leading X at the independent materialized text origin");
                auto physicalLeft = std::min(double(raw.left), double(raw.left) + raw.width);
                auto physicalRight = std::max(double(raw.left), double(raw.left) + raw.width);
                Require(std::abs(double(rectangles[0].x) - origin.x - physicalLeft) < 0.001 &&
                            std::abs(double(rectangles[0].width) - (physicalRight - physicalLeft)) < 0.001,
                        L"root range preserves the exact native advance interval, including its direction");
                if (raw.width >= 0)
                    continue;
                ++negative;
                for (float fraction : {0.1f, 0.9f})
                {
                    auto localX = leading + (trailing - leading) * fraction;
                    auto hit = NativeDocumentRenderer::HitTest(frame, origin.x + localX,
                        rectangles[0].y + rectangles[0].height * 0.5f);
                    auto expected = start + (fraction > 0.5f ? 2 : 0);
                    Require(hit && hit->inside && hit->position == expected &&
                                hit->cluster.start == start && hit->cluster.end == start + 2,
                            L"root negative-advance clicks follow the nearest original native caret");
                }
                auto partial = NativeDocumentRenderer::Rectangles(frame, start, start + 1);
                Require(partial.size() == 1 && partial[0].width == 0 &&
                            partial[0].start == start && partial[0].end == start,
                        L"root partial leading mark preserves native zero-width and zero-length range identity");
            }
            Require(negative == expectedNegative,
                    L"real Consolas root paths exercise negative advances instead of passing only positive-width controls");
        };
        checkGeometry(*fresh, 3);
        checkGeometry(*warm, 3);
        auto systemBody = make({}, 300, true);
        checkGeometry(*systemBody, 2);
        Draw(*fresh);
        Draw(*warm);
        Draw(*systemBody);
        CancelledFontPreparation(memory);
        auto scriptPixels = TextBitmapCacheTests::RunScripts(memory);
        return L"scripts: real Core Main/owners; separate mark and sibling scopes; intact single container; "
               L"cold/warm rows; code/image/math placeholders; terminal br; narrow emoji wrap; cross-line font-height selection; "
               L"signed combining-mark root ranges and carets with plain and sup code; real COM cancellation and same-source recovery; cache hits=" +
               std::to_wstring(after.hits - before.hits) + scriptPixels;
    }
}
