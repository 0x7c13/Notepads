// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeDiagramRendererTests.h"
#include "NativeMathCoordinatorTests.h"
#include "Rendering/NativeDocumentRenderer.h"
#include "Resources/Diagrams/NativeDiagramWorker.h"
#include "Resources/Math/NativeMathMemory.h"
#include "Resources/Math/NativeMathWorker.h"
#include <dwrite_2.h>
#include <chrono>
#include <cmath>
#include <cstring>
#include <vector>
#include <thread>

namespace Notepads::Markdown::Native::DiagramRendererTests
{
    namespace
    {
        void Require(bool value, wchar_t const *detail)
        {
            if (!value)
                throw winrt::hresult_error(E_FAIL, detail);
        }
        notepads_markdown_core_error Error() noexcept
        {
            return {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        }
        notepads_markdown_math_usage Usage(notepads_markdown_math_control *control)
        {
            notepads_markdown_math_usage result{};
            result.header = {NOTEPADS_MARKDOWN_MATH_ABI_VERSION, sizeof(result)};
            auto error = Error();
            Require(notepads_markdown_math_control_usage(control, &result, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                    L"diagram renderer real shared usage");
            return result;
        }
        PreviewDiagramVisual const &Visual(PreviewFrame const &frame)
        {
            PreviewDiagramVisual const *found{};
            for (auto const &block : frame.Blocks())
                for (auto const &text : block.texts)
                    for (auto const &value : text.diagrams)
                    {
                        Require(!found, L"dedicated renderer fixture has exactly one real Diagram occurrence");
                        found = &value;
                    }
            Require(found != nullptr, L"real derived Diagram row has a renderer visual");
            return *found;
        }
        PreviewTextVisual const &Text(PreviewFrame const &frame)
        {
            for (auto const &block : frame.Blocks())
                for (auto const &text : block.texts)
                    if (!text.diagrams.empty())
                        return text;
            throw winrt::hresult_error(E_FAIL, L"real Diagram occurrence has its original body layout");
        }
        class InkObserver final : public IDWriteTextRenderer
        {
            std::atomic<ULONG> _references{1};
            UINT32 _offset{};

          public:
            bool placeholderGlyph{};
            uint32_t inlineCalls{};
            std::array<wchar_t, 64> caption{};
            explicit InkObserver(UINT32 offset = UINT32_MAX) noexcept : _offset(offset)
            {
            }
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWritePixelSnapping) &&
                    iid != __uuidof(IDWriteTextRenderer))
                    return E_NOINTERFACE;
                *output = static_cast<IDWriteTextRenderer *>(this);
                AddRef();
                return S_OK;
            }
            ULONG STDMETHODCALLTYPE AddRef() noexcept override
            {
                return ++_references;
            }
            ULONG STDMETHODCALLTYPE Release() noexcept override
            {
                // Stack-owned for the synchronous layout Draw call.
                return --_references;
            }
            HRESULT STDMETHODCALLTYPE IsPixelSnappingDisabled(void *, BOOL *value) noexcept override
            {
                if (!value)
                    return E_POINTER;
                *value = TRUE;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetCurrentTransform(void *, DWRITE_MATRIX *value) noexcept override
            {
                if (!value)
                    return E_POINTER;
                *value = {1, 0, 0, 1, 0, 0};
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetPixelsPerDip(void *, FLOAT *value) noexcept override
            {
                if (!value)
                    return E_POINTER;
                *value = 1;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT, FLOAT, DWRITE_MEASURING_MODE,
                                                   DWRITE_GLYPH_RUN const *,
                                                   DWRITE_GLYPH_RUN_DESCRIPTION const *description,
                                                   IUnknown *) noexcept override
            {
                if (!description)
                    return E_POINTER;
                placeholderGlyph |= _offset >= description->textPosition &&
                                    _offset - description->textPosition < description->stringLength;
                for (UINT32 index = 0; index < description->stringLength; ++index)
                {
                    auto position = uint64_t{description->textPosition} + index;
                    if (position < caption.size())
                        caption[static_cast<size_t>(position)] = description->string[index];
                }
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE DrawUnderline(void *, FLOAT, FLOAT, DWRITE_UNDERLINE const *,
                                                    IUnknown *) noexcept override
            {
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE DrawStrikethrough(void *, FLOAT, FLOAT, DWRITE_STRIKETHROUGH const *,
                                                        IUnknown *) noexcept override
            {
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE DrawInlineObject(void *context, FLOAT x, FLOAT y, IDWriteInlineObject *object,
                                                       BOOL sideways, BOOL rtl, IUnknown *effect) noexcept override
            {
                if (!object)
                    return E_POINTER;
                ++inlineCalls;
                return object->Draw(context, this, x, y, sideways, rtl, effect);
            }
        };
        void Caption(IDWriteTextLayout *label, std::wstring_view expected)
        {
            Require(label && label->GetTextAlignment() == DWRITE_TEXT_ALIGNMENT_LEADING &&
                        label->GetWordWrapping() == DWRITE_WORD_WRAPPING_NO_WRAP &&
                        label->GetParagraphAlignment() == DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
                    L"caption uses its own leading no-wrap format, independent of centered body alignment");
            InkObserver ink;
            winrt::check_hresult(label->Draw(nullptr, &ink, 0, 0));
            Require(expected.size() < ink.caption.size() &&
                        std::wstring_view{ink.caption.data(), expected.size()} == expected &&
                        ink.caption[expected.size()] == 0,
                    L"dedicated caption layout contains the final effective state's literal caption");
        }
        void Fallback(PreviewFrame const &frame, uint32_t start, uint32_t end, std::wstring_view caption)
        {
            auto const &visual = Visual(frame);
            auto const &text = Text(frame);
            Require(visual.hasInline && visual.label && !visual.presentation,
                    L"nonresident fallback always installs a funded zero-ink replacement for the Main object");
            winrt::com_ptr<IDWriteInlineObject> object;
            winrt::check_hresult(text.layout->GetInlineObject(visual.offset, object.put()));
            Require(static_cast<bool>(object), L"DWrite owns an actual inline object at U+FFFC");
            DWRITE_INLINE_OBJECT_METRICS metrics{};
            winrt::check_hresult(object->GetMetrics(&metrics));
            Require(metrics.width == visual.width && metrics.height == visual.height &&
                        metrics.baseline == visual.baseline,
                    L"fallback geometry and its measured line baseline use identical retained metrics");
            InkObserver ink{visual.offset};
            winrt::check_hresult(text.layout->Draw(nullptr, &ink, 0, 0));
            Require(ink.inlineCalls == 1 && !ink.placeholderGlyph,
                    L"ordinary U+FFFC glyph ink never accompanies a fallback caption");
            Caption(visual.label.get(), caption);
            float bodySize{}, captionSize{};
            winrt::check_hresult(text.layout->GetFontSize(visual.offset, &bodySize));
            winrt::check_hresult(visual.label->GetFontSize(0, &captionSize));
            Require(bodySize == captionSize &&
                        visual.label->GetMaxWidth() <= (std::max)(1.0f, visual.width - 12),
                    L"caption captures the actual object font size and bounded available width");
            std::array<wchar_t, 128> bodyFamily{}, captionFamily{};
            winrt::check_hresult(text.layout->GetFontFamilyName(visual.offset, bodyFamily.data(),
                                                               static_cast<UINT32>(bodyFamily.size())));
            winrt::check_hresult(visual.label->GetFontFamilyName(0, captionFamily.data(),
                                                                static_cast<UINT32>(captionFamily.size())));
            Require(std::wstring_view{bodyFamily.data()} == std::wstring_view{captionFamily.data()},
                    L"caption retains the captured theme/code family of the original object");
            auto rectangles = NativeDocumentRenderer::Rectangles(frame, start, end);
            Require(rectangles.size() == 1 && std::abs(rectangles[0].width - visual.width) < 0.01f &&
                        std::abs(rectangles[0].height - visual.height) < 0.01f,
                    L"fallback atomic selection reserves the complete caption rectangle");
            auto hit = NativeDocumentRenderer::HitTest(frame, rectangles[0].x + rectangles[0].width / 2,
                                                       rectangles[0].y + rectangles[0].height / 2);
            Require(hit && hit->inside && hit->cluster.start == start && hit->cluster.end == end,
                    L"fallback center remains exactly the original atomic Main object");
        }
        struct Completion
        {
            winrt::handle done{CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET, EVENT_ALL_ACCESS)};
            NativeDiagramWorker::PreparedNode *node{};
            DiagramPresentationHandle payload;
            int32_t status{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
            bool started{};
            ~Completion()
            {
                if (started)
                    (void)WaitForSingleObjectEx(done.get(), INFINITE, FALSE);
                if (node)
                    NativeDiagramWorker::ReleasePrepared(node);
            }
            static void Complete(void *context, notepads_markdown_diagram_job *, NativeDiagramWorkerResult const &result) noexcept
            {
                auto node = static_cast<NativeDiagramWorker::PreparedNode *>(context);
                auto self = static_cast<Completion *>(NativeDiagramWorker::PreparedOwner(node));
                self->status = result.status;
                self->payload = NativeDiagramWorker::BuildPresentationAfterThreadExit(node, self->status);
                SetEvent(self->done.get());
            }
        };
        struct Fonts
        {
            winrt::com_ptr<IDWriteFactory> factory;
            winrt::com_ptr<IDWriteFontCollection> collection;
            winrt::com_ptr<IDWriteFontFallback> fallback;
            notepads_markdown_diagram_font_capability capability{};
            Fonts()
            {
                winrt::check_hresult(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory),
                                                         reinterpret_cast<IUnknown **>(factory.put())));
                winrt::check_hresult(factory->GetSystemFontCollection(collection.put(), FALSE));
                winrt::com_ptr<IDWriteFontFallbackBuilder> builder;
                winrt::check_hresult(factory.as<IDWriteFactory2>()->CreateFontFallbackBuilder(builder.put()));
                wchar_t const *names[]{L"Segoe UI", L"Microsoft YaHei UI", L"Segoe UI Symbol", L"Segoe UI Emoji"};
                DWRITE_UNICODE_RANGE range{0, 0x10ffff};
                winrt::check_hresult(builder->AddMapping(&range, 1, names, 4, collection.get(), nullptr, nullptr, 1));
                winrt::check_hresult(builder->CreateFontFallback(fallback.put()));
                capability.version = 1;
                capability.size = sizeof(capability);
                capability.factory = factory.get();
                capability.collection = collection.get();
                capability.fallback = fallback.get();
                capability.font_epoch = 1;
                std::copy_n(L"en-US", 6, capability.locale_name);
            }
        };
        // Exercise actual SVG resources through independent WARP contexts. A fresh
        // cache on each destination is the pixel oracle; no timing threshold is used.
        void CheckDeviceReuse(ID2D1Factory1 *factory, IDXGIDevice *dxgi, ID2D1Device *device,
                              DiagramPresentationHandle const &payload)
        {
            Require(factory && dxgi && device && payload && payload->State() == NativeDiagramState::Ready,
                    L"device reuse starts from the actual Ready SVG artifact");
            winrt::com_ptr<ID2D1DeviceContext> a, b;
            winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, a.put()));
            winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, b.put()));
            Require(a.as<IUnknown>().get() != b.as<IUnknown>().get(), L"same-device contexts are actually distinct");
            constexpr UINT32 pixelWidth = 384, pixelHeight = 320;
            struct State
            {
                float dpi;
                float translateX, translateY;
                bool inspection;
            };
            constexpr State ordinary{96, 0, 0, false};
            constexpr State fractional{144, 0.375f, 0.625f, false};
            constexpr State inspection{192, 0.375f, 0.625f, true};
            auto paint = [&](ID2D1DeviceContext *context, DiagramDrawCache &cache, State state,
                             HRESULT expected = S_OK)
            {
                const auto scale = state.dpi / 96.0f;
                const auto transform = D2D1::Matrix3x2F::Translation(state.translateX, state.translateY);
                const auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_TARGET,
                    D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), state.dpi, state.dpi);
                winrt::com_ptr<ID2D1Bitmap1> target;
                winrt::check_hresult(context->CreateBitmap(D2D1::SizeU(pixelWidth, pixelHeight), nullptr, 0,
                                                          properties, target.put()));
                context->SetDpi(state.dpi, state.dpi);
                context->SetTransform(transform);
                context->SetUnitMode(D2D1_UNIT_MODE_DIPS);
                context->SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                DiagramPlacement placement;
                const auto viewport = D2D1::RectF(16, 20, 168, 120);
                auto left = 12.25f, top = 10.5f;
                if (state.inspection)
                {
                    DiagramInspectionPlacement inspected;
                    Require(NativeDiagramDrawing::Inspect(payload, viewport, {2.0f, 0.25f, 0.75f}, inspected),
                            L"cross-context inspection has a real bounded placement");
                    placement = inspected.diagram;
                    left = inspected.origin.x;
                    top = inspected.origin.y;
                }
                else
                    Require(NativeDiagramDrawing::Place(*payload.operator->(), (std::min)(260.0f,
                                pixelWidth / scale - 32.0f), placement) == NOTEPADS_MARKDOWN_CORE_OK,
                            L"cross-context ordinary paint has a real fitted placement");
                context->SetTarget(target.get());
                context->BeginDraw();
                context->Clear(D2D1::ColorF(0, 0, 0, 0));
                const auto actual = NativeDiagramDrawing::Draw(payload, placement, context, left, top, cache,
                                                               state.inspection ? &viewport : nullptr);
                D2D1_MATRIX_3X2_F after{};
                context->GetTransform(&after);
                const auto ended = context->EndDraw();
                context->SetTarget(nullptr);
                winrt::check_hresult(ended);
                Require(actual == expected, L"cross-context SVG returns the expected immediate status and successful EndDraw");
                Require(after._11 == transform._11 && after._12 == transform._12 && after._21 == transform._21 &&
                            after._22 == transform._22 && after._31 == transform._31 && after._32 == transform._32,
                        L"cross-context SVG paint restores the caller's exact transform");
                const auto readable = D2D1::BitmapProperties1(
                    D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                    D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), state.dpi, state.dpi);
                winrt::com_ptr<ID2D1Bitmap1> readback;
                winrt::check_hresult(context->CreateBitmap(D2D1::SizeU(pixelWidth, pixelHeight), nullptr, 0,
                                                          readable, readback.put()));
                winrt::check_hresult(readback->CopyFromBitmap(nullptr, target.get(), nullptr));
                std::vector<uint8_t> pixels(static_cast<size_t>(pixelWidth) * pixelHeight * 4);
                D2D1_MAPPED_RECT mapped{};
                winrt::check_hresult(readback->Map(D2D1_MAP_OPTIONS_READ, &mapped));
                for (UINT32 row = 0; row < pixelHeight; ++row)
                    std::memcpy(pixels.data() + static_cast<size_t>(row) * pixelWidth * 4,
                                mapped.bits + static_cast<size_t>(row) * mapped.pitch, pixelWidth * 4);
                winrt::check_hresult(readback->Unmap());
                uint64_t ink{};
                for (UINT32 y = 0; y < pixelHeight; ++y)
                    for (UINT32 x = 0; x < pixelWidth; ++x)
                    {
                        const auto alpha = pixels[(static_cast<size_t>(y) * pixelWidth + x) * 4 + 3];
                        ink += alpha != 0;
                        if (state.inspection && alpha)
                        {
                            // Conservatively include both transformed boundary pixels.
                            const auto minX = std::floor((viewport.left + state.translateX) * scale);
                            const auto minY = std::floor((viewport.top + state.translateY) * scale);
                            const auto maxX = std::ceil((viewport.right + state.translateX) * scale);
                            const auto maxY = std::ceil((viewport.bottom + state.translateY) * scale);
                            Require(x >= minX && x < maxX && y >= minY && y < maxY,
                                    L"cross-context inspection SVG pixels remain inside the transformed clip");
                        }
                    }
                if (SUCCEEDED(expected)) Require(ink > 20, L"pixel comparisons contain real SVG ink");
                return pixels;
            };
            auto equalFresh = [&](ID2D1DeviceContext *context, std::vector<uint8_t> const &actual, State state)
            {
                DiagramDrawCache fresh;
                Require(actual == paint(context, fresh, state),
                        L"same-device retained SVG pixels equal a freshly parsed SVG on the destination context");
            };
            DiagramDrawCache cache;
            const auto first = paint(a.get(), cache, ordinary);
            auto originalDocument = cache.document;
            auto originalStream = cache.stream;
            Require(originalDocument && originalStream, L"first context creates both real SVG resources");
            const auto second = paint(b.get(), cache, ordinary);
            Require(cache.document.get() == originalDocument.get() && cache.stream.get() == originalStream.get(),
                    L"A to B on the same D2D device preserves the exact SVG document and stream");
            Require(first == second, L"equivalent independent contexts paint identical actual SVG pixels");
            Require(cache.context.get() == b.as<IUnknown>().get(),
                    L"successful cross-context reuse records the last actual context for later negative-cache scope");
            equalFresh(b.get(), second, ordinary);
            equalFresh(a.get(), paint(a.get(), cache, fractional), fractional);
            equalFresh(b.get(), paint(b.get(), cache, inspection), inspection);
            Require(paint(a.get(), cache, ordinary) == first && cache.document.get() == originalDocument.get() &&
                        cache.stream.get() == originalStream.get(),
                    L"A to B to A, DPI changes and inspection preserve one SVG without baking destination state");

            // The same D3D/DXGI device still permits distinct D2D resource domains.
            winrt::com_ptr<ID2D1Device> replacement;
            winrt::check_hresult(factory->CreateDevice(dxgi, replacement.put()));
            winrt::com_ptr<IUnknown> originalDevice;
            winrt::check_hresult(device->QueryInterface(__uuidof(IUnknown), originalDevice.put_void()));
            Require(replacement.as<IUnknown>().get() != originalDevice.get(),
                    L"replacement D2D resource domain is distinct despite the same D3D device");
            winrt::com_ptr<ID2D1DeviceContext> c, d;
            winrt::check_hresult(replacement->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, c.put()));
            winrt::check_hresult(replacement->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, d.put()));
            const auto changed = paint(c.get(), cache, ordinary);
            Require(cache.document.get() != originalDocument.get() && cache.stream.get() != originalStream.get(),
                    L"a different D2D device recreates resources even when the D3D device and payload are unchanged");
            equalFresh(c.get(), changed, ordinary);
            originalDocument = nullptr;
            originalStream = nullptr;

            // Synthesize the existing negative creation-cache state for C. This
            // tests retry scope, not an actual out-of-memory condition in Direct2D.
            cache.document = nullptr;
            cache.stream = nullptr;
            cache.context = c.as<IUnknown>();
            cache.status.store(E_OUTOFMEMORY, std::memory_order_release);
            (void)paint(c.get(), cache, ordinary, E_OUTOFMEMORY);
            Require(!cache.document && !cache.stream, L"the failed creation remains cached on its original context");
            const auto recovered = paint(d.get(), cache, ordinary);
            Require(cache.document && cache.stream && SUCCEEDED(cache.status.load(std::memory_order_acquire)),
                    L"a different context on the same D2D device retries a cached creation failure");
            equalFresh(d.get(), recovered, ordinary);
        }

        void CheckFrameHandoff(PreviewFrame const &frame, ID2D1DeviceContext *context,
                               DiagramPresentationHandle const &otherOwner)
        {
            auto const &original = Visual(frame);
            Require(context && original.presentation && otherOwner &&
                        original.presentation.operator->() != otherOwner.operator->() &&
                        original.presentation->Key() == otherOwner->Key(),
                    L"handoff tests use two distinct actual Ready owners with the same captured key");
            struct Copy
            {
                std::shared_ptr<PreviewFrame> frame;
                std::shared_ptr<PreviewVector<PreviewBlockVisual>> blocks;
                PreviewDiagramVisual *visual{};
            };
            auto fresh = [&]
            {
                Copy copy{std::make_shared<PreviewFrame>(frame),
                    std::make_shared<PreviewVector<PreviewBlockVisual>>(frame.Blocks().get_allocator())};
                copy.blocks->assign(frame.Blocks().begin(), frame.Blocks().end());
                copy.frame->measuredBlocks = copy.blocks;
                for (auto &block : *copy.blocks)
                    for (auto &text : block.texts)
                        for (auto &diagram : text.diagrams)
                            if (diagram.object == original.object) copy.visual = &diagram;
                Require(copy.visual && copy.frame->diagram.presentations.count &&
                            copy.frame->diagram.presentations.count == frame.diagram.presentations.count &&
                            copy.frame->measuredBlocks != frame.measuredBlocks,
                        L"fresh measured blocks preserve the real nonempty presentation snapshot");
                auto const &cache = copy.visual->drawing;
                Require(!cache.owner && !cache.context && !cache.document && !cache.stream &&
                            cache.status.load(std::memory_order_acquire) == S_OK,
                        L"copying measured visuals starts with no UI SVG references or negative cache");
                return copy;
            };
            constexpr UINT32 width = 1024, height = 1600;
            auto paint = [&](PreviewFrame const &value)
            {
                const auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_TARGET,
                    D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96);
                winrt::com_ptr<ID2D1Bitmap1> target;
                winrt::check_hresult(context->CreateBitmap(D2D1::SizeU(width, height), nullptr, 0,
                                                          properties, target.put()));
                context->SetDpi(96, 96);
                context->SetTransform(D2D1::Matrix3x2F::Identity());
                context->SetUnitMode(D2D1_UNIT_MODE_DIPS);
                context->SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                context->SetTarget(target.get());
                context->BeginDraw();
                context->Clear(D2D1::ColorF(0, 0, 0, 0));
                try { NativeDocumentRenderer::Draw(value, context, 4, 4); }
                catch (...) { (void)context->EndDraw(); context->SetTarget(nullptr); throw; }
                const auto ended = context->EndDraw();
                context->SetTarget(nullptr);
                winrt::check_hresult(ended);
                const auto readable = D2D1::BitmapProperties1(
                    D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                    D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96);
                winrt::com_ptr<ID2D1Bitmap1> readback;
                winrt::check_hresult(context->CreateBitmap(D2D1::SizeU(width, height), nullptr, 0,
                                                          readable, readback.put()));
                winrt::check_hresult(readback->CopyFromBitmap(nullptr, target.get(), nullptr));
                std::vector<uint8_t> pixels(static_cast<size_t>(width) * height * 4);
                D2D1_MAPPED_RECT mapped{};
                winrt::check_hresult(readback->Map(D2D1_MAP_OPTIONS_READ, &mapped));
                for (UINT32 row = 0; row < height; ++row)
                    std::memcpy(pixels.data() + static_cast<size_t>(row) * width * 4,
                                mapped.bits + static_cast<size_t>(row) * mapped.pitch, width * 4);
                winrt::check_hresult(readback->Unmap());
                uint64_t ink{};
                for (size_t index = 3; index < pixels.size(); index += 4) ink += pixels[index] != 0;
                Require(ink > 20, L"frame handoff pixel comparisons contain actual SVG pixels");
                return pixels;
            };
            auto previous = fresh();
            const auto expected = paint(*previous.frame);
            auto document = previous.visual->drawing.document;
            auto stream = previous.visual->drawing.stream;
            Require(document && stream, L"handoff donor is populated through actual NativeDocumentRenderer Draw");
            auto next = fresh();
            const auto ordinal = previous.visual->ordinal;
            Require(ordinal < previous.frame->diagramDrawOutcomes.size() && next.visual->ordinal == ordinal,
                    L"handoff fixture uses the actual bounded object ordinal");
            const auto oldOutcome = previous.frame->diagramDrawOutcomes[ordinal].status.load(std::memory_order_acquire);
            const auto newOutcome = next.frame->diagramDrawOutcomes[ordinal].status.load(std::memory_order_acquire);
            Require(oldOutcome == S_OK && newOutcome == S_FALSE, L"new measured frame has no borrowed draw outcome");
            next.frame->TakeDiagramDrawingFrom(*previous.frame);
            Require(next.visual->drawing.document.get() == document.get() && next.visual->drawing.stream.get() == stream.get() &&
                        !previous.visual->drawing.owner && !previous.visual->drawing.context &&
                        !previous.visual->drawing.document && !previous.visual->drawing.stream,
                    L"handoff moves the exact successful SVG references out of the previous measured visuals");
            Require(previous.frame->diagramDrawOutcomes[ordinal].status.load(std::memory_order_acquire) == oldOutcome &&
                        next.frame->diagramDrawOutcomes[ordinal].status.load(std::memory_order_acquire) == newOutcome &&
                        previous.frame->DiagramState(original.object) == NativeDiagramState::Ready,
                    L"cache handoff preserves both independent Frame outcomes and the old published object state");
            Require(paint(*next.frame) == expected && next.visual->drawing.document.get() == document.get() &&
                        next.visual->drawing.stream.get() == stream.get(),
                    L"the next measured frame really reuses the moved SVG on the same context with identical pixels");
            auto independentlyParsed = fresh();
            Require(paint(*independentlyParsed.frame) == expected &&
                        independentlyParsed.visual->drawing.document.get() != document.get(),
                    L"handoff pixels equal an independently parsed cache on the same actual context");
            Require(paint(*previous.frame) == expected && previous.visual->drawing.document &&
                        previous.visual->drawing.document.get() != document.get() &&
                        next.visual->drawing.document.get() == document.get() && next.visual->drawing.stream.get() == stream.get(),
                    L"the old frame can recreate its moved cache and repaint identical pixels without taking back the next frame's resources");
            auto shared = next.frame->Reposition(next.frame->viewportY, next.frame->viewportHeight);
            Require(shared && shared->measuredBlocks == next.frame->measuredBlocks,
                    L"the no-op handoff counterexample actually shares the same measured container");
            next.frame->TakeDiagramDrawingFrom(*next.frame);
            shared->TakeDiagramDrawingFrom(*next.frame);
            Require(next.visual->drawing.document.get() == document.get() && next.visual->drawing.stream.get() == stream.get() &&
                        shared->diagramDrawOutcomes[ordinal].status.load(std::memory_order_acquire) == S_FALSE &&
                        next.frame->diagramDrawOutcomes[ordinal].status.load(std::memory_order_acquire) == S_OK,
                    L"self and reposition handoff cannot self-move references or transfer draw outcomes");

            auto differentOwner = fresh();
            differentOwner.visual->presentation = otherOwner;
            differentOwner.frame->TakeDiagramDrawingFrom(*next.frame);
            Require(!differentOwner.visual->drawing.document && !differentOwner.visual->drawing.stream &&
                        next.visual->drawing.document.get() == document.get() && next.visual->drawing.stream.get() == stream.get(),
                    L"equal captured keys with distinct actual destination owners cannot borrow SVG references");
            auto failedDestination = fresh();
            failedDestination.visual->drawing.status.store(E_OUTOFMEMORY, std::memory_order_release);
            failedDestination.frame->TakeDiagramDrawingFrom(*next.frame);
            Require(failedDestination.visual->drawing.status.load(std::memory_order_acquire) == E_OUTOFMEMORY &&
                        !failedDestination.visual->drawing.document && next.visual->drawing.document.get() == document.get(),
                    L"handoff preserves a negative destination cache instead of replacing its fresh-frame retry");
            Require(paint(*failedDestination.frame) == expected &&
                        failedDestination.frame->DiagramState(original.object) == NativeDiagramState::Ready &&
                        failedDestination.visual->drawing.document.get() != document.get(),
                    L"the destination's existing fresh-frame retry recreates its own cache and exact pixels after handoff refused it");
            auto refusesFailure = fresh();
            const auto oldFailureDocument = previous.visual->drawing.document;
            previous.visual->drawing.status.store(E_OUTOFMEMORY, std::memory_order_release);
            refusesFailure.frame->TakeDiagramDrawingFrom(*previous.frame);
            Require(!refusesFailure.visual->drawing.document && !refusesFailure.visual->drawing.stream &&
                        previous.visual->drawing.document.get() == oldFailureDocument.get() &&
                        previous.visual->drawing.status.load(std::memory_order_acquire) == E_OUTOFMEMORY &&
                        previous.frame->diagramDrawOutcomes[ordinal].status.load(std::memory_order_acquire) == S_OK &&
                        refusesFailure.frame->diagramDrawOutcomes[ordinal].status.load(std::memory_order_acquire) == S_FALSE,
                    L"an injected negative donor cache is never moved and neither frame's published outcome is rewritten");
        }

        uint64_t DrawPixels(PreviewFrame const &frame, uint32_t start, uint32_t end, bool fallback = false,
                            bool rejectSvgContext = false, DiagramPresentationHandle const *handoffOther = nullptr)
        {
            winrt::com_ptr<ID2D1Factory1> factory;
            winrt::check_hresult(D2D1CreateFactory(rejectSvgContext ? D2D1_FACTORY_TYPE_SINGLE_THREADED
                                                                 : D2D1_FACTORY_TYPE_MULTI_THREADED,
                                                   factory.put()));
            winrt::com_ptr<ID3D11Device> d3d;
            winrt::com_ptr<ID3D11DeviceContext> immediate;
            D3D_FEATURE_LEVEL level{};
            winrt::check_hresult(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr,
                                                   D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
                                                   d3d.put(), &level, immediate.put()));
            auto dxgi = d3d.as<IDXGIDevice>();
            winrt::com_ptr<ID2D1Device> device;
            winrt::check_hresult(factory->CreateDevice(dxgi.get(), device.put()));
            winrt::com_ptr<ID2D1DeviceContext> context;
            winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, context.put()));
            context->SetDpi(96, 96);
            constexpr UINT32 width = 1024, height = 1600;
            auto properties = D2D1::BitmapProperties1(
                D2D1_BITMAP_OPTIONS_TARGET,
                D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96);
            winrt::com_ptr<ID2D1Bitmap1> target;
            winrt::check_hresult(
                context->CreateBitmap(D2D1::SizeU(width, height), nullptr, 0, properties, target.put()));
            context->SetTarget(target.get());
            context->BeginDraw();
            context->Clear(D2D1::ColorF(0, 0, 0, 0));
            NativeDocumentRenderer::Draw(frame, context.get(), 4, 4);
            winrt::check_hresult(context->EndDraw());
            auto const &visual = Visual(frame);
            if (!fallback && !rejectSvgContext)
                Require(frame.DiagramState(visual.object) == NativeDiagramState::Ready && visual.drawing.document &&
                            visual.drawing.stream,
                        L"WARP consumes the actual canonical Node through a retained no-copy read stream");
            else if (rejectSvgContext)
            {
                Require(visual.state == NativeDiagramState::Ready &&
                            frame.DiagramState(visual.object) == NativeDiagramState::Failed &&
                            FAILED(visual.drawing.status.load(std::memory_order_acquire)),
                        L"Ready -> Draw failure selects an effective Failed state on an unsupported SVG context");
                Caption(visual.drawFailureLabels[0].get(), L"Diagram failed");
            }
            else
                Require(frame.DiagramState(visual.object) != NativeDiagramState::Ready && visual.label,
                        L"WARP paints the actual nonresident caption in selected and unselected passes");
            auto document = visual.drawing.document.get();
            auto stream = visual.drawing.stream.get();
            auto caption = visual.label.get();
            auto failedCaption = visual.drawFailureLabels[0].get();
            context->BeginDraw();
            NativeDocumentRenderer::Draw(frame, context.get(), 4, 4, start, end);
            winrt::check_hresult(context->EndDraw());
            Require(document == visual.drawing.document.get() && stream == visual.drawing.stream.get(),
                    L"same Frame/context/device reuses one real SVG document and one read cursor");
            Require(caption == visual.label.get() && failedCaption == visual.drawFailureLabels[0].get() &&
                        (!caption || caption->GetTextAlignment() == DWRITE_TEXT_ALIGNMENT_LEADING),
                    L"paint reuses worker-measured captions without mutating body or caption alignment");
            context->SetTarget(nullptr);
            properties.bitmapOptions = D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
            winrt::com_ptr<ID2D1Bitmap1> readback;
            winrt::check_hresult(
                context->CreateBitmap(D2D1::SizeU(width, height), nullptr, 0, properties, readback.put()));
            winrt::check_hresult(readback->CopyFromBitmap(nullptr, target.get(), nullptr));
            D2D1_MAPPED_RECT mapped{};
            winrt::check_hresult(readback->Map(D2D1_MAP_OPTIONS_READ, &mapped));
            uint64_t colored{};
            auto boxes = NativeDocumentRenderer::Rectangles(frame, start, end);
            for (auto const &box : boxes)
            {
                auto left = static_cast<UINT32>((std::clamp)(box.x + 5, 0.0f, static_cast<float>(width)));
                auto top = static_cast<UINT32>((std::clamp)(box.y + 5, 0.0f, static_cast<float>(height)));
                auto right = static_cast<UINT32>((std::clamp)(box.x + box.width + 3, 0.0f, static_cast<float>(width)));
                auto bottom =
                    static_cast<UINT32>((std::clamp)(box.y + box.height + 3, 0.0f, static_cast<float>(height)));
                for (auto y = top; y < bottom; ++y)
                    for (auto x = left; x < right; ++x)
                    {
                        auto pixel = mapped.bits + static_cast<size_t>(y) * mapped.pitch + static_cast<size_t>(x) * 4;
                        if (pixel[3] && (pixel[0] < 200 || pixel[1] < 200 || pixel[2] < 200))
                            ++colored;
                    }
                if (fallback || rejectSvgContext)
                {
                    // The dedicated fixture Main consists only of this one
                    // object. Any ink beside its interior rows is overflow.
                    for (auto y = top + 2; y + 2 < bottom; ++y)
                        for (UINT32 x = 0; x < width; ++x)
                        {
                            if (static_cast<float>(x) >= box.x + 2 &&
                                static_cast<float>(x) <= box.x + box.width + 6)
                                continue;
                            auto pixel = mapped.bits + static_cast<size_t>(y) * mapped.pitch +
                                         static_cast<size_t>(x) * 4;
                            Require(pixel[3] == 0,
                                    L"narrow fallback and Draw-failure caption ink stays inside its atomic rectangle");
                        }
                }
            }
            winrt::check_hresult(readback->Unmap());
            Require(colored > 20,
                    fallback || rejectSvgContext
                        ? L"actual fallback rendering produces selected pixels within the atomic caption rectangle"
                        : L"actual SVG foreground/shape pixels exist inside the atomic object, not merely its page");
            if (!fallback && !rejectSvgContext)
            {
                auto moved = frame.Reposition(1, frame.viewportHeight);
                Require(moved && moved->viewportY == 1 && moved->measuredBlocks == frame.measuredBlocks &&
                            !frame.Reposition(frame.totalHeight + frame.theme.padding, frame.viewportHeight),
                        L"Ready diagram repositions only within measured coverage and shares layouts");
                context->SetTarget(target.get());
                context->BeginDraw();
                NativeDocumentRenderer::Draw(*moved, context.get(), 4, 4);
                winrt::check_hresult(context->EndDraw());
                Require(document == visual.drawing.document.get() && stream == visual.drawing.stream.get(),
                        L"repositioned diagram reuses the SVG document and stream on the same context");

                // Inspect the same SVG/stream through two different placements on
                // the real WARP target. The rest of the target must remain clear.
                auto const viewport = D2D1::RectF(36, 44, 190, 204);
                auto const bytes = visual.presentation->Bytes();
                auto const byteCount = visual.presentation->ByteCount();
                auto renderInspection = [&](DiagramInspectionTransform transform)
                {
                    DiagramInspectionPlacement placement;
                    Require(NativeDiagramDrawing::Inspect(visual.presentation, viewport, transform, placement),
                            L"Ready diagram admits a bounded inspection placement");
                    context->SetTarget(target.get());
                    context->BeginDraw();
                    context->Clear(D2D1::ColorF(0, 0, 0, 0));
                    winrt::check_hresult(NativeDiagramDrawing::Draw(visual.presentation, placement.diagram,
                        context.get(), placement.origin.x, placement.origin.y, visual.drawing, &placement.clip));
                    winrt::check_hresult(context->EndDraw());
                    context->SetTarget(nullptr);
                    winrt::check_hresult(readback->CopyFromBitmap(nullptr, target.get(), nullptr));
                    D2D1_MAPPED_RECT inspectionPixels{};
                    winrt::check_hresult(readback->Map(D2D1_MAP_OPTIONS_READ, &inspectionPixels));
                    uint64_t fingerprint{}, inside{};
                    bool outside{};
                    for (UINT32 y = 0; y < height; ++y)
                        for (UINT32 x = 0; x < width; ++x)
                        {
                            auto pixel = inspectionPixels.bits + static_cast<size_t>(y) * inspectionPixels.pitch +
                                         static_cast<size_t>(x) * 4;
                            if (!pixel[3])
                                continue;
                            if (x >= 36 && x < 190 && y >= 44 && y < 204)
                            {
                                fingerprint = fingerprint * 16777619u + x + 2048u * y + pixel[3];
                                ++inside;
                            }
                            else
                                outside = true;
                        }
                    winrt::check_hresult(readback->Unmap());
                    Require(!outside && inside > 20, L"inspection draws real SVG pixels only inside its clip");
                    return fingerprint;
                };
                auto fitPixels = renderInspection({});
                auto detailPixels = renderInspection({3.0f, 0.0f, 0.0f});
                Require(fitPixels != detailPixels && document == visual.drawing.document.get() &&
                        stream == visual.drawing.stream.get() && bytes == visual.presentation->Bytes() &&
                        byteCount == visual.presentation->ByteCount(),
                        L"inspection zoom changes WARP pixels and keeps the same canonical SVG bytes, stream and document");
                context->SetTarget(target.get());

                // Inject a failed SVG creation for the current resource key.
                visual.drawing.document = nullptr;
                visual.drawing.stream = nullptr;
                visual.drawing.status.store(E_OUTOFMEMORY, std::memory_order_release);
                auto retry = moved->Reposition(2, moved->viewportHeight);
                Require(retry != nullptr, L"reposition creates a fresh draw outcome after a cache failure");
                context->BeginDraw();
                NativeDocumentRenderer::Draw(*retry, context.get(), 4, 4);
                winrt::check_hresult(context->EndDraw());
                Require(visual.drawing.document && visual.drawing.stream &&
                            retry->DiagramState(visual.object) == NativeDiagramState::Ready,
                        L"fresh frame retries failed SVG creation on the same resource key");
                context->SetTarget(nullptr);
                if (handoffOther)
                {
                    CheckFrameHandoff(frame, context.get(), *handoffOther);
                    CheckDeviceReuse(factory.get(), dxgi.get(), device.get(), visual.presentation);
                }
            }
            return colored;
        }
    }

    Results Run(std::shared_ptr<ReadingMemory> const &memory)
    {
        Results results;
        Require(memory && memory->ModelOwner(), L"diagram renderer uses a real accepted source");
        NativeMathControl control;
        auto error = Error();
        Require(control.Initialize(&error) == NOTEPADS_MARKDOWN_CORE_OK, L"diagram renderer uses actual common Derived authority");
        auto before = Usage(control.Get());
        {
            auto cancel = std::allocate_shared<PreviewCancellation>(ReadingAllocator<PreviewCancellation>{memory},
                                                                    memory->ModelOwner().Get());
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                                                                    memory, cancel, 71, NextReadingTextToken());
            reading->EnsureMain();
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = memory->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = 800;
            key.indent = 24;
            key.gap = 8;
            PreviewDiagramSnapshot snapshot;
            snapshot.enabled = true;
            snapshot.generation = 1;
            snapshot.key.identity = key.model;
            snapshot.key.sourceGeneration = reading->sourceGeneration;
            snapshot.key.textGeneration = reading->textGeneration;
            snapshot.key.attempt = snapshot.key.factoryEpoch = snapshot.key.fontEpoch = snapshot.key.styleEpoch = 1;
            snapshot.key.dpiEpoch = snapshot.key.targetWidthEpoch = snapshot.key.deviceEpoch = 1;
            snapshot.key.targetWidth = 800;
            snapshot.key.dpi = 96;
            std::atomic<uint64_t> latest{1};
            auto create = [&](PreviewDiagramSnapshot value, double width = 800, PreviewTheme theme = {},
                              std::shared_ptr<NativeDocumentRenderer> *retained = nullptr)
            {
                auto current = key;
                current.width = width;
                current.indent = (std::min)(width / 4, current.indent);
                ++key.layout_generation;
                auto renderer = NativeDocumentRenderer::Create({}, cancel, current, std::move(theme), reading, {}, {},
                                                               std::move(value));
                auto frame = renderer->CreateFrame(0, 1500, 1, latest);
                Require(frame && !reading->fault, L"one diagram cannot poison the rest of the source");
                if (retained) *retained = std::move(renderer);
                return frame;
            };
            auto pending = create(snapshot);
            auto const &pendingVisual = Visual(*pending);
            Require(pendingVisual.state == NativeDiagramState::Pending && pendingVisual.label &&
                        pendingVisual.hasInline,
                    L"first Pending uses a readable caption and a real Preview-funded zero-ink inline object");
            results.pendingLabel = true;
            auto object = pendingVisual.object;
            auto descriptor = std::find_if(
                reading->Descriptors()->objects.begin(), reading->Descriptors()->objects.end(),
                [&](auto const &value) { return value.object == object && value.kind == ReadingObjectKind::Diagram; });
            Require(descriptor != reading->Descriptors()->objects.end() &&
                        descriptor->text.end == descriptor->text.start + 1 &&
                        descriptor->input.find(L"flowchart") != ReadingString::npos &&
                        reading->Descriptors()->Copy(*reading->Main(), descriptor->text.start, descriptor->text.end) ==
                            descriptor->source,
                    L"actual one-position Main object copies the whole original fence and source line endings");
            results.sourceAndCopy = true;
            Fallback(*pending, descriptor->text.start, descriptor->text.end, L"Diagram pending");
            for (auto const &[state, caption] :
                 {std::pair{NativeDiagramState::Limited, L"Diagram limited"},
                  std::pair{NativeDiagramState::Unsupported, L"Diagram unsupported"},
                  std::pair{NativeDiagramState::Failed, L"Diagram failed"},
                  std::pair{NativeDiagramState::Unavailable, L"Diagram unavailable"},
                  std::pair{NativeDiagramState::Cancelled, L"Diagram cancelled"},
                  std::pair{NativeDiagramState::FontUnavailable, L"Diagram font unavailable"}})
            {
                auto typed = snapshot;
                typed.attempts[0] = state;
                auto frame = create(typed);
                Require(Visual(*frame).state == state && frame->DiagramState(object) == state && !reading->fault,
                        L"typed nonresident diagram attempts preserve source and effective accessibility state");
                Fallback(*frame, descriptor->text.start, descriptor->text.end, caption);
            }
            results.typedLabels = true;
            for (bool dark : {false, true})
            {
                PreviewTheme theme;
                theme.fontSize = 22;
                theme.lineHeight = 33;
                if (dark)
                {
                    theme.foreground = {255, 240, 240, 240};
                    theme.background = {255, 20, 20, 20};
                    theme.panelBackground = {255, 35, 35, 35};
                    theme.borderColor = {255, 120, 120, 120};
                    theme.highContrast = true;
                }
                auto typed = snapshot;
                typed.attempts[0] = NativeDiagramState::Limited;
                auto narrowFallback = create(typed, 96, std::move(theme));
                Fallback(*narrowFallback, descriptor->text.start, descriptor->text.end, L"Diagram limited");
                (void)DrawPixels(*narrowFallback, descriptor->text.start, descriptor->text.end, true);
            }
            Fonts fonts;
            snapshot.key.object = object;
            notepads_markdown_diagram_style style{{NOTEPADS_MARKDOWN_DIAGRAM_ABI_VERSION, sizeof(notepads_markdown_diagram_style)},
                                   0,
                                   0,
                                   {32, 36, 50, 255},
                                   {255, 255, 255, 255},
                                   {81, 90, 133, 255},
                                   {241, 243, 255, 220},
                                   {40, 80, 160, 255}};
            MathCoordinatorTests::CheckDiagramFailurePublication(memory->ModelOwner().Get(), snapshot.key,
                                                                 fonts.capability, style);
            Completion completion;
            Require(static_cast<bool>(completion.done), L"diagram renderer actual completion event");
            Require(NativeDiagramWorker::Prepare(control.Get(), memory->ModelOwner().Get(), snapshot.key, 0,
                                                 fonts.capability, style, &completion, Completion::Complete,
                                                 &completion.node, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                    L"diagram renderer prepares its real funded native Node");
            Require(NativeDiagramWorker::StartPrepared(completion.node).status == NOTEPADS_MARKDOWN_CORE_OK,
                    L"diagram renderer starts its real private-profile worker");
            completion.started = true;
            Require(WaitForSingleObjectEx(completion.done.get(), 10000, FALSE) == WAIT_OBJECT_0 &&
                        completion.status == NOTEPADS_MARKDOWN_CORE_OK && completion.payload &&
                        completion.payload->State() == NativeDiagramState::Ready,
                    L"Ready coverage requires actual compiled-profile output after real thread exit");
            Require(completion.payload->LinkBefore({}), L"diagram presentation freezes its immutable next once");
            snapshot.presentations.Add(snapshot.key, object, completion.payload);
            snapshot.attempts[0] = NativeDiagramState::Ready;
            ++snapshot.generation;
            // Handoff deep copies allocate through the original PreviewAllocator.
            // Keep its layout owner alive until every copied frame is destroyed.
            std::shared_ptr<NativeDocumentRenderer> readyRenderer;
            auto ready = create(snapshot, 800, {}, &readyRenderer);
            auto const &visual = Visual(*ready);
            Require(visual.state == NativeDiagramState::Ready && visual.hasInline && !visual.label,
                    L"real funded artifact installs its prepaid inline metrics");
            Caption(visual.drawFailureLabels[0].get(), L"Diagram failed");
            Caption(visual.drawFailureLabels[1].get(), L"Diagram limited");
            Caption(visual.drawFailureLabels[2].get(), L"Diagram unavailable");
            results.nativeReady = true;
            auto rectangles = NativeDocumentRenderer::Rectangles(*ready, descriptor->text.start, descriptor->text.end);
            Require(rectangles.size() == 1 && std::abs(rectangles[0].width - visual.width) < 0.01f &&
                        std::abs(rectangles[0].height - visual.height) < 0.01f,
                    L"atomic selection uses actual fitted SVG dimensions");
            auto hit = NativeDocumentRenderer::HitTest(*ready, rectangles[0].x + rectangles[0].width / 2,
                                                       rectangles[0].y + rectangles[0].height / 2);
            Require(hit && hit->inside && hit->cluster.start == descriptor->text.start &&
                        hit->cluster.end == descriptor->text.end,
                    L"diagram center hit is exactly the existing Main object, never an internal node");
            results.atomicGeometry = true;
            results.exactInline = visual.baseline == visual.height && visual.width == visual.placement.width;
            Require(results.exactInline, L"inline and draw use the same fitted dimensions and bottom baseline");
            {
                // A second real funded Ready Node has an equal captured key but
                // a distinct owner. Do not fabricate a presentation around bytes.
                Completion other;
                Require(static_cast<bool>(other.done), L"handoff alternate owner has its real completion event");
                Require(NativeDiagramWorker::Prepare(control.Get(), memory->ModelOwner().Get(), snapshot.key, 0,
                            fonts.capability, style, &other, Completion::Complete, &other.node, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                        L"handoff prepares an independent real funded owner");
                Require(NativeDiagramWorker::StartPrepared(other.node).status == NOTEPADS_MARKDOWN_CORE_OK,
                        L"handoff starts the independent private-profile worker");
                other.started = true;
                Require(WaitForSingleObjectEx(other.done.get(), 10000, FALSE) == WAIT_OBJECT_0 &&
                            other.status == NOTEPADS_MARKDOWN_CORE_OK && other.payload &&
                            other.payload->State() == NativeDiagramState::Ready && other.payload->LinkBefore({}),
                        L"handoff mismatched-owner counterexample requires actual compiled output after thread exit");
                results.inkPixels = DrawPixels(*ready, descriptor->text.start, descriptor->text.end, false, false, &other.payload);
            }
            readyRenderer.reset();
            results.nativePixels = results.sameContextReuse = true;
            {
                winrt::com_ptr<IStream> stream;
                auto retained = *snapshot.presentations.Find(object);
                winrt::check_hresult(NativeDiagramArtifact::OpenReadStream(retained, stream.put()));
                STATSTG info{};
                winrt::check_hresult(stream->Stat(&info, STATFLAG_NONAME));
                snapshot.presentations = {};
                Require(info.cbSize.QuadPart == retained->ByteCount(),
                        L"stream covers the exact original native buffer without hidden copy");
                uint8_t first{};
                ULONG count{};
                winrt::check_hresult(stream->Read(&first, 1, &count));
                Require(count == 1 && first == '<',
                        L"actual Node stream remains readable through its independent last COM ref");
                snapshot.presentations.Add(snapshot.key, object, retained);
                results.streamRetainsOwner = true;
            }
            auto narrow = create(snapshot, 96);
            auto const &fitted = Visual(*narrow);
            Require(fitted.state == NativeDiagramState::Ready && fitted.width <= 96 && fitted.placement.scale <= 1 &&
                        std::abs(fitted.width / fitted.height - visual.width / visual.height) < 0.001f,
                    L"narrow display fits proportionally without enlarging or changing Main");
            results.narrowFit = true;
            auto drawFailure = create(snapshot, 96);
            auto moved = drawFailure->Reposition(1, drawFailure->viewportHeight);
            Require(moved != nullptr, L"diagram frame can be repositioned before its first draw");
            (void)DrawPixels(*moved, descriptor->text.start, descriptor->text.end, false, true);
            Require(drawFailure->DiagramState(object) == NativeDiagramState::Ready &&
                        moved->DiagramState(object) == NativeDiagramState::Failed,
                    L"repositioned failure cannot change the original frame's diagram result");
            (void)DrawPixels(*moved, descriptor->text.start, descriptor->text.end);
            Require(moved->DiagramState(object) == NativeDiagramState::Ready,
                    L"redraw on a supported context clears the frame's prior failure");
            Require(!reading->fault &&
                        reading->Descriptors()->Copy(*reading->Main(), descriptor->text.start, descriptor->text.end) ==
                            descriptor->source,
                    L"Ready Draw failure preserves literal Main/source copy and does not fault the document");
            auto stale = snapshot;
            ++stale.key.attempt;
            stale.attempts[0] = NativeDiagramState::Pending;
            auto refused = create(stale);
            Require(Visual(*refused).state == NativeDiagramState::Pending && !Visual(*refused).presentation,
                    L"new attempt cannot consume a stale ready artifact");
            results.staleArtifactRejected = true;
            results.immutableFrame = ready->DiagramState(object) == NativeDiagramState::Ready &&
                                     Visual(*pending).state == NativeDiagramState::Pending;
            Require(results.immutableFrame, L"publication cannot mutate either an old Pending or an old Ready Frame");
        }
        auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
        do
        {
            auto after = Usage(control.Get());
            if (after.control.bytes == before.control.bytes &&
                after.control.retained_bytes == before.control.retained_bytes &&
                after.control.jobs == before.control.jobs && after.control.residents == before.control.residents)
            {
                results.nativeOwnersDropped = true;
                break;
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(1));
        } while (std::chrono::steady_clock::now() < deadline);
        Require(results.nativeOwnersDropped,
                L"Frames, native SVG, streams, inline refs and Node really return shared policy to baseline");
        return results;
    }
}
