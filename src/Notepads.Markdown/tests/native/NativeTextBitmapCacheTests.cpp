// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeTextBitmapCacheTests.h"
#include "Rendering/NativeDocumentRenderer.h"
#include "Rendering/NativeTextBitmapCache.h"
#include <algorithm>
#include <array>
#include <cstring>
#include <chrono>
#include <thread>

namespace Notepads::Markdown::Native::TextBitmapCacheTests
{
    namespace
    {
        void Require(bool value, wchar_t const *detail)
        {
            if (!value)
                throw Failure{detail};
        }

        struct PaintState
        {
            float dpi{96};
            float phase{};
            bool negativeOrigin{};
            D2D1_TEXT_ANTIALIAS_MODE antialias{D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE};
            std::optional<D2D1_RECT_F> clip;
            bool supplyPaintClip{true};
            std::optional<size_t> hoveredLink;
            float drawPhaseX{}, drawPhaseY{};
        };

        class Raster
        {
            winrt::com_ptr<ID2D1DeviceContext> _context;
            winrt::com_ptr<ID2D1Bitmap1> _target, _readback;

          public:
            static constexpr UINT32 Width = 1024, Height = 512;
            explicit Raster(ID2D1Factory1 *factory, bool hardware = false)
            {
                winrt::com_ptr<ID3D11Device> d3d;
                winrt::check_hresult(D3D11CreateDevice(nullptr, hardware ? D3D_DRIVER_TYPE_HARDWARE : D3D_DRIVER_TYPE_WARP, nullptr,
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
                    d3d.put(), nullptr, nullptr));
                winrt::com_ptr<ID2D1Device> device;
                winrt::check_hresult(factory->CreateDevice(d3d.as<IDXGIDevice>().get(), device.put()));
                winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, _context.put()));
                auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_TARGET,
                    D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96);
                winrt::check_hresult(_context->CreateBitmap(D2D1::SizeU(Width, Height), nullptr, 0, properties, _target.put()));
                properties.bitmapOptions = D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
                winrt::check_hresult(_context->CreateBitmap(D2D1::SizeU(Width, Height), nullptr, 0, properties, _readback.put()));
            }

            std::vector<uint8_t> Paint(PreviewFrame const &frame, NativeTextBitmapCache *cache = nullptr,
                                       PaintState state = {}, uint32_t start = 0, uint32_t end = 0,
                                       std::function<void()> const *repaint = nullptr)
            {
                auto scale = state.dpi / 96.0f;
                auto shift = state.phase / scale;
                _context->SetTarget(_target.get());
                _context->SetDpi(state.dpi, state.dpi);
                _context->SetUnitMode(D2D1_UNIT_MODE_DIPS);
                _context->SetPrimitiveBlend(D2D1_PRIMITIVE_BLEND_SOURCE_OVER);
                _context->SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                _context->SetTextAntialiasMode(state.antialias);
                _context->SetTextRenderingParams(nullptr);
                _context->SetTransform(D2D1::Matrix3x2F::Translation(
                    shift - (state.negativeOrigin ? 16.0f : 0.0f), shift));
                _context->BeginDraw();
                _context->Clear(D2D1::ColorF(D2D1::ColorF::White));
                if (state.clip)
                    _context->PushAxisAlignedClip(*state.clip, D2D1_ANTIALIAS_MODE_ALIASED);
                try
                {
                    NativeDocumentRenderer::Draw(frame, _context.get(), 16 + state.drawPhaseX, 16 + state.drawPhaseY, start, end,
                                                 {}, false, nullptr, false, state.hoveredLink,
                                                 state.supplyPaintClip ? state.clip : std::nullopt, cache, repaint);
                }
                catch (...)
                {
                    if (state.clip)
                        _context->PopAxisAlignedClip();
                    (void)_context->EndDraw();
                    _context->SetTarget(nullptr);
                    throw;
                }
                if (state.clip)
                    _context->PopAxisAlignedClip();
                auto status = _context->EndDraw();
                _context->SetTarget(nullptr);
                winrt::check_hresult(status);
                winrt::check_hresult(_readback->CopyFromBitmap(nullptr, _target.get(), nullptr));
                D2D1_MAPPED_RECT mapped{};
                winrt::check_hresult(_readback->Map(D2D1_MAP_OPTIONS_READ, &mapped));
                struct Unmap
                {
                    ID2D1Bitmap1 *bitmap;
                    ~Unmap() { (void)bitmap->Unmap(); }
                } unmap{_readback.get()};
                std::vector<uint8_t> pixels(static_cast<size_t>(Width) * Height * 4);
                for (UINT32 y = 0; y < Height; ++y)
                    std::memcpy(pixels.data() + static_cast<size_t>(y) * Width * 4,
                                mapped.bits + static_cast<size_t>(y) * mapped.pitch, Width * 4);
                return pixels;
            }

            void Prune(NativeTextBitmapCache &cache)
            {
                _context->SetTarget(_target.get());
                cache.Begin(_context.get());
                _context->SetTarget(nullptr);
            }
        };

        void Equal(std::vector<uint8_t> const &actual, std::vector<uint8_t> const &expected,
                   wchar_t const *detail)
        {
            if (actual == expected)
                return;
            size_t different{};
            unsigned maximum{};
            for (size_t i = 0; i < std::min(actual.size(), expected.size()); ++i)
            {
                auto delta = static_cast<unsigned>(std::abs(int{actual[i]} - int{expected[i]}));
                different += delta != 0;
                maximum = std::max(maximum, delta);
            }
            // Transparent BGRA8 SourceOver adds one quantization step: at most one LSB.
            // The oracle remains the original direct draw; no golden is regenerated.
            if (actual.size() == expected.size() && maximum <= 1)
                return;
            throw Failure{std::wstring{detail} + L"; different bytes=" +
                std::to_wstring(different) + L", maximum channel difference=" + std::to_wstring(maximum)};
        }
    }

    namespace
    {
        std::wstring PreparedPixels(std::shared_ptr<ReadingMemory> const &memory)
        {
            // Headless/WARP machines keep the existing deterministic oracle.
            // A hardware-capable run must actually complete and upload a result.
            winrt::com_ptr<ID3D11Device> probe;
            if (FAILED(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                nullptr, 0, D3D11_SDK_VERSION, probe.put(), nullptr, nullptr)))
                return L" Independent hardware preparation unavailable; synchronous WARP oracle only.";
            winrt::com_ptr<IDXGIAdapter> adapter;
            winrt::check_hresult(probe.as<IDXGIDevice>()->GetAdapter(adapter.put()));
            DXGI_ADAPTER_DESC1 description{};
            winrt::check_hresult(adapter.as<IDXGIAdapter1>()->GetDesc1(&description));
            if (description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE)
                return L" Software adapter keeps synchronous drawing; hardware preparation is unqualified.";
            for (double scale : {1.0, 2.0})
            {
                winrt::com_ptr<ID2D1Factory1> factory;
                winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, factory.put()));
                Raster raster{factory.get(), true};
                NativeTextBitmapCache cache;
                auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
                auto token = NextReadingTextToken();
                auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                    memory, cancel, 23, token);
                reading->PublishMain(MainTextSnapshot::Build(memory, cancel->Get(), 23, token));
                notepads_markdown_preview_key key{};
                key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
                key.model = reading->Main()->Identity();
                key.layout_generation = key.theme_generation = key.measurement_generation = 1;
                key.width = 560;
                key.indent = 24;
                key.gap = 12;
                PreviewTheme theme;
                theme.physicalScale = scale;
                auto renderer = NativeDocumentRenderer::Create({}, cancel, key, theme, reading);
                std::atomic<uint64_t> latest{1};
                auto frame = renderer->CreateFrame(0, 10000, 1, latest);
                Require(frame && frame->textPrewarm, L"the cold fixture publishes its bounded preparation handle");
                PaintState state;
                state.dpi = static_cast<float>(96 * scale);
                std::vector<uint8_t> firstPhasePixels;
                for (uint64_t phase = 0; phase < 2; ++phase)
                {
                    auto phaseFrame = *frame;
                    auto phaseBlocks = std::make_shared<PreviewVector<PreviewBlockVisual>>(frame->Blocks().get_allocator());
                    phaseBlocks->assign(frame->Blocks().begin(), frame->Blocks().end());
                    phaseFrame.measuredBlocks = phaseBlocks;
                    std::array<notepads_markdown_preview_block, 1> positioned{phaseBlocks->front().block};
                    NativeTextBitmapCache::PositionPrewarm(frame->textPrewarm, positioned);
                    // Real split-view surface origins include padding plus a
                    // nonzero physical fraction. Keep it separate from the
                    // integral SIS context transform, just as the control does.
                    state.drawPhaseX = static_cast<float>((phase ? 0.625 : 0.375) / scale);
                    state.drawPhaseY = static_cast<float>((phase ? 0.40625 : 0.21875) / scale);
                    const bool checkPending = scale == 1.0 && phase == 0;
                    auto gate = checkPending ? std::make_shared<NativeTextBitmapCache::PreparationGate>() : nullptr;
                    if (gate) NativeTextBitmapCache::SetPreparationGate(phaseFrame, gate);
                    // Selection starts preparation using the actual target but
                    // bypasses every normal cache entry. Its layouts are distinct
                    // from the measurement-only layouts used by the worker.
                    (void)raster.Paint(phaseFrame, &cache, state, 0, frame->main->Length());
                    Require(cache.Stats().entries == phase * 2,
                            L"selection does not prepopulate pixels for the new real origin phase");
                    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(10);
                    if (gate)
                    {
                        while (!gate->entered.load() && std::chrono::steady_clock::now() < deadline)
                            std::this_thread::sleep_for(std::chrono::milliseconds(1));
                        Require(gate->entered.load(), L"the pending pixel test reached the worker gate");
                        auto notifications = std::make_shared<std::atomic<unsigned>>(0);
                        std::function<void()> repaint = [notifications] { ++*notifications; };
                        const auto placeholder = raster.Paint(phaseFrame, &cache, state, 0, 0, &repaint);
                        const auto ink = phaseFrame.theme.borderColor;
                        bool hasBar{};
                        for (size_t i = 0; i + 3 < placeholder.size(); i += 4)
                            if (placeholder[i] == ink.B && placeholder[i + 1] == ink.G &&
                                placeholder[i + 2] == ink.R && placeholder[i + 3] == 255) { hasBar = true; break; }
                        // A second draw while waiting shares the same pending notification.
                        const auto same = raster.Paint(phaseFrame, &cache, state, 0, 0, &repaint);
                        gate->released = true;
                        Require(hasBar && placeholder == same && cache.Stats().prewarmUploads == 0,
                                L"a pending row paints stable skeleton bars while the worker owns its exact pixels");
                        while (!notifications->load() && std::chrono::steady_clock::now() < deadline)
                            std::this_thread::sleep_for(std::chrono::milliseconds(1));
                        Require(notifications->load() == 1, L"worker completion requests repaint once without another scroll");
                        Require(placeholder != raster.Paint(phaseFrame, nullptr, state),
                                L"the temporary bars and the final colored text have different pixels");
                    }
                    while (!NativeTextBitmapCache::PreparedCount(frame->textPrewarm) &&
                           std::chrono::steady_clock::now() < deadline)
                    {
                        std::this_thread::sleep_for(std::chrono::milliseconds(1));
                        // If the worker is just retiring, the next paint can
                        // schedule the latest pending item without waiting.
                        (void)raster.Paint(phaseFrame, &cache, state, 0, frame->main->Length());
                    }
                    Require(NativeTextBitmapCache::PreparedCount(frame->textPrewarm) > 0,
                            L"independent same-adapter worker completes the real dense-Emoji bitmap at each origin");
                    (void)raster.Paint(phaseFrame, &cache, state, 0, frame->main->Length());
                    Require(NativeTextBitmapCache::PreparedCount(frame->textPrewarm) == 1,
                            L"unchanged actual origin preserves its ready result without another request");
                    auto translationGate = std::make_shared<NativeTextBitmapCache::PreparationGate>();
                    NativeTextBitmapCache::SetPreparationGate(phaseFrame, translationGate);
                    auto translated = state;
                    translated.drawPhaseY += static_cast<float>(2 / scale);
                    (void)raster.Paint(phaseFrame, &cache, translated, 0, frame->main->Length());
                    const auto translatedPrepared = NativeTextBitmapCache::PreparedCount(frame->textPrewarm);
                    translationGate->released = true;
                    NativeTextBitmapCache::SetPreparationGate(phaseFrame, nullptr);
                    Require(translatedPrepared == 1,
                            L"an integral physical surface-origin change preserves ready CPU pixels");
                    phaseBlocks->front().block.y += 2 / scale;
                    positioned[0] = phaseBlocks->front().block;
                    NativeTextBitmapCache::PositionPrewarm(frame->textPrewarm, positioned);
                    Require(NativeTextBitmapCache::PreparedCount(frame->textPrewarm) == 1,
                            L"integral physical block displacement preserves already-published CPU pixels");
                    phaseBlocks->front().block.y += 0.34375 / scale;
                    positioned[0] = phaseBlocks->front().block;
                    NativeTextBitmapCache::PositionPrewarm(frame->textPrewarm, positioned);
                    (void)raster.Paint(phaseFrame, &cache, state, 0, frame->main->Length());
                    const auto repositionDeadline = std::chrono::steady_clock::now() + std::chrono::seconds(10);
                    while (!NativeTextBitmapCache::PreparedCount(frame->textPrewarm) &&
                           std::chrono::steady_clock::now() < repositionDeadline)
                    {
                        std::this_thread::sleep_for(std::chrono::milliseconds(1));
                        (void)raster.Paint(phaseFrame, &cache, state, 0, frame->main->Length());
                    }
                    Require(NativeTextBitmapCache::PreparedCount(frame->textPrewarm) == 1,
                            L"changed physical block phase is prepared again after the older CPU result was published");
                    auto expected = raster.Paint(phaseFrame, nullptr, state);
                    if (phase)
                        Require(expected != firstPhasePixels, L"the second actual origin has independently different glyph pixels");
                    else
                        firstPhasePixels = expected;
                    Equal(raster.Paint(phaseFrame, &cache, state), expected,
                          L"hardware CPU handoff/upload pixels equal current direct drawing at fractional origins and 1x/2x");
                    Require(cache.Stats().prewarmUploads == phase + 1 && cache.Stats().entries == (phase + 1) * 2,
                            L"each new real origin uploads one exact matching prepared Emoji bitmap");
                    auto before = cache.Stats();
                    Equal(raster.Paint(phaseFrame, &cache, state), expected, L"prepared hardware entry preserves warm exact-key pixels");
                    Require(cache.Stats().hits == before.hits + 2 && cache.Stats().prewarmUploads == before.prewarmUploads,
                            L"the uploaded origin entry becomes an ordinary weak-owned bitmap hit");
                }
                if (scale == 1.0)
                {
                    auto gate = std::make_shared<NativeTextBitmapCache::PreparationGate>();
                    NativeTextBitmapCache::SetPreparationGate(*frame, gate);
                    state.drawPhaseX = 0.125f;
                    state.drawPhaseY = 0.25f;
                    auto notifications = std::make_shared<std::atomic<unsigned>>(0);
                    std::function<void()> repaint = [notifications] { ++*notifications; };
                    // The preceding synthetic relayout moved the worker's block.
                    // Publish the original frame geometry before drawing this frame again.
                    std::array<notepads_markdown_preview_block, 1> positioned{frame->Blocks().front().block};
                    NativeTextBitmapCache::PositionPrewarm(frame->textPrewarm, positioned);
                    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(10);
                    while (!gate->entered.load() && std::chrono::steady_clock::now() < deadline)
                    {
                        (void)raster.Paint(*frame, &cache, state, 0, frame->main->Length());
                        std::this_thread::sleep_for(std::chrono::milliseconds(1));
                    }
                    Require(gate->entered.load(), L"the readback-failure test reached its worker gate");
                    const auto placeholder = raster.Paint(*frame, &cache, state, 0, 0, &repaint);
                    gate->failBeforePublish = true;
                    gate->released = true;
                    while (!notifications->load() && std::chrono::steady_clock::now() < deadline)
                        std::this_thread::sleep_for(std::chrono::milliseconds(1));
                    Require(notifications->load() == 1, L"a failed pixel readback wakes the waiting view");
                    const auto expected = raster.Paint(*frame, nullptr, state);
                    Require(placeholder != expected, L"the deferred failure case exercised a placeholder");
                    Equal(raster.Paint(*frame, &cache, state, 0, 0, &repaint), expected,
                          L"the repaint after failure replaces the skeleton with complete direct text");
                }
                cache.Clear();
                Require(NativeTextBitmapCache::PreparedCount(frame->textPrewarm) == 0 && cache.Stats().bytes == 0,
                        L"Clear retires both prepared CPU payloads and uploaded bitmap bytes");
            }
            return L" Independent same-adapter hardware preparation at 1x/2x with two nonzero actual-origin phases, "
                   L"real CPU upload/direct pixels, ready-result stability, integral block reuse and fractional block re-preparation, warm hits "
                   L"and cancellation/CPU/GPU retirement passed.";
        }
    }

    std::wstring Run(std::shared_ptr<ReadingMemory> const &memory)
    {
        winrt::com_ptr<ID2D1Factory1> factory;
        winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, factory.put()));
        Raster raster{factory.get()};
        NativeTextBitmapCache cache;
        std::weak_ptr<MainTextSnapshot const> weakMain;
        {
            auto makeReading = [&]
            {
                auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
                auto token = NextReadingTextToken();
                auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                    memory, cancel, 23, token);
                reading->PublishMain(MainTextSnapshot::Build(memory, cancel->Get(), 23, token));
                return reading;
            };
            auto reading = makeReading();
            weakMain = reading->Main();
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = reading->Main()->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = 560;
            key.indent = 24;
            key.gap = 12;
            std::atomic<uint64_t> latest{1};
            auto make = [&](std::shared_ptr<ReadingSourceState> const &source, PreviewTheme theme = {})
            {
                auto renderer = NativeDocumentRenderer::Create({}, source->cancel, key, theme, source);
                auto frame = renderer->CreateFrame(0, 10000, 1, latest);
                Require(frame && frame->main->Text() == FixtureMain && frame->Blocks().size() == 2,
                        L"bitmap oracle uses the unchanged Lab Emoji and authored mixed-style paragraphs");
                return frame;
            };
            auto frame = make(reading);
            auto equivalent = make(reading);
            Require(frame->main == equivalent->main && frame->Blocks()[0].texts[0].layout.get() !=
                        equivalent->Blocks()[0].texts[0].layout.get(),
                    L"warm bitmap oracle rematerializes an equivalent actual DWrite layout");
            auto plain = raster.Paint(*frame);
            Require(std::any_of(plain.begin(), plain.end(), [](uint8_t value) { return value != 255; }),
                    L"Lab Emoji pixel oracle has actual ink on its white target");
            Equal(raster.Paint(*frame, &cache), plain, L"cold bitmap pixels equal original DrawTextLayout");
            auto before = cache.Stats();
            Require(before.entries == 2 && before.builds == 2 && before.bytes > 0,
                    L"both ordinary paragraphs actually populate the bounded bitmap cache");
            Equal(raster.Paint(*equivalent, &cache), plain, L"rematerialized warm pixels equal original DrawTextLayout");
            Require(cache.Stats().hits == before.hits + 2 && cache.Stats().builds == before.builds,
                    L"fresh equivalent layouts use final pixels rather than rebuild");
            auto shifted = *equivalent;
            shifted.viewportY += 4; // Integral physical translation at both 96 and 144 DPI.
            auto shiftedPixels = raster.Paint(shifted);
            before = cache.Stats();
            Equal(raster.Paint(shifted, &cache), shiftedPixels, L"integer viewport translation warm pixel oracle");
            Require(cache.Stats().hits == before.hits + 2 && cache.Stats().builds == before.builds,
                    L"integer viewport translation replays both existing paragraph bitmaps");

            // The source's signed physical x origin straddles -0.5; the positive
            // cases cover +0.5 at another DPI. These are exact phases, not buckets.
            constexpr std::array phases{
                PaintState{96, -0.51f, true}, PaintState{96, -0.49f, true},
                PaintState{144, 0.49f}, PaintState{144, 0.51f},
                PaintState{96, 0, false, D2D1_TEXT_ANTIALIAS_MODE_ALIASED}};
            for (auto const &state : phases)
            {
                auto stateDetail = L"; dpi=" + std::to_wstring(state.dpi) + L", phase=" +
                    std::to_wstring(state.phase) + L", negativeOrigin=" + std::to_wstring(state.negativeOrigin) +
                    L", AA=" + std::to_wstring(static_cast<uint32_t>(state.antialias));
                auto coldDetail = L"DPI/signed half-pixel/aliased cold pixel oracle" + stateDetail;
                auto warmDetail = L"DPI/signed half-pixel/aliased warm pixel oracle" + stateDetail;
                auto expected = raster.Paint(*frame, nullptr, state);
                before = cache.Stats();
                Equal(raster.Paint(*frame, &cache, state), expected, coldDetail.c_str());
                Require(cache.Stats().builds == before.builds + 2, L"distinct actual raster state builds new pixels");
                before = cache.Stats();
                Equal(raster.Paint(*equivalent, &cache, state), expected, warmDetail.c_str());
                Require(cache.Stats().hits == before.hits + 2, L"same actual raster state really replays");
            }

            cache.Clear();
            PaintState clipped;
            clipped.clip = D2D1::RectF(0, 32, 1024, 512);
            auto partial = raster.Paint(*frame, nullptr, clipped);
            Require(partial != plain, L"the authored viewport clip actually cuts visible Emoji ink");
            before = cache.Stats();
            Equal(raster.Paint(*frame, &cache, clipped), partial, L"partial-first viewport pixel oracle");
            Require(cache.Stats().builds == before.builds + 2, L"partial-first draw records both complete paragraphs");
            before = cache.Stats();
            Equal(raster.Paint(*frame, &cache), plain, L"partial-first recording retains the complete paragraph");
            Require(cache.Stats().hits == before.hits + 2, L"full draw actually replays partial-first entries");
            cache.Clear();
            Equal(raster.Paint(*frame, &cache), plain, L"full-first viewport pixel oracle");
            before = cache.Stats();
            Equal(raster.Paint(*frame, &cache, clipped), partial, L"full-first replay respects the current viewport clip");
            Require(cache.Stats().hits == before.hits + 2, L"clipped draw actually replays full-first entries");

            auto end = frame->main->Length();
            auto selected = raster.Paint(*frame, nullptr, {}, 0, end);
            before = cache.Stats();
            Equal(raster.Paint(*frame, &cache, {}, 0, end), selected, L"selection bypass preserves original foreground/background");
            Require(cache.Stats().hits == before.hits && cache.Stats().builds == before.builds,
                    L"selection intersecting both paragraphs bypasses the bitmap cache");
            Equal(raster.Paint(*frame, &cache), plain, L"selection cancellation replays clean old pixels");
            // The preceding hit deliberately left the selected layout's effects
            // untouched. A miss must clear them before recording new pixels.
            cache.Clear();
            Equal(raster.Paint(*frame, &cache), plain, L"cold recording cannot burn old selection effects into its bitmap");

            auto changed = *frame;
            ++changed.key.layout_generation;
            ++changed.key.measurement_generation;
            before = cache.Stats();
            Equal(raster.Paint(changed, &cache), plain, L"unrelated layout/measurement generations preserve pixels");
            Require(cache.Stats().hits == before.hits + 2 && cache.Stats().builds == before.builds,
                    L"unrelated readiness generations do not evict static ordinary paragraphs");
            ++changed.key.theme_generation;
            changed.theme.foreground = {255, 120, 25, 70};
            before = cache.Stats();
            auto expected = raster.Paint(changed);
            Equal(raster.Paint(changed, &cache), expected, L"changed theme uses its new original foreground pixels");
            Require(cache.Stats().builds == before.builds + 2, L"theme generation invalidates previous paragraph pixels");
            changed = *frame;
            ++changed.key.model.generation;
            before = cache.Stats();
            Equal(raster.Paint(changed, &cache), plain, L"changed frame model identity preserves original pixels");
            Require(cache.Stats().builds == before.builds + 2, L"frame model identity is part of the pixel key");
            auto nextReading = makeReading();
            auto nextFrame = make(nextReading);
            before = cache.Stats();
            Equal(raster.Paint(*nextFrame, &cache), plain, L"new Main owner preserves original pixels");
            Require(cache.Stats().builds == before.builds + 2, L"same model and content with a new Main owner cannot hit an old entry");

            // All three owners use the same nodes. Retire the middle of each
            // collision chain and verify both surviving owners can still replay.
            auto lastFrame = make(makeReading());
            (void)raster.Paint(*lastFrame, &cache);
            before = cache.Stats();
            nextFrame.reset();
            nextReading.reset();
            raster.Prune(cache);
            Require(cache.Stats().entries + 2 == before.entries, L"only the expired owner's two entries are removed");
            Equal(raster.Paint(*frame, &cache), plain, L"older owner survives collision-chain pruning");
            Equal(raster.Paint(*lastFrame, &cache), plain, L"newer owner survives collision-chain pruning");
            Require(cache.Stats().hits == before.hits + 4 && cache.Stats().builds == before.builds,
                    L"pruning one owner preserves lookup for both neighbours");

            Raster replacement{factory.get()};
            before = cache.Stats();
            Equal(replacement.Paint(*frame, &cache), replacement.Paint(*frame), L"new D2D device recreates compatible bitmap pixels");
            Require(cache.Stats().entries == 2 && cache.Stats().builds == before.builds + 2,
                    L"device transition retires old device entries before publishing new ones");
            PaintState clearType;
            clearType.antialias = D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE;
            before = cache.Stats();
            Equal(replacement.Paint(*frame, &cache, clearType), replacement.Paint(*frame, nullptr, clearType),
                  L"explicit ClearType uses the original path");
            Require(cache.Stats().builds == before.builds && cache.Stats().hits == before.hits,
                    L"unsupported ClearType target state does not build or replay grayscale pixels");
            cache.Clear();
            PreviewTheme tall;
            tall.lineHeight = 2100;
            auto oversized = make(reading, tall);
            before = cache.Stats();
            Equal(replacement.Paint(*oversized, &cache), replacement.Paint(*oversized),
                  L"over-limit nominal paragraph uses the unchanged original draw path");
            Require(cache.Stats().entries == 0 && cache.Stats().builds == before.builds,
                    L"over-limit paragraph does not allocate a resident bitmap");
            // Repopulate once so expiration proves weak ownership, rather than
            // merely observing the preceding explicit Clear.
            (void)raster.Paint(*frame, &cache);
            Require(cache.Stats().entries == 2, L"owner-retirement probe begins with resident bitmaps");
        }
        Require(weakMain.expired(), L"bitmap entries do not retain Main, model, layouts or measured blocks");
        raster.Prune(cache);
        Require(cache.Stats().entries == 0 && cache.Stats().bytes == 0,
                L"expired Main owners retire resident bitmap bytes on the next Begin");
        cache.Clear();
        Require(cache.Stats().entries == 0 && cache.Stats().bytes == 0, L"Clear leaves zero resident bitmap bytes");
        return L" WARP direct/cold/warm Lab Emoji and mixed italic/strike pixels; new equivalent layouts, signed "
               L"half-pixel phases, 96/144 DPI, aliased text, partial/full clips, selection cancellation, "
               L"theme/frame-identity/Main/device transitions, weak retirement and over-limit fallback passed. "
               L"No public UI, physical GPU memory or timing claim." + PreparedPixels(memory);
    }
    std::wstring RunStatic(std::shared_ptr<ReadingMemory> const &memory)
    {
        wchar_t const *stage = L"factory";
        try
        {
            winrt::com_ptr<ID2D1Factory1> factory;
            winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, factory.put()));
            stage = L"Raster/WARP target";
            Raster raster{factory.get()};
            NativeTextBitmapCache cache;
            std::weak_ptr<MainTextSnapshot const> weakMain;
            {
                stage = L"reading/Main setup";
                auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
                auto token = NextReadingTextToken();
                auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                    memory, cancel, 23, token);
                reading->PublishMain(MainTextSnapshot::Build(memory, cancel->Get(), 23, token));
                weakMain = reading->Main();
                notepads_markdown_preview_key key{};
                key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
                key.model = reading->Main()->Identity();
                key.layout_generation = key.theme_generation = key.measurement_generation = 1;
                key.width = 560;
                key.indent = 24;
                key.gap = 12;
                std::atomic<uint64_t> latest{1};
                // Frame allocations retire independently, but their allocators
                // borrow the renderer's live layout for any new vector copies.
                std::shared_ptr<NativeDocumentRenderer> frameRenderer, equivalentRenderer;
                auto make = [&](std::shared_ptr<NativeDocumentRenderer> &renderer)
                {
                    stage = L"make renderer";
                    renderer = NativeDocumentRenderer::Create({}, cancel, key, {}, reading);
                    stage = L"make Frame";
                    auto frame = renderer->CreateFrame(0, 10000, 1, latest);
                    Require(frame && frame->main->Text() == StaticFixtureMain && frame->Blocks().size() == 5,
                            L"static bitmap oracle uses its independent Main and five real measured blocks");
                    return frame;
                };
                auto frame = make(frameRenderer);
                auto equivalent = make(equivalentRenderer);
                stage = L"validate static visuals";
                auto const &blocks = frame->Blocks();
                auto bmpPrefixEnd = static_cast<size_t>(blocks[3].block.logical_start);
                Require(bmpPrefixEnd <= StaticFixtureMain.size() &&
                            std::none_of(StaticFixtureMain.begin(), StaticFixtureMain.begin() + bmpPrefixEnd,
                            [](wchar_t value) { return value >= 0xd800 && value <= 0xdfff; }) &&
                            blocks[0].block.kind == 2 && blocks[0].texts.size() == 1 &&
                            blocks[1].block.kind == 1 && blocks[1].texts.size() == 1 &&
                            blocks[2].block.kind == 6 && blocks[2].texts.size() == 4,
                        L"the BMP prefix contains a real heading, ordinary paragraph and four table text cells");
                constexpr uint32_t staticTexts = 8;
                constexpr std::array<size_t, 5> staticBlockIndices{0, 1, 2, 3, 4};
                for (size_t index : staticBlockIndices)
                    for (size_t textIndex = 0; textIndex != blocks[index].texts.size(); ++textIndex)
                    {
                        auto const &text = blocks[index].texts[textIndex];
                        Require(text.bitmapCandidate && text.matchesMainText && text.layout && !text.scriptDrawing,
                                L"every authored static heading/paragraph/table/inline-code text is an actual bitmap candidate");
                        Require(text.layout.get() != equivalent->Blocks()[index].texts[textIndex].layout.get(),
                                L"equivalent static Frame rematerializes each real DWrite layout");
                    }
                auto const &paragraph = blocks[1].texts[0];
                DWRITE_FONT_WEIGHT weight{};
                DWRITE_FONT_STYLE style{};
                BOOL strike{};
                stage = L"font weight/style/strike metrics";
                winrt::check_hresult(paragraph.layout->GetFontWeight(
                    static_cast<UINT32>(StaticFixtureMain.find(L"bold") - paragraph.logicalStart), &weight, nullptr));
                winrt::check_hresult(paragraph.layout->GetFontStyle(
                    static_cast<UINT32>(StaticFixtureMain.find(L"italic") - paragraph.logicalStart), &style, nullptr));
                winrt::check_hresult(paragraph.layout->GetStrikethrough(
                    static_cast<UINT32>(StaticFixtureMain.find(L"strike") - paragraph.logicalStart), &strike, nullptr));
                Require(weight >= DWRITE_FONT_WEIGHT_BOLD && style == DWRITE_FONT_STYLE_ITALIC && strike,
                        L"static bitmap pixels include actual bold, italic and strikethrough formatting");

                stage = L"plain direct paint";
                auto plain = raster.Paint(*frame);
                Require(std::any_of(plain.begin(), plain.end(), [](uint8_t value) { return value != 255; }),
                        L"static BMP bitmap oracle paints actual ink");
                stage = L"static cold cache paint";
                Equal(raster.Paint(*frame, &cache), plain, L"static heading/paragraph/table/inline-code cold pixels match direct draw");
                auto before = cache.Stats();
                Require(before.entries == staticTexts && before.builds == staticTexts && before.bytes > 0 &&
                            before.commandEntries == 2 && before.commandTextUnits ==
                                blocks[3].texts[0].utf16Length + blocks[4].texts[0].utf16Length,
                        L"six bitmap visuals plus link-only and mixed inline-code/link command entries really populate");
                stage = L"static equivalent warm cache paint";
                Equal(raster.Paint(*equivalent, &cache), plain, L"equivalent static text/inline-code warm pixels match direct draw");
                Require(cache.Stats().hits == before.hits + staticTexts && cache.Stats().builds == before.builds,
                        L"fresh equivalent heading/paragraph/table/link/inline-code layouts actually replay all eight existing entries");

                for (float dpi : {120.0f, 168.0f, 216.0f, 240.0f})
                {
                    // Fractional physical em sizes can change command-list
                    // hinting. Plain visuals retain exact raster pixels; rich
                    // visuals retain commands only for certified glyph runs.
                    PaintState fractionalDpi;
                    fractionalDpi.dpi = dpi;
                    NativeTextBitmapCache fractionalCache;
                    auto directPixels = raster.Paint(*frame, nullptr, fractionalDpi);
                    auto coldPixels = raster.Paint(*frame, &fractionalCache, fractionalDpi);
                    auto coldDetail = L"fractional-DPI heading/paragraph/table and rich link/code cold pixels match direct; DPI=" + std::to_wstring(dpi);
                    Equal(coldPixels, directPixels, coldDetail.c_str());
                    auto cold = fractionalCache.Stats();
                    Require(cold.entries >= staticTexts - 2 && cold.entries <= staticTexts &&
                                cold.commandEntries == cold.entries - (staticTexts - 2) && cold.bytes > 0,
                            L"fractional-DPI ordinary visuals genuinely retain pixels rather than a global cache bypass");
                    auto warmPixels = raster.Paint(*equivalent, &fractionalCache, fractionalDpi);
                    auto warmDetail = L"fractional-DPI equivalent layouts replay exact pixels while rich text stays faithful; DPI=" + std::to_wstring(dpi);
                    Equal(warmPixels, directPixels, warmDetail.c_str());
                    Require(fractionalCache.Stats().hits == cold.hits + cold.entries &&
                                fractionalCache.Stats().builds == cold.builds,
                            L"fractional-DPI warm replay really hits every admitted entry");
                }

                {
                    PreviewTheme integerEmTheme;
                    integerEmTheme.fontSize = 16;
                    integerEmTheme.lineHeight = 24;
                    auto integerEmKey = key;
                    ++integerEmKey.theme_generation;
                    ++integerEmKey.layout_generation;
                    ++integerEmKey.measurement_generation;
                    auto integerEmRenderer = NativeDocumentRenderer::Create({}, cancel, integerEmKey, integerEmTheme, reading);
                    auto integerEmFrame = integerEmRenderer->CreateFrame(0, 10000, 1, latest);
                    auto linkBlocks = std::make_shared<PreviewVector<PreviewBlockVisual>>(integerEmFrame->Blocks().get_allocator());
                    linkBlocks->push_back(integerEmFrame->Blocks()[3]);
                    auto linkOnly = *integerEmFrame;
                    linkOnly.measuredBlocks = linkBlocks;
                    linkOnly.viewportY = linkBlocks->front().block.y;
                    PaintState dpi125;
                    dpi125.dpi = 120;
                    NativeTextBitmapCache integerEmCache;
                    auto expected = raster.Paint(linkOnly, nullptr, dpi125);
                    Equal(raster.Paint(linkOnly, &integerEmCache, dpi125), expected,
                          L"16-DIP link text at 125 percent preserves direct color and glyph pixels");
                    Require(integerEmCache.Stats().commandEntries == 1,
                            L"whole physical-pixel glyph runs retain rich commands even at 125 percent");
                    Equal(raster.Paint(linkOnly, &integerEmCache, dpi125), expected,
                          L"certified 125-percent rich commands preserve genuine warm pixels");
                    Require(integerEmCache.Stats().hits == 1 && integerEmCache.Stats().builds == 1,
                            L"125-percent rich text is admitted by actual glyph size rather than a blanket DPI bypass");

                    // The base format is still 16 DIP (20 physical pixels),
                    // but this second actual style run is 17 DIP (21.25 pixels).
                    // Checking only the base font size would admit this layout.
                    auto mixedRun = linkOnly;
                    ++mixedRun.key.theme_generation;
                    auto const &richText = linkBlocks->front().texts.front();
                    const auto changed = richText.links.front().start;
                    winrt::check_hresult(richText.layout->SetFontSize(17, {changed, 1}));
                    FLOAT baseSize{}, changedSize{};
                    winrt::check_hresult(richText.layout->GetFontSize(0, &baseSize, nullptr));
                    winrt::check_hresult(richText.layout->GetFontSize(changed, &changedSize, nullptr));
                    Require(baseSize == 16 && changedSize == 17,
                            L"rich mixed-em fixture keeps an integral base run and a fractional physical style run");
                    NativeTextBitmapCache mixedRunCache;
                    stage = L"125-percent rich mixed-em negative admission";
                    auto mixedPixels = raster.Paint(mixedRun, nullptr, dpi125);
                    Equal(raster.Paint(mixedRun, &mixedRunCache, dpi125), mixedPixels,
                          L"fractional physical glyph run keeps genuine direct rich-text pixels");
                    auto rejected = mixedRunCache.Stats();
                    Require(rejected.entries == 0 && rejected.commandEntries == 0 && rejected.builds == 0 && rejected.failures == 1,
                            L"one fractional actual run rejects retained commands despite the integral base font");
                    Equal(raster.Paint(mixedRun, &mixedRunCache, dpi125), mixedPixels,
                          L"repeated mixed-em rich draw stays faithful through the negative epoch");
                    Require(mixedRunCache.Stats().failures == rejected.failures && mixedRunCache.Stats().hits == 0,
                            L"unsupported mixed-em input neither probes repeatedly nor replays stale command pixels");
                }

                // Same document Y, two exact integral viewport displacements:
                // before normalization the float origins are 528.1984252929688
                // and 22.198421478271484 and occupy distinct exact cache keys.
                for (size_t blockIndex : {size_t{1}, size_t{3}, size_t{4}})
                    for (float dpi : {96.0f, 144.0f, 192.0f})
                    {
                        auto viewportBlocks = std::make_shared<PreviewVector<PreviewBlockVisual>>(blocks.get_allocator());
                        viewportBlocks->push_back(blocks[blockIndex]);
                        viewportBlocks->back().block.y = 8271.198421478272;
                        auto firstViewport = *frame;
                        firstViewport.measuredBlocks = viewportBlocks;
                        firstViewport.viewportY = 7759;
                        auto secondViewport = firstViewport;
                        secondViewport.viewportY = 8265;
                        PaintState state;
                        state.dpi = dpi;
                        NativeTextBitmapCache viewportCache;
                        stage = L"fractional document-Y cold viewport paint";
                        Equal(raster.Paint(firstViewport, &viewportCache, state), raster.Paint(firstViewport, nullptr, state),
                              L"normalized ordinary/link/inline-code cold viewport pixels equal direct drawing");
                        auto viewportBefore = viewportCache.Stats();
                        Require(viewportBefore.entries == 1 && viewportBefore.builds == 1,
                                L"the first viewport really records its complete independent text visual");
                        auto farPixels = raster.Paint(secondViewport, nullptr, state);
                        Require(std::any_of(farPixels.begin(), farPixels.end(), [](uint8_t value) { return value != 255; }),
                                L"the second fractional document-Y viewport contains actual text ink");
                        stage = L"fractional document-Y warm viewport paint";
                        Equal(raster.Paint(secondViewport, &viewportCache, state), farPixels,
                              L"normalized ordinary/link/inline-code warm viewport pixels equal independent direct drawing");
                        Require(viewportCache.Stats().hits == viewportBefore.hits + 1 &&
                                    viewportCache.Stats().builds == viewportBefore.builds,
                                L"integral viewport displacement reuses the exact raster despite different float ULPs");
                        auto const &text = viewportBlocks->front().texts.front();
                        auto selectedPixels = raster.Paint(secondViewport, nullptr, state, static_cast<uint32_t>(text.logicalStart),
                                                           static_cast<uint32_t>(text.logicalEnd));
                        stage = L"fractional document-Y selection bypass paint";
                        Equal(raster.Paint(secondViewport, &viewportCache, state, static_cast<uint32_t>(text.logicalStart),
                                           static_cast<uint32_t>(text.logicalEnd)), selectedPixels,
                              L"fractional-origin selected glyphs and badges preserve the direct paint geometry");
                        Require(viewportCache.Stats().builds == viewportBefore.builds,
                                L"fractional-origin selection never records a dynamic payload");
                        Equal(raster.Paint(secondViewport, &viewportCache, state), farPixels,
                              L"fractional-origin selection cancellation restores clean cached pixels");
                    }

                Require(blocks[4].texts.size() == 1 && blocks[4].texts[0].bitmapCandidate &&
                            !blocks[4].texts[0].inlineCode.empty() && !blocks[4].texts[0].links.empty(),
                        L"the real cached mixed paragraph simultaneously contains inline code, a badge and a link");
                auto const &inlineText = blocks[4].texts[0];
                auto inlineMain = frame->main->Text().substr(inlineText.logicalStart, inlineText.utf16Length);
                auto const &badge = inlineText.inlineCode.front();
                stage = L"inline-code font/badge and supplementary text";
                UINT32 codeFontLength{};
                winrt::check_hresult(inlineText.layout->GetFontFamilyNameLength(badge.start, &codeFontLength, nullptr));
                Require(codeFontLength > 0 && badge.bounds.right > badge.bounds.left && badge.bounds.bottom > badge.bounds.top &&
                            inlineMain.find(L"👨‍👩‍👧‍👦") != std::wstring_view::npos &&
                            inlineMain.find(L"𠮷") != std::wstring_view::npos && inlineMain.find(L"ā́") != std::wstring_view::npos,
                        L"the actual cached code text retains the colored family, supplementary CJK, combining marks, font and badge");
                stage = L"mixed inline-code/link filtered visual";
                auto inlineOnly = *frame;
                auto inlineBlocks = std::make_shared<PreviewVector<PreviewBlockVisual>>(blocks.get_allocator());
                inlineBlocks->push_back(blocks[4]);
                inlineOnly.measuredBlocks = std::move(inlineBlocks);
                NativeTextBitmapCache inlineCache;
                stage = L"inline-code-only direct paint";
                auto inlinePixels = raster.Paint(inlineOnly);
                Require(std::any_of(inlinePixels.begin(), inlinePixels.end(), [](uint8_t value) { return value != 255; }),
                        L"the independent supplementary inline-code glyph and badge oracle paints actual ink");
                stage = L"inline-code-only cold cache paint";
                Equal(raster.Paint(inlineOnly, &inlineCache), inlinePixels,
                      L"inline-code cold command glyphs and independent badge pixels equal direct drawing");
                Require(inlineCache.Stats().entries == 1 && inlineCache.Stats().builds == 1 &&
                            inlineCache.Stats().hits == 0 && inlineCache.Stats().bytes == 0 &&
                            inlineCache.Stats().commandEntries == 1 && inlineCache.Stats().commandTextUnits == inlineText.utf16Length,
                        L"inline code retains exactly one command entry with text units and no claimed bitmap pixel bytes");
                stage = L"inline-code-only warm cache paint";
                Equal(raster.Paint(inlineOnly, &inlineCache), inlinePixels,
                      L"inline-code warm command glyphs and independent badge pixels equal direct drawing");
                Require(inlineCache.Stats().entries == 1 && inlineCache.Stats().builds == 1 && inlineCache.Stats().hits == 1 &&
                            inlineCache.Stats().commandEntries == 1 && inlineCache.Stats().commandTextUnits == inlineText.utf16Length,
                        L"the real inline-code paragraph replays its one command entry without rebuilding the glyphs");

                constexpr std::array inlinePhases{
                    PaintState{144, -0.51f}, PaintState{144, -0.49f},
                    PaintState{144, 0.49f}, PaintState{144, 0.51f}};
                for (auto const &state : inlinePhases)
                {
                    NativeTextBitmapCache phaseCache;
                    stage = L"144 DPI signed-phase inline-code direct paint";
                    auto expected = raster.Paint(inlineOnly, nullptr, state);
                    stage = L"144 DPI signed-phase inline-code cold command paint";
                    Equal(raster.Paint(inlineOnly, &phaseCache, state), expected,
                          L"144 DPI signed half-pixel inline-code command cold pixels equal direct drawing");
                    Require(phaseCache.Stats().builds == 1 && phaseCache.Stats().commandEntries == 1 && phaseCache.Stats().bytes == 0,
                            L"signed-phase inline code actually retains one command payload without a bitmap");
                    auto translated = inlineOnly;
                    translated.viewportY += 4; // Six integral physical pixels at 144 DPI.
                    stage = L"144 DPI translated inline-code direct paint";
                    auto translatedPixels = raster.Paint(translated, nullptr, state);
                    auto phaseBefore = phaseCache.Stats();
                    stage = L"144 DPI translated inline-code warm command paint";
                    Equal(raster.Paint(translated, &phaseCache, state), translatedPixels,
                          L"144 DPI signed-phase integral translation command replay equals direct drawing");
                    Require(phaseCache.Stats().hits == phaseBefore.hits + 1 && phaseCache.Stats().builds == phaseBefore.builds &&
                                phaseCache.Stats().commandEntries == 1 && phaseCache.Stats().commandTextUnits == inlineText.utf16Length,
                            L"integral translation really replays the signed-phase command payload");
                }

                Require(blocks[3].texts.size() == 1 && !blocks[3].texts[0].links.empty() && blocks[3].texts[0].inlineCode.empty(),
                        L"the independent link-only paragraph contains a link without inline code");
                stage = L"link-only filtered visual";
                auto linkOnly = *frame;
                auto linkBlocks = std::make_shared<PreviewVector<PreviewBlockVisual>>(blocks.get_allocator());
                linkBlocks->push_back(blocks[3]);
                linkOnly.measuredBlocks = std::move(linkBlocks);
                NativeTextBitmapCache linkCache;
                stage = L"link-only direct paint";
                auto linkPixels = raster.Paint(linkOnly);
                Require(std::any_of(linkPixels.begin(), linkPixels.end(), [](uint8_t value) { return value != 255; }),
                        L"the link-only oracle actually paints foreground ink");
                stage = L"link-only cold command paint";
                Equal(raster.Paint(linkOnly, &linkCache), linkPixels, L"link-only cold command color pixels equal direct drawing");
                stage = L"link-only warm command paint";
                Equal(raster.Paint(linkOnly, &linkCache), linkPixels, L"link-only warm command color pixels equal direct drawing");
                Require(linkCache.Stats().entries == 1 && linkCache.Stats().builds == 1 && linkCache.Stats().hits == 1 &&
                            linkCache.Stats().commandEntries == 1 && linkCache.Stats().commandTextUnits == blocks[3].texts[0].utf16Length &&
                            linkCache.Stats().bytes == 0,
                        L"the independent link-only text really builds and hits one retained command payload");

                for (bool mixed : {false, true})
                {
                    auto const &only = mixed ? inlineOnly : linkOnly;
                    auto const &text = mixed ? inlineText : blocks[3].texts[0];
                    auto &effectCache = mixed ? inlineCache : linkCache;
                    auto const &normalPixels = mixed ? inlinePixels : linkPixels;
                    stage = mixed ? L"mixed inline-code/link hover/selection" : L"link-only hover/selection";
                    Require(frame->descriptors && !text.links.empty(), L"link effect oracle has real semantic descriptors");
                    auto linkStart = text.logicalStart + text.links.front().start;
                    auto linkEnd = linkStart + text.links.front().length;
                    auto descriptor = std::find_if(frame->descriptors->links.begin(), frame->descriptors->links.end(),
                        [&](auto const &link) { return link.text.start == linkStart && link.text.end == linkEnd; });
                    Require(descriptor != frame->descriptors->links.end(), L"hover targets the exact real link descriptor");
                    PaintState hover;
                    hover.hoveredLink = static_cast<size_t>(descriptor - frame->descriptors->links.begin());
                    auto hoveredPixels = raster.Paint(only, nullptr, hover);
                    Require(hoveredPixels != normalPixels, L"real link hover adds visible underline pixels");
                    auto effectsBefore = effectCache.Stats();
                    Equal(raster.Paint(only, &effectCache, hover), hoveredPixels, L"hover bypass matches direct underline pixels");
                    Require(effectCache.Stats().hits == effectsBefore.hits && effectCache.Stats().builds == effectsBefore.builds,
                            L"hovered link text bypasses command lookup and recording");
                    BOOL underlined{};
                    winrt::check_hresult(text.layout->GetUnderline(text.links.front().start, &underlined, nullptr));
                    Require(!underlined, L"hover drawing leaves no underline on the shared DWrite layout");
                    Equal(raster.Paint(only, &effectCache), normalPixels, L"hover cancellation restores clean original command pixels");
                    Require(effectCache.Stats().hits == effectsBefore.hits + 1 && effectCache.Stats().builds == effectsBefore.builds,
                            L"hover cancellation really hits the normal entry without residual underline pixels");
                    auto selectedPixels = raster.Paint(only, nullptr, {}, static_cast<uint32_t>(text.logicalStart),
                                                        static_cast<uint32_t>(text.logicalEnd));
                    Require(selectedPixels != normalPixels, L"link selection visibly changes foreground/background pixels");
                    effectsBefore = effectCache.Stats();
                    Equal(raster.Paint(only, &effectCache, {}, static_cast<uint32_t>(text.logicalStart),
                                       static_cast<uint32_t>(text.logicalEnd)), selectedPixels,
                          L"selected link/inline-code command bypass matches direct drawing");
                    Require(effectCache.Stats().hits == effectsBefore.hits && effectCache.Stats().builds == effectsBefore.builds,
                            L"selection never records or replays its dynamic effects");
                    Equal(raster.Paint(only, &effectCache), normalPixels, L"selection cancellation restores clean link command pixels");
                    Require(effectCache.Stats().hits == effectsBefore.hits + 1 && effectCache.Stats().builds == effectsBefore.builds,
                            L"selection cancellation really reuses the unselected normal entry");
                }

                auto changedLink = linkOnly;
                // Only link RGBA changes: the theme generation and base foreground stay identical.
                changedLink.theme.linkForeground = {255, 170, 33, 88};
                stage = L"independent linkForeground key direct paint";
                auto changedLinkPixels = raster.Paint(changedLink);
                Require(changedLinkPixels != linkPixels, L"link-only foreground RGBA independently changes actual pixels");
                auto linkBefore = linkCache.Stats();
                stage = L"independent linkForeground key cold command paint";
                Equal(raster.Paint(changedLink, &linkCache), changedLinkPixels, L"changed link RGBA commands equal direct drawing");
                Require(linkCache.Stats().builds == linkBefore.builds + 1 && linkCache.Stats().hits == linkBefore.hits &&
                            linkCache.Stats().commandEntries == linkBefore.commandEntries + 1,
                        L"linkForeground alone creates a distinct command entry without a theme-generation change");
                linkBefore = linkCache.Stats();
                Equal(raster.Paint(changedLink, &linkCache), changedLinkPixels, L"changed link RGBA warm command pixels equal direct drawing");
                Require(linkCache.Stats().hits == linkBefore.hits + 1 && linkCache.Stats().builds == linkBefore.builds,
                        L"the independent link-color key really replays its matching command entry");

                stage = L"selection direct paint";
                auto selected = raster.Paint(*frame, nullptr, {}, 0, frame->main->Length());
                Require(selected != plain, L"the static fixture selection visibly changes the raster");
                before = cache.Stats();
                stage = L"selection cache bypass paint";
                Equal(raster.Paint(*frame, &cache, {}, 0, frame->main->Length()), selected,
                      L"selected static text and table cells preserve direct foreground/background pixels");
                Require(cache.Stats().hits == before.hits && cache.Stats().builds == before.builds,
                        L"selection intersects all eight static visuals and bypasses their retained payloads");
                stage = L"selection cancellation paint";
                Equal(raster.Paint(*frame, &cache), plain, L"cancelling selection restores clean static bitmap pixels");
                Require(cache.Stats().hits == before.hits + staticTexts && cache.Stats().builds == before.builds,
                        L"selection cancellation reuses all eight clean entries");

                PaintState scaled;
                scaled.dpi = 144;
                stage = L"144 DPI direct paint";
                auto scaledPixels = raster.Paint(*frame, nullptr, scaled);
                before = cache.Stats();
                stage = L"144 DPI cold cache paint";
                Equal(raster.Paint(*frame, &cache, scaled), scaledPixels, L"144 DPI static text/inline-code cold pixel oracle");
                Require(cache.Stats().builds == before.builds + staticTexts, L"144 DPI builds eight independent static entries");
                before = cache.Stats();
                stage = L"144 DPI equivalent warm cache paint";
                Equal(raster.Paint(*equivalent, &cache, scaled), scaledPixels, L"144 DPI equivalent static text/inline-code warm pixel oracle");
                Require(cache.Stats().hits == before.hits + staticTexts && cache.Stats().builds == before.builds,
                        L"144 DPI rematerialized static layouts replay the correct rasters");

                cache.Clear();
                PaintState clipped;
                clipped.clip = D2D1::RectF(0, 32, 1024, 512);
                stage = L"partial viewport direct paint";
                auto partial = raster.Paint(*frame, nullptr, clipped);
                Require(partial != plain, L"static partial viewport actually cuts the heading ink");
                before = cache.Stats();
                stage = L"partial viewport cold cache paint";
                Equal(raster.Paint(*frame, &cache, clipped), partial, L"partial-first static text/inline-code cold pixel oracle");
                Require(cache.Stats().builds == before.builds + staticTexts,
                        L"partial-first static draw records every complete eligible visual");
                before = cache.Stats();
                stage = L"partial-first full viewport warm paint";
                Equal(raster.Paint(*equivalent, &cache), plain, L"partial-first static bitmaps retain full heading/table pixels");
                Require(cache.Stats().hits == before.hits + staticTexts && cache.Stats().builds == before.builds,
                        L"full viewport after partial-first recording really replays all eight complete entries");

                cache.Clear();
                PaintState lowerEdge;
                lowerEdge.clip = D2D1::RectF(0, 0, 1024, 60);
                lowerEdge.supplyPaintClip = false;
                stage = L"lower-edge unculled direct paint";
                auto unculledPixels = raster.Paint(*frame, nullptr, lowerEdge);
                Require(unculledPixels != plain && std::any_of(unculledPixels.begin(), unculledPixels.end(),
                            [](uint8_t value) { return value != 255; }),
                        L"small lower-edge viewport contains actual heading ink and excludes later text");
                lowerEdge.supplyPaintClip = true;
                before = cache.Stats();
                stage = L"lower-edge cold culled cache paint";
                Equal(raster.Paint(*frame, &cache, lowerEdge), unculledPixels,
                      L"one-sided paint culling cold pixels match independently unculled direct drawing");
                auto visibleBuilds = cache.Stats().builds - before.builds;
                Require(visibleBuilds > 0 && visibleBuilds < staticTexts && cache.Stats().entries == visibleBuilds,
                        L"small lower edge records only a nonempty strict subset of the eight static text visuals");
                before = cache.Stats();
                stage = L"lower-edge equivalent warm cache paint";
                Equal(raster.Paint(*equivalent, &cache, lowerEdge), unculledPixels,
                      L"one-sided paint culling warm pixels match independently unculled direct drawing");
                Require(cache.Stats().hits == before.hits + visibleBuilds && cache.Stats().builds == before.builds,
                        L"small lower-edge warm draw replays only its genuinely visible static entries");
                before = cache.Stats();
                stage = L"lower-edge recovery full viewport paint";
                Equal(raster.Paint(*equivalent, &cache), plain,
                      L"full viewport after one-sided culling preserves all heading/paragraph/table pixels");
                Require(cache.Stats().entries == staticTexts && cache.Stats().hits == before.hits + visibleBuilds &&
                            cache.Stats().builds == before.builds + staticTexts - visibleBuilds,
                        L"full viewport reuses visible rasters and builds the previously culled complete text visuals");

                cache.Clear();
                auto above = *frame;
                above.viewportY += 1000;
                PaintState upperEdge;
                upperEdge.clip = D2D1::RectF(0, 0, 1024, 512);
                for (size_t index : staticBlockIndices)
                    for (auto const &text : blocks[index].texts)
                        Require(16 + blocks[index].block.y + text.y + text.height - above.viewportY < upperEdge.clip->top,
                                L"all eight static visuals lie nominally above the actual viewport clip");
                upperEdge.supplyPaintClip = false;
                stage = L"upper-edge unculled direct paint";
                auto abovePixels = raster.Paint(above, nullptr, upperEdge);
                upperEdge.supplyPaintClip = true;
                before = cache.Stats();
                stage = L"upper-edge cold admission fallback paint";
                Equal(raster.Paint(above, &cache, upperEdge), abovePixels,
                      L"above-viewport cold fallback matches independently unculled direct pixels");
                stage = L"upper-edge repeated admission fallback paint";
                Equal(raster.Paint(above, &cache, upperEdge), abovePixels,
                      L"above-viewport repeated fallback matches independently unculled direct pixels");
                Require(cache.Stats().builds == before.builds && cache.Stats().hits == before.hits &&
                            cache.Stats().entries == 0,
                        L"nominally above-viewport text neither builds nor replays nor occupies resident bitmap entries");
                before = cache.Stats();
                stage = L"upper-edge recovery full viewport paint";
                Equal(raster.Paint(*equivalent, &cache), plain,
                      L"returning from above-viewport fallback restores complete static pixels");
                Require(cache.Stats().entries == staticTexts && cache.Stats().builds == before.builds + staticTexts &&
                            cache.Stats().hits == before.hits,
                        L"returning full viewport newly builds all eight complete static entries");

                stage = L"heading-only above-viewport lookup setup";
                auto headingOnly = *frame;
                auto headingBlocks = std::make_shared<PreviewVector<PreviewBlockVisual>>(blocks.get_allocator());
                headingBlocks->push_back(blocks[0]);
                headingOnly.measuredBlocks = std::move(headingBlocks);
                const auto headingY = 16 + blocks[0].block.y + blocks[0].texts[0].y;
                Require(headingY == std::floor(headingY), L"heading lookup probe starts at an exact integral origin");
                NativeTextBitmapCache headingCache;
                auto headingPixels = raster.Paint(headingOnly);
                Equal(raster.Paint(headingOnly, &headingCache), headingPixels, L"heading-only seed preserves direct pixels");
                Require(headingCache.Stats().entries == 1 && headingCache.Stats().builds == 1,
                        L"heading-only seed really populates one resident bitmap");
                auto headingAbove = headingOnly;
                headingAbove.viewportY += 1000;
                PaintState headingClip;
                headingClip.clip = D2D1::RectF(0, 0, 1024, 512);
                headingClip.supplyPaintClip = false;
                stage = L"above heading independent direct paint";
                auto headingAbovePixels = raster.Paint(headingAbove, nullptr, headingClip);
                headingClip.supplyPaintClip = true;
                auto headingBefore = headingCache.Stats();
                stage = L"above heading exact-phase cached lookup";
                // Above the nominal clip, exact lookup stays allowed while the
                // renderer forbids new entries and LRU promotion of old ones.
                Equal(raster.Paint(headingAbove, &headingCache, headingClip), headingAbovePixels,
                      L"above-viewport exact-phase heading hit preserves independently clipped direct pixels");
                Require(headingCache.Stats().hits == headingBefore.hits + 1 &&
                            headingCache.Stats().builds == headingBefore.builds && headingCache.Stats().entries == 1,
                        L"nominally above text allows an exact hit without building another entry");

                NativeTextBitmapCache mismatchCache;
                PaintState phaseSeed;
                phaseSeed.phase = 0.49f;
                stage = L"above heading phase-mismatch seed";
                auto phaseSeedPixels = raster.Paint(headingOnly, nullptr, phaseSeed);
                Equal(raster.Paint(headingOnly, &mismatchCache, phaseSeed), phaseSeedPixels,
                      L"the isolated 0.49-phase heading seed preserves direct pixels");
                Require(mismatchCache.Stats().entries == 1 && mismatchCache.Stats().builds == 1,
                        L"phase-mismatch cache contains only the 0.49 heading entry");
                headingClip.phase = 0.51f;
                headingClip.supplyPaintClip = false;
                stage = L"above heading phase-mismatch direct paint";
                auto mismatchPixels = raster.Paint(headingAbove, nullptr, headingClip);
                headingClip.supplyPaintClip = true;
                headingBefore = mismatchCache.Stats();
                stage = L"above heading phase-mismatch fallback";
                Equal(raster.Paint(headingAbove, &mismatchCache, headingClip), mismatchPixels,
                      L"above-viewport phase mismatch preserves the original direct draw");
                Require(mismatchCache.Stats().hits == headingBefore.hits && mismatchCache.Stats().builds == headingBefore.builds &&
                            mismatchCache.Stats().entries == 1,
                        L"an above-viewport 0.51 query neither reuses 0.49 pixels nor builds another entry");
            }
            stage = L"weak owner retirement/Prune";
            Require(weakMain.expired(), L"static bitmap entries retain no Main, layout, table block or model owner");
            raster.Prune(cache);
            Require(cache.Stats().entries == 0 && cache.Stats().bytes == 0 &&
                        cache.Stats().commandEntries == 0 && cache.Stats().commandTextUnits == 0,
                    L"expired static Main owner releases bitmap bytes and command entries/text units on the next Begin");
            return L" WARP direct/cold/equivalent-warm BMP bold/italic/strike paragraph, heading and four table cells; "
                   L"real eight-entry hits including family Emoji/supplementary CJK/combining inline-code/link glyph/badge pixels; "
                   L"link RGBA key, hover/selection bypass and cancellation, 144 DPI, partial-first/full and one-sided cull "
                   L"pixels, fractional document-Y viewport reuse with selection/badges at 96/144/192 DPI, "
                   L"above-viewport admission and weak owner retirement passed. No Lab Emoji fixture changes, public UI or timing claim.";
        }
        catch (winrt::hresult_error const &error)
        {
            throw Failure{std::wstring{L"RunStatic stage="} + stage + L"; HRESULT=" +
                std::to_wstring(error.code().value) + L"; " + std::wstring{error.message()}};
        }
    }

    std::wstring RunImageRelayout(std::shared_ptr<ReadingMemory> const &memory)
    {
        winrt::com_ptr<ID2D1Factory1> factory;
        winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, factory.put()));
        Raster raster{factory.get()};
        NativeTextBitmapCache cache;
        std::weak_ptr<MainTextSnapshot const> weakMain;
        {
            auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
            auto token = NextReadingTextToken();
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                memory, cancel, 23, token);
            reading->PublishMain(MainTextSnapshot::Build(memory, cancel->Get(), 23, token));
            weakMain = reading->Main();
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = reading->Main()->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = 560;
            key.indent = 24;
            key.gap = 12;
            std::atomic<uint64_t> latest{1};
            auto policy = NativeImagePolicy::CreateWindowedV2();
            notepads_markdown_image_epoch epoch{};
            std::copy(std::begin(key.model.document), std::end(key.model.document), std::begin(epoch.source.document));
            epoch.source.revision = key.model.revision;
            epoch.source_generation = reading->sourceGeneration;
            epoch.provider_generation = 1;
            policy->Begin(epoch);
            for (double scale : {1.0, 1.25, 1.5, 2.0})
            {
                PreviewTheme theme;
                theme.physicalScale = scale;
                auto beforeRenderer = NativeDocumentRenderer::Create({}, cancel, key, theme, reading);
                auto before = beforeRenderer->CreateFrame(0, 10000, 1, latest);
                Require(before && before->main == reading->Main(), L"image relayout uses the same current Main owner");
                auto imageIndex = static_cast<size_t>(std::find_if(before->Blocks().begin(), before->Blocks().end(),
                    [](auto const &block)
                    {
                        return std::any_of(block.texts.begin(), block.texts.end(),
                                           [](auto const &text) { return !text.images.empty(); });
                    }) - before->Blocks().begin());
                Require(imageIndex < before->Blocks().size(), L"the unchanged structured fixture contains its real image block");
                auto const &imageBlock = before->Blocks()[imageIndex];
                Require(imageBlock.texts.size() == 1 && imageBlock.texts[0].images.size() == 1,
                        L"the image occupies one independently measured paragraph");
                auto object = imageBlock.texts[0].images[0].object;
                auto tailIndex = static_cast<size_t>(imageIndex + 1);
                while (tailIndex < before->Blocks().size() &&
                       (before->Blocks()[tailIndex].texts.size() != 1 ||
                        !before->Blocks()[tailIndex].texts[0].bitmapCandidate ||
                        !before->Blocks()[tailIndex].texts[0].images.empty()))
                    ++tailIndex;
                Require(tailIndex < before->Blocks().size(), L"the authored image has a following independent static text block");
                auto snapshot = std::make_shared<ImagePresentationSnapshot>(memory);
                snapshot->textGeneration = reading->textGeneration;
                snapshot->policy = policy->SnapshotV2();
                ImagePresentationEntry intrinsic;
                intrinsic.state.object = object;
                intrinsic.state.state = NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED;
                intrinsic.state.status_flags = NOTEPADS_MARKDOWN_IMAGE_V2_HAS_METADATA;
                intrinsic.state.metadata = MakeRasterImageMetadata({1000, 333, 1000, 333, 1, 4000});
                snapshot->entries.push_back(intrinsic);
                auto nextKey = key;
                ++nextKey.layout_generation;
                ++nextKey.measurement_generation;
                auto afterRenderer = NativeDocumentRenderer::Create({}, cancel, nextKey, theme, reading, snapshot);
                auto after = afterRenderer->CreateFrame(0, 10000, 1, latest);
                Require(after && after->main == before->main && after->Blocks().size() == before->Blocks().size(),
                        L"metadata arrival rematerializes the same source and blocks without changing the Main owner");
                auto const &replacement = after->Blocks()[imageIndex];
                auto const &image = replacement.texts[0].images[0];
                const auto imageScale = std::min(1.0, std::max(1.0, replacement.block.width) / 1000.0);
                Require(image.width == static_cast<float>(1000 * imageScale) &&
                            image.height == static_cast<float>(333 * imageScale) && image.baseline == image.height,
                        L"image aspect ratio, actual dimensions and baseline remain the exact intrinsic geometry");
                for (auto const *block : {&imageBlock, &replacement})
                {
                    const auto raw = std::max<double>(theme.lineHeight, block->texts[0].y + block->texts[0].height);
                    const auto padding = (block->measuredHeight - raw) * scale;
                    Require(padding >= -0.000001 && padding < 1.000001 &&
                                std::abs(block->measuredHeight * scale - std::round(block->measuredHeight * scale)) < 0.000001,
                            L"only less than one physical pixel of trailing block space is added");
                }
                const auto displacement = after->Blocks()[tailIndex].block.y - before->Blocks()[tailIndex].block.y;
                Require(displacement > 0 && std::abs(displacement * scale - std::round(displacement * scale)) < 0.000001,
                        L"intrinsic image arrival moves the next block by complete physical pixels");
                auto const start = static_cast<uint32_t>(before->Blocks()[tailIndex].texts[0].logicalStart);
                const auto beforeLine = before->navigation->Line(before->navigation->LineAt(start)).y;
                const auto afterLine = after->navigation->Line(after->navigation->LineAt(start)).y;
                Require(std::abs((afterLine - beforeLine) - displacement) < 0.000001,
                        L"current navigation and selection lines follow the exact submitted block displacement");
                auto isolate = [&](PreviewFrame const &frame)
                {
                    auto result = frame;
                    auto const &blocks = frame.Blocks();
                    auto selected = std::make_shared<PreviewVector<PreviewBlockVisual>>(blocks.get_allocator());
                    selected->push_back(blocks[tailIndex]);
                    result.measuredBlocks = selected;
                    result.viewportY = std::floor(before->Blocks()[tailIndex].block.y * scale) / scale;
                    return result;
                };
                auto oldTail = isolate(*before);
                auto newTail = isolate(*after);
                PaintState state;
                state.dpi = static_cast<float>(96 * scale);
                cache.Clear();
                auto comparePixels = [&](std::vector<uint8_t> const &actual, std::vector<uint8_t> const &expected,
                                         wchar_t const *detail)
                {
                    try
                    {
                        Equal(actual, expected, detail);
                    }
                    catch (Failure const &failure)
                    {
                        throw Failure{failure.detail + L"; scale=" + std::to_wstring(scale)};
                    }
                };
                // Keep the independent oracle and cache construction sequential:
                // argument evaluation order must not choose which D2D draw runs first.
                auto beforePixels = raster.Paint(oldTail, nullptr, state);
                auto cachedBeforePixels = raster.Paint(oldTail, &cache, state);
                comparePixels(cachedBeforePixels, beforePixels,
                              L"before intrinsic metadata the genuine tail glyphs match independent direct paint");
                auto coldStatistics = cache.Stats();
                Require(coldStatistics.entries == 1, L"the before-image tail actually populates one eligible cache entry");
                auto cachedWarmPixels = raster.Paint(oldTail, &cache, state);
                comparePixels(cachedWarmPixels, beforePixels,
                              L"the genuine tail glyphs preserve direct pixels on a second actual cache replay at every DPI");
                Require(cache.Stats().builds == coldStatistics.builds && cache.Stats().hits == coldStatistics.hits + 1,
                        L"the original 125-percent failure remains covered by both cold construction and genuine warm replay");
                auto statistics = cache.Stats();
                auto pixels = raster.Paint(newTail, nullptr, state);
                Require(std::any_of(pixels.begin(), pixels.end(), [](uint8_t value) { return value != 255; }),
                        L"the after-image viewport contains actual tail glyph pixels");
                auto cachedAfterPixels = raster.Paint(newTail, &cache, state);
                comparePixels(cachedAfterPixels, pixels,
                              L"cross-layout tail glyph pixels match current direct painting after image metadata arrives");
                if (scale == 1 || scale == 2)
                    Require(cache.Stats().builds == statistics.builds && cache.Stats().hits == statistics.hits + 1,
                            L"integral-scale image relayout really replays the old eligible text pixels");
                if (scale == 1.25)
                {
                    PaintState impreciseCrop = state;
                    impreciseCrop.phase = 0.49f;
                    NativeTextBitmapCache fallback;
                    auto directFallback = raster.Paint(oldTail, nullptr, impreciseCrop);
                    auto cachedFallback = raster.Paint(oldTail, &fallback, impreciseCrop);
                    comparePixels(cachedFallback, directFallback,
                                  L"125-percent fractional SIS translation preserves direct pixels with an uncropped prefix");
                    Require(fallback.Stats().entries == 1 && fallback.Stats().failures == 0,
                            L"a bounded nonnegative prefix preserves the original fractional transform and retains pixels");
                    auto negative = oldTail;
                    negative.viewportY += 32;
                    NativeTextBitmapCache rejected;
                    auto negativeDirect = raster.Paint(negative, nullptr, impreciseCrop);
                    auto negativeCached = raster.Paint(negative, &rejected, impreciseCrop);
                    comparePixels(negativeCached, negativeDirect,
                                  L"125-percent inexact negative crop keeps the original direct draw");
                    Require(rejected.Stats().entries == 0 && rejected.Stats().failures == 1,
                            L"an inexact negative crop is rejected instead of retaining another raster phase");
                }
                std::weak_ptr<PreviewVector<PreviewBlockVisual> const> weakBlocks = before->measuredBlocks;
                before.reset();
                Require(weakBlocks.expired(), L"cross-layout pixel replay retains no old measured tree");
            }
        }
        Require(weakMain.expired(), L"image-relayout bitmap entries retain no Main or reading owner");
        raster.Prune(cache);
        Require(cache.Stats().entries == 0 && cache.Stats().bytes == 0,
                L"image-relayout owner retirement releases the resident bitmap bytes");
        return L" Intrinsic image geometry at 1/1.25/1.5/2 scale keeps exact image size/baseline, adds less than one "
               L"physical pixel of block tail, moves current navigation by integral physical pixels and preserves direct "
               L"tail glyph pixels; 1x/2x cross-layout cache hits and weak old-tree/Main retirement passed.";
    }
    std::wstring RunScripts(std::shared_ptr<ReadingMemory> const &memory)
    {
        winrt::com_ptr<ID3D11Device> hardware;
        const bool useHardware = SUCCEEDED(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION, hardware.put(), nullptr, nullptr));
        for (float dpi : {96.0f, 144.0f, 192.0f})
        {
            const auto scale = dpi / 96.0;
            winrt::com_ptr<ID2D1Factory1> factory;
            winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, factory.put()));
            Raster raster{factory.get(), useHardware};
            auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
            auto pixelMemory = ReadingMemory::Create(memory->ModelOwner().Clone(), cancel->Get(), memory->Identity());
            const auto token = NextReadingTextToken();
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{pixelMemory},
                pixelMemory, cancel, 23, token);
            reading->PublishMain(MainTextSnapshot::Build(pixelMemory, cancel->Get(), 23, token));
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = reading->Main()->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = 300;
            key.indent = 24;
            key.gap = 12;
            PreviewTheme theme;
            theme.physicalScale = scale;
            theme.fontFamily = L"Segoe UI";
            theme.fontSize = 18;
            theme.lineHeight = 27;
            auto renderer = NativeDocumentRenderer::Create({}, cancel, key, theme, reading);
            std::atomic<uint64_t> latest{1};
            auto source = renderer->CreateFrame(0, 10000, 1, latest);
            Require(source && source->main && source->textPrewarm,
                    L"script pixel oracle owns a real immutable Frame and bounded prewarm handle");
            auto blocks = std::make_shared<PreviewVector<PreviewBlockVisual>>(source->Blocks().get_allocator());
            const auto text = source->main->Text();
            const auto entity = text.find(L"entity á emoji 👩‍💻");
            const auto linked = text.find(L"link-entity 👩‍💻");
            Require(entity != std::wstring_view::npos && linked != std::wstring_view::npos,
                    L"script oracle keeps authored entity/ZWJ and linked-entity sequence locations");
            for (auto const &block : source->Blocks())
                if ((entity >= block.block.logical_start && entity < block.block.logical_end) ||
                    (linked >= block.block.logical_start && linked < block.block.logical_end))
                {
                    blocks->push_back(block);
                    blocks->back().block.y = blocks->size() == 1 ? 0 : 90;
                }
            Require(blocks->size() == 2 && (*blocks)[0].texts[0].scriptDrawing && (*blocks)[1].texts[0].scriptDrawing,
                    L"two actual sealed script layouts cover combining/entity Emoji and a scripted color link");
            auto frame = *source;
            frame.measuredBlocks = blocks;
            PaintState state;
            state.dpi = dpi;
            state.drawPhaseX = static_cast<float>(0.375 / scale);
            state.drawPhaseY = static_cast<float>(0.21875 / scale);
            auto direct = raster.Paint(frame, nullptr, state);
            Require(std::any_of(direct.begin(), direct.end(), [](auto value) { return value != 255; }),
                    L"script direct pixel oracle contains actual visible glyphs");
            NativeTextBitmapCache cache;
            auto clipped = state;
            clipped.clip = D2D1::RectF(0, 22, 400, 40);
            Equal(raster.Paint(frame, &cache, clipped), raster.Paint(frame, nullptr, clipped),
                  L"partial-first script bitmap preserves actual clipped entity/Emoji pixels");
            Equal(raster.Paint(frame, &cache, state), direct,
                  L"full script paint after a partial-first cache never bakes the first viewport clip");
            auto before = cache.Stats();
            const bool cached = dpi != 144;
            Require(before.entries == (cached ? 2u : 0u) && before.builds == (cached ? 2u : 0u) &&
                    before.bytes <= 32u * 1024u * 1024u,
                    L"1x/2x really cache both script bitmaps while fractional DPI keeps direct drawing");
            Equal(raster.Paint(frame, &cache, state), direct, L"script warm bitmap pixels match independent direct drawing");
            Require(cache.Stats().hits == before.hits + (cached ? 2 : 0),
                    L"script warm equality exercises two real bitmap hits");
            auto translated = state;
            translated.drawPhaseY += static_cast<float>(2 / scale);
            before = cache.Stats();
            Equal(raster.Paint(frame, &cache, translated), raster.Paint(frame, nullptr, translated),
                  L"integral physical script translation preserves baselines, Emoji and link pixels");
            Require(cache.Stats().builds == before.builds && cache.Stats().hits == before.hits + (cached ? 2 : 0),
                    L"integral script displacement reuses the exact existing phase instead of rebuilding");
            auto distant = frame;
            auto distantBlocks = std::make_shared<PreviewVector<PreviewBlockVisual>>(*blocks);
            for (auto &block : *distantBlocks) block.block.y += 1048576.0;
            distant.measuredBlocks = distantBlocks;
            distant.viewportY += 1048576.0;
            Equal(raster.Paint(distant, &cache, state), raster.Paint(distant, nullptr, state),
                  L"large integer document-Y changes preserve actual script pixel origins and cached baselines");
            distant = {};
            distantBlocks.reset();
            const auto start = static_cast<uint32_t>(linked + 12);
            const auto end = static_cast<uint32_t>(start + 5);
            before = cache.Stats();
            Equal(raster.Paint(frame, &cache, state, start, end), raster.Paint(frame, nullptr, state, start, end),
                  L"script selection bypass preserves selected foreground and original baseline geometry");
            Require(cache.Stats().builds == before.builds && cache.Stats().hits == before.hits + (cached ? 1 : 0),
                    L"selection bypasses only the intersecting scripted link entry");
            auto hover = state;
            for (size_t i = 0; i < frame.descriptors->links.size(); ++i)
                if (frame.descriptors->links[i].text.start <= start && frame.descriptors->links[i].text.end > start)
                    hover.hoveredLink = i;
            Require(hover.hoveredLink.has_value(), L"scripted link hover names the real canonical action");
            before = cache.Stats();
            Equal(raster.Paint(frame, &cache, hover), raster.Paint(frame, nullptr, hover),
                  L"script hover bypass preserves live underline and color glyph pixels");
            Require(cache.Stats().builds == before.builds && cache.Stats().hits == before.hits + (cached ? 1 : 0),
                    L"hover bypasses the intersecting scripted link bitmap");
            Equal(raster.Paint(frame, &cache, state), direct,
                  L"hover and selection cancellation restore original static pixels without residue");
            // The original Lab 1571/1618 phase ranges differ only by float ULPs
            // as their large document Y is subtracted from a moving viewport.
            auto offGrid = state;
            offGrid.drawPhaseY = static_cast<float>(0.50655364990234375 / scale);
            (*blocks)[1].block.y += (0.4398956298828125 - 0.50655364990234375) / scale;
            const auto offGridPixels = raster.Paint(frame, nullptr, offGrid);
            NativeTextBitmapCache phaseCache;
            Equal(raster.Paint(frame, &phaseCache, offGrid), offGridPixels,
                  L"non-grid Lab script origins use one actual renderer position for cold direct and bitmap paint");
            if (cached)
                for (double jitter : {-1.0 / 65536, 1.0 / 16384})
                {
                    auto micro = offGrid;
                    micro.drawPhaseY += static_cast<float>(jitter / scale);
                    const auto seeded = phaseCache.Stats();
                    const auto microDirect = raster.Paint(frame, nullptr, micro);
                    Equal(microDirect, offGridPixels,
                          L"float-ULP microphases use the same actual direct script position");
                    Equal(raster.Paint(frame, &phaseCache, micro), microDirect,
                          L"float-ULP script microphases retain the same genuine glyph pixels");
                    Require(phaseCache.Stats().builds == seeded.builds && phaseCache.Stats().hits == seeded.hits + 2,
                            L"microphase stability comes from identical actual draw input rather than an approximate cache key");
                }
            Equal(raster.Paint(frame, &phaseCache, offGrid, start, end),
                  raster.Paint(frame, nullptr, offGrid, start, end),
                  L"selected off-grid scripts share the same glyph and paint-rectangle position");
            auto offGridHover = offGrid;
            offGridHover.hoveredLink = hover.hoveredLink;
            Equal(raster.Paint(frame, &phaseCache, offGridHover), raster.Paint(frame, nullptr, offGridHover),
                  L"off-grid script hover underline uses the same actual normalized origin");
            Equal(raster.Paint(frame, &phaseCache, offGrid), offGridPixels,
                  L"off-grid hover and selection cancellation restores the clean cached script origin");
            auto distinct = offGrid;
            distinct.drawPhaseY += static_cast<float>(0.0625 / scale);
            const auto distinctPixels = raster.Paint(frame, nullptr, distinct);
            before = phaseCache.Stats();
            Equal(raster.Paint(frame, &phaseCache, distinct), distinctPixels,
                  L"a genuinely different script phase paints its own direct glyph pixels");
            Require(phaseCache.Stats().builds == before.builds + (cached ? 2 : 0),
                    L"the stability grid does not alias distinct physical script phases");
            state = offGrid;
            direct = offGridPixels;
            if (cached && useHardware)
            {
                // Four real short scripts fill the same bounded nearby queue.
                auto nearbyFrame = frame;
                auto nearbyBlocks = std::make_shared<PreviewVector<PreviewBlockVisual>>(source->Blocks().get_allocator());
                for (auto const &block : source->Blocks())
                    if (block.texts.size() == 1 && block.texts[0].scriptDrawing &&
                        text.substr(static_cast<size_t>(block.block.logical_start),
                                    static_cast<size_t>(block.block.logical_end - block.block.logical_start)).starts_with(L"near-"))
                    {
                        nearbyBlocks->push_back(block);
                        nearbyBlocks->back().block.y = 240 + 40 * (nearbyBlocks->size() - 1);
                    }
                Require(nearbyBlocks->size() == 4, L"four authored independent short script paragraphs fill nearby capacity");
                nearbyFrame.measuredBlocks = nearbyBlocks;
                nearbyFrame.viewportHeight = 200;
                nearbyFrame.textPrewarm = NativeTextBitmapCache::CreatePrewarm();
                NativeTextBitmapCache::BeginPrewarm(nearbyFrame.textPrewarm, key, theme, nearbyFrame.main);
                auto nearState = state;
                nearState.clip = D2D1::RectF(0, 0, 400, 200);
                NativeTextBitmapCache nearbyCache;
                auto nearGate = std::make_shared<NativeTextBitmapCache::PreparationGate>();
                NativeTextBitmapCache::SetPreparationGate(nearbyFrame, nearGate);
                auto nearNotifications = std::make_shared<std::atomic<unsigned>>(0);
                std::function<void()> nearRepaint = [nearNotifications] { ++*nearNotifications; };
                const auto nearDirect = raster.Paint(nearbyFrame, nullptr, nearState);
                Equal(raster.Paint(nearbyFrame, &nearbyCache, nearState, 0, 0, &nearRepaint), nearDirect,
                      L"nearby worker-only requests preserve authoritative clipped pixels without a skeleton");
                Require(nearbyCache.Stats().entries == 0 && nearbyCache.Stats().builds == 0 &&
                            nearbyCache.Stats().prewarmUploads == 0,
                        L"nearby misses do not build, promote or upload UI cache entries");
                const auto nearDeadline = std::chrono::steady_clock::now() + std::chrono::seconds(15);
                while (!nearGate->entered.load() && std::chrono::steady_clock::now() < nearDeadline)
                    std::this_thread::sleep_for(std::chrono::milliseconds(1));
                const bool nearWorkerEntered = nearGate->entered.load();
                // Promote the last queued script while another nearby raster is held.
                (*nearbyBlocks)[3].block.y = 0;
                (void)raster.Paint(nearbyFrame, &nearbyCache, nearState, 0, 0, &nearRepaint);
                nearGate->released = true;
                Require(nearWorkerEntered && nearbyCache.Stats().entries == 0,
                        L"entering the viewport promotes queued script work without synchronous UI rasterization");
                while (NativeTextBitmapCache::PreparedCount(nearbyFrame.textPrewarm) < 4 &&
                       std::chrono::steady_clock::now() < nearDeadline)
                    std::this_thread::sleep_for(std::chrono::milliseconds(1));
                Require(NativeTextBitmapCache::PreparedCount(nearbyFrame.textPrewarm) == 4,
                        L"all four script slots hold actual unconsumed worker pixel payloads");
                (*nearbyBlocks)[3].block.y = 360;
                (void)raster.Paint(nearbyFrame, &nearbyCache, nearState, 0, 0, &nearRepaint);
                Require(NativeTextBitmapCache::PreparedCount(nearbyFrame.textPrewarm) == 4 &&
                            nearbyCache.Stats().prewarmUploads == 0,
                        L"nearby paints never consume ready payloads, including an already promoted visible script");
                auto visibleBlocks = std::make_shared<PreviewVector<PreviewBlockVisual>>(*nearbyBlocks);
                visibleBlocks->push_back((*blocks)[0]);
                visibleBlocks->back().block.y = 0;
                auto visibleFrame = nearbyFrame;
                visibleFrame.measuredBlocks = visibleBlocks;
                auto visibleGate = std::make_shared<NativeTextBitmapCache::PreparationGate>();
                NativeTextBitmapCache::SetPreparationGate(visibleFrame, visibleGate);
                auto visibleNotifications = nearNotifications->load();
                (void)raster.Paint(visibleFrame, &nearbyCache, nearState, 0, 0, &nearRepaint);
                const auto visibleDeadline = std::chrono::steady_clock::now() + std::chrono::seconds(15);
                while (!visibleGate->entered.load() && std::chrono::steady_clock::now() < visibleDeadline)
                {
                    std::this_thread::sleep_for(std::chrono::milliseconds(1));
                    const auto delivered = nearNotifications->load();
                    if (delivered != visibleNotifications)
                    {
                        visibleNotifications = delivered;
                        (void)raster.Paint(visibleFrame, &nearbyCache, nearState, 0, frame.main->Length());
                    }
                }
                const bool visibleWorkerEntered = visibleGate->entered.load();
                visibleGate->released = true;
                Require(visibleWorkerEntered && nearbyCache.Stats().entries == 0 && nearbyCache.Stats().builds == 0,
                        L"a new visible script reclaims nearby-only capacity instead of rasterizing synchronously on UI");
                while (NativeTextBitmapCache::PreparedCount(nearbyFrame.textPrewarm) < 4 &&
                       std::chrono::steady_clock::now() < visibleDeadline)
                    std::this_thread::sleep_for(std::chrono::milliseconds(1));
                Require(NativeTextBitmapCache::PreparedCount(nearbyFrame.textPrewarm) == 4,
                        L"replacement publishes a real visible payload into the original four-slot budget");
                auto shown = std::make_shared<PreviewVector<PreviewBlockVisual>>(source->Blocks().get_allocator());
                shown->push_back(visibleBlocks->back());
                shown->push_back((*nearbyBlocks)[3]);
                shown->back().block.y = 90;
                visibleFrame.measuredBlocks = shown;
                Equal(raster.Paint(visibleFrame, &nearbyCache, nearState, 0, 0, &nearRepaint),
                      raster.Paint(visibleFrame, nullptr, nearState),
                      L"first visible worker uploads preserve the new and protected promoted script's actual glyph pixels");
                Require(nearbyCache.Stats().prewarmUploads == 2 && nearbyCache.Stats().entries == 2,
                        L"both the replacement and protected promoted payload are uploaded, not recreated on UI");
                nearbyCache.Clear();
                nearbyFrame = {};
                visibleFrame = {};
                nearbyBlocks.reset();
                visibleBlocks.reset();
                shown.reset();

                NativeTextBitmapCache asynchronous;
                auto gate = std::make_shared<NativeTextBitmapCache::PreparationGate>();
                NativeTextBitmapCache::SetPreparationGate(frame, gate);
                auto notifications = std::make_shared<std::atomic<unsigned>>(0);
                std::function<void()> repaint = [notifications] { ++*notifications; };
                (void)raster.Paint(frame, &asynchronous, state, 0, 0, &repaint);
                const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(15);
                while (!gate->entered.load() && std::chrono::steady_clock::now() < deadline)
                    std::this_thread::sleep_for(std::chrono::milliseconds(1));
                const auto held = raster.Paint(frame, &asynchronous, state, 0, 0, &repaint);
                auto scrolling = frame;
                scrolling.viewportY += 1 / scale;
                (void)raster.Paint(scrolling, &asynchronous, state, 0, 0, &repaint);
                scrolling = {};
                const bool workerEntered = gate->entered.load();
                gate->released = true;
                Require(workerEntered && asynchronous.Stats().prewarmUploads == 0 && held != direct,
                        L"the first scripted cold draw is pending while the independent worker owns exact pixels");
                while (NativeTextBitmapCache::PreparedCount(frame.textPrewarm) < 2 &&
                       std::chrono::steady_clock::now() < deadline)
                    std::this_thread::sleep_for(std::chrono::milliseconds(1));
                Require(NativeTextBitmapCache::PreparedCount(frame.textPrewarm) == 2 && notifications->load() > 0,
                        L"both scripted pixels arrive and request repaint without another scroll");
                Equal(raster.Paint(frame, &asynchronous, state, 0, 0, &repaint), direct,
                      L"real same-adapter scripted worker uploads match direct target pixels");
                Require(asynchronous.Stats().prewarmUploads == 2 && asynchronous.Stats().entries == 2,
                        L"the first follow-up repaint actually uploads both script payloads");
                const auto uploaded = asynchronous.Stats();
                Equal(raster.Paint(frame, &asynchronous, state), direct,
                      L"after asynchronous upload both scripted glyphs replay without rerasterization");
                Require(asynchronous.Stats().hits == uploaded.hits + 2 && asynchronous.Stats().builds == uploaded.builds,
                        L"scripted worker results become ordinary bounded bitmap hits");
                for (unsigned step = 1; step <= 8; ++step)
                {
                    auto moving = frame;
                    moving.viewportY += step / scale;
                    const auto previous = asynchronous.Stats();
                    Equal(raster.Paint(moving, &asynchronous, state), raster.Paint(moving, nullptr, state),
                          L"successive physical-pixel viewport steps preserve actual scripted glyph and baseline pixels");
                    Require(asynchronous.Stats().builds == previous.builds && asynchronous.Stats().hits == previous.hits + 2,
                            L"continuous viewport steps reuse both worker-produced scripted bitmaps");
                }
                auto phaseNotifications = notifications->load();
                for (auto &block : *blocks) block.block.y += 0.25 / scale;
                (void)raster.Paint(frame, &asynchronous, state, 0, 0, &repaint);
                const auto phaseDeadline = std::chrono::steady_clock::now() + std::chrono::seconds(15);
                while (NativeTextBitmapCache::PreparedCount(frame.textPrewarm) < 2 &&
                       std::chrono::steady_clock::now() < phaseDeadline)
                {
                    std::this_thread::sleep_for(std::chrono::milliseconds(1));
                    // Dispatch only a real worker notification; selection keeps pixels unconsumed.
                    const auto delivered = notifications->load();
                    if (delivered != phaseNotifications)
                    {
                        phaseNotifications = delivered;
                        (void)raster.Paint(frame, &asynchronous, state, 0, frame.main->Length());
                    }
                }
                Require(NativeTextBitmapCache::PreparedCount(frame.textPrewarm) == 2,
                        L"two changed script phases publish pixels before their first upload");
                auto retirement = std::make_shared<NativeTextBitmapCache::PreparationGate>();
                NativeTextBitmapCache::SetPreparationGate(frame, retirement);
                phaseNotifications = notifications->load();
                for (auto &block : *blocks) block.block.y += 0.125 / scale;
                (void)raster.Paint(frame, &asynchronous, state, 0, 0, &repaint);
                const auto retirementDeadline = std::chrono::steady_clock::now() + std::chrono::seconds(15);
                while (!retirement->entered.load() && std::chrono::steady_clock::now() < retirementDeadline)
                {
                    std::this_thread::sleep_for(std::chrono::milliseconds(1));
                    const auto delivered = notifications->load();
                    if (delivered != phaseNotifications)
                    {
                        phaseNotifications = delivered;
                        (void)raster.Paint(frame, &asynchronous, state, 0, frame.main->Length());
                    }
                }
                const bool replacedStalePhase = retirement->entered.load();
                auto retainedStore = frame.textPrewarm;
                std::weak_ptr<MainTextSnapshot const> weakMain = frame.main;
                std::weak_ptr<ReadingMemory> weakMemory = pixelMemory;
                std::weak_ptr<PreviewVector<PreviewBlockVisual>> weakBlocks = blocks;
                asynchronous.Clear();
                frame = {};
                source.reset();
                renderer.reset();
                reading.reset();
                blocks.reset();
                pixelMemory.reset();
                const bool ownersRetired = weakMain.expired() && weakMemory.expired() && weakBlocks.expired();
                retirement->released = true;
                Require(replacedStalePhase,
                        L"a current script phase supersedes stale unuploaded script pixels without blocking all four slots");
                Require(ownersRetired,
                        L"a held script worker retains no Main, measured tree, ReadingMemory or its model pin");
                Require(asynchronous.Stats().bytes == 0 && asynchronous.Stats().entries == 0,
                        L"explicit device/cache retirement releases script pixels and cancels preparation");
                Require(NativeTextBitmapCache::PreparedCount(retainedStore) == 0,
                        L"cancelled script work cannot republish stale pixels after source retirement");
            }
        }
        return useHardware ? L" Script bitmap hardware pixels: 1x/2x cold/hit, full ink after clipping, integral translation, 150% fallback, selection/hover bypass, pending-worker upload and repaint passed."
                           : L" Script bitmap WARP pixels passed; hardware async preparation unavailable.";
    }

}
