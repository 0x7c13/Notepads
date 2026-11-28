// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeDocumentRendererTests.h"
#include "Rendering/NativeDocumentRenderer.h"
#include "Resources/Math/NativeMathArtifact.h"
#include "Resources/Math/NativeMathWorker.h"
#include "notepads/markdown/derived.h"
#include <array>
#include <chrono>
#include <thread>

namespace Notepads::Markdown::Native::MathRendererTests
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
            notepads_markdown_math_usage usage{};
            usage.header = {NOTEPADS_MARKDOWN_MATH_ABI_VERSION, sizeof(usage)};
            auto error = Error();
            Require(notepads_markdown_math_control_usage(control, &usage, &error) == NOTEPADS_MARKDOWN_CORE_OK, L"renderer real policy usage");
            return usage;
        }

        struct Job
        {
            notepads_markdown_math_job *value{};
            ~Job()
            {
                if (value)
                {
                    auto error = Error();
                    (void)notepads_markdown_math_job_release(value, &error);
                }
            }
        };

        struct Completion
        {
            winrt::handle done{CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET, EVENT_ALL_ACCESS)};
            winrt::com_ptr<ID2D1Factory1> factory;
            NativeMathArtifactResult artifact;
            int32_t status{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
            bool started{};

            Completion()
            {
                Require(static_cast<bool>(done), L"renderer fixed completion event created");
                winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, factory.put()));
            }
            ~Completion()
            {
                if (started)
                {
                    // Failure-only background rescue: the stack callback record
                    // remains alive until the actual callback has returned.
                    (void)WaitForSingleObjectEx(done.get(), INFINITE, FALSE);
                }
            }
            static void Complete(void *context, notepads_markdown_math_job *job, NativeMathWorkerResult const &result) noexcept
            {
                auto self = static_cast<Completion *>(context);
                self->status = result.status;
                if (result.status == NOTEPADS_MARKDOWN_CORE_OK && result.stack.threadSignaled)
                {
                    self->artifact = NativeMathArtifact::BuildAfterThreadExit(job, self->factory.get());
                    self->status = self->artifact.status;
                }
                SetEvent(self->done.get());
            }
        };

        PreviewMathVisual const &Visual(PreviewFrame const &frame)
        {
            PreviewMathVisual const *result{};
            for (auto const &block : frame.Blocks())
            {
                for (auto const &text : block.texts)
                {
                    for (auto const &math : text.math)
                    {
                        Require(!result, L"fixed renderer fixture has one actual Math occurrence");
                        result = &math;
                    }
                }
            }
            Require(result != nullptr, L"actual derived row became a renderer visual");
            return *result;
        }

        void RequireMathHover(PreviewFrame const &frame, bool expected)
        {
            size_t count{};
            for (auto const &block : frame.Blocks())
            {
                for (auto const &text : block.texts)
                {
                    for (auto const &math : text.math)
                    {
                        ++count;
                        auto left = static_cast<float>(block.block.x) + text.x + math.x;
                        auto top = static_cast<float>(block.block.y - frame.viewportY) + text.y + math.y;
                        auto x = left + math.width / 2;
                        auto y = top + math.height / 2;
                        auto hit = NativeDocumentRenderer::MathFallbackAt(frame, x, y);
                        Require(expected ? hit == text.logicalStart + math.offset : !hit,
                                L"actual math box offers terminal diagnostics but never Ready/loading errors");
                        Require(!NativeDocumentRenderer::MathFallbackAt(frame, left - 1, y) &&
                                    !NativeDocumentRenderer::MathFallbackAt(frame, x, top - 1),
                                L"math hover does not snap adjacent whitespace to an error");
                        Require(!NativeDocumentRenderer::ImageAt(frame, x, y),
                                L"math hover leaves the existing image-only geometry contract unchanged");
                    }
                }
            }
            Require(count == 1, L"fixed hover fixture uses its one actual immutable math visual");
        }

        class MathRaster
        {
            winrt::com_ptr<ID2D1DeviceContext> _context;
            winrt::com_ptr<ID2D1Bitmap1> _target, _readback;

          public:
            static constexpr UINT32 Width = 1024, Height = 256;
            explicit MathRaster(ID2D1Factory1 *factory)
            {
                winrt::com_ptr<ID3D11Device> d3d;
                winrt::com_ptr<ID3D11DeviceContext> d3dContext;
                winrt::check_hresult(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr,
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
                    d3d.put(), nullptr, d3dContext.put()));
                winrt::com_ptr<ID2D1Device> device;
                winrt::check_hresult(factory->CreateDevice(d3d.as<IDXGIDevice>().get(), device.put()));
                winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, _context.put()));
                auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_TARGET,
                    D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96);
                winrt::check_hresult(_context->CreateBitmap(D2D1::SizeU(Width, Height), nullptr, 0, properties, _target.put()));
                properties.bitmapOptions = D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
                winrt::check_hresult(_context->CreateBitmap(D2D1::SizeU(Width, Height), nullptr, 0, properties, _readback.put()));
            }
            std::vector<uint8_t> Paint(PreviewFrame const &frame, float originY, uint32_t start = 0, uint32_t end = 0,
                                      std::optional<D2D1_RECT_F> clip = {},
                                      D2D1_MATRIX_3X2_F transform = D2D1::Matrix3x2F::Identity())
            {
                _context->SetDpi(96, 96);
                _context->SetTransform(transform);
                _context->SetTarget(_target.get());
                _context->BeginDraw();
                _context->Clear(D2D1::ColorF(D2D1::ColorF::White));
                try
                {
                    NativeDocumentRenderer::Draw(frame, _context.get(), 4, originY, start, end,
                                                 {}, false, nullptr, false, {}, clip);
                }
                catch (...)
                {
                    (void)_context->EndDraw();
                    throw;
                }
                winrt::check_hresult(_context->EndDraw());
                _context->SetTarget(nullptr);
                winrt::check_hresult(_readback->CopyFromBitmap(nullptr, _target.get(), nullptr));
                D2D1_MAPPED_RECT mapped{};
                winrt::check_hresult(_readback->Map(D2D1_MAP_OPTIONS_READ, &mapped));
                std::vector<uint8_t> pixels(static_cast<size_t>(Width) * Height * 4);
                for (UINT32 y = 0; y < Height; ++y)
                    std::memcpy(pixels.data() + static_cast<size_t>(y) * Width * 4,
                                mapped.bits + static_cast<size_t>(y) * mapped.pitch, Width * 4);
                winrt::check_hresult(_readback->Unmap());
                return pixels;
            }
        };

        void DrawSameFactory(PreviewFrame const &frame, ID2D1Factory1 *factory, uint32_t start, uint32_t end)
        {
            (void)MathRaster{factory}.Paint(frame, 4, start, end);
        }

        // Synchronous stack-only observer of the real DWrite layout callbacks.
        // It neither substitutes glyph metrics nor issues any graphics calls.
        class TextBaselines final : public IDWriteTextRenderer
        {
            std::atomic<ULONG> _references{1};
            float _expected{};

          public:
            bool prefix{}, object{}, suffix{}, matches{true};
            explicit TextBaselines(float expected) noexcept : _expected(expected)
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
                // The caller owns the initial reference on its stack until
                // IDWriteTextLayout::Draw has synchronously returned.
                return --_references;
            }
            HRESULT STDMETHODCALLTYPE IsPixelSnappingDisabled(void *, BOOL *disabled) noexcept override
            {
                if (!disabled)
                    return E_POINTER;
                *disabled = TRUE;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetCurrentTransform(void *, DWRITE_MATRIX *transform) noexcept override
            {
                if (!transform)
                    return E_POINTER;
                *transform = {1, 0, 0, 1, 0, 0};
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetPixelsPerDip(void *, FLOAT *value) noexcept override
            {
                if (!value)
                    return E_POINTER;
                *value = 1;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT, FLOAT baseline, DWRITE_MEASURING_MODE,
                                                   DWRITE_GLYPH_RUN const *run,
                                                   DWRITE_GLYPH_RUN_DESCRIPTION const *description,
                                                   IUnknown *) noexcept override
            {
                if (!run || !description || !run->glyphCount)
                    return E_INVALIDARG;
                // Real fixture Main is exactly `pre U+FFFC post`: the glyph
                // runs before and after its one inline object are independent
                // observations of the surrounding text's actual baseline.
                prefix |= description->textPosition < 4 && description->stringLength <= 4 - description->textPosition;
                suffix |= description->textPosition >= 5;
                matches &= std::isfinite(baseline) && std::abs(baseline - _expected) <= 0.001f;
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
            HRESULT STDMETHODCALLTYPE DrawInlineObject(void *, FLOAT, FLOAT originY, IDWriteInlineObject *inlineObject,
                                                       BOOL, BOOL, IUnknown *) noexcept override
            {
                if (!inlineObject)
                    return E_POINTER;
                DWRITE_INLINE_OBJECT_METRICS metrics{};
                auto status = inlineObject->GetMetrics(&metrics);
                if (FAILED(status))
                    return status;
                // DWrite passes the object's top-left origin to this callback,
                // unlike DrawGlyphRun's baselineOriginY.
                auto baseline = originY + metrics.baseline;
                object = true;
                matches &= std::isfinite(baseline) && std::abs(baseline - _expected) <= 0.001f;
                return S_OK;
            }
        };

        struct BaselineInk
        {
            uint64_t pixels{};
            D2D1_RECT_F pixelsBox{}, inlineBox{};
            float textBaseline{}, expectedTop{}, expectedBottom{};
            bool contained{}, matchesText{};
        };

        BaselineInk ReadBaselineInk(PreviewFrame const &frame, ID2D1Factory1 *factory)
        {
            auto const &math = Visual(frame);
            Require(math.state == NativeMathState::Ready && math.presentation && !math.label,
                    L"baseline pixels require the actual Ready artifact, never a status placeholder");
            IDWriteTextLayout *layout{};
            for (auto const &block : frame.Blocks())
                for (auto const &text : block.texts)
                    if (!text.math.empty())
                        layout = text.layout.get();
            Require(layout != nullptr, L"baseline pixels use the actual renderer's DWrite layout");
            DWRITE_LINE_METRICS line{};
            DWRITE_TEXT_METRICS text{};
            UINT32 lines{};
            winrt::check_hresult(layout->GetLineMetrics(&line, 1, &lines));
            winrt::check_hresult(layout->GetMetrics(&text));
            Require(lines == 1 && std::isfinite(line.baseline) && std::isfinite(text.top),
                    L"real fixed fixture has one finite text baseline");
            float baseline = text.top + line.baseline;
            TextBaselines probe{baseline};
            winrt::check_hresult(layout->Draw(nullptr, &probe, 0, 0));
            Require(probe.prefix && probe.object && probe.suffix && probe.matches &&
                        std::abs(math.anchor.baseline - baseline) <= 0.001f,
                    L"actual prefix, inline callback, suffix and stored replay anchor share the same DWrite baseline");
            winrt::com_ptr<IDWriteInlineObject> object;
            DWRITE_TEXT_RANGE range{};
            DWRITE_INLINE_OBJECT_METRICS inlineMetrics{};
            winrt::check_hresult(layout->GetInlineObject(math.offset, object.put(), &range));
            Require(object && range.startPosition == math.offset && range.length == 1,
                    L"baseline pixels inspect the actual installed one-position inline object");
            winrt::check_hresult(object->GetMetrics(&inlineMetrics));
            Require(std::abs(inlineMetrics.baseline - math.placement.metrics.baseline) <= 0.001f &&
                        std::abs(inlineMetrics.height - math.placement.metrics.height) <= 0.001f &&
                        std::abs(inlineMetrics.width - math.placement.metrics.width) <= 0.001f,
                    L"pixel containment uses actual DWrite inline metrics matching the current placement");

            winrt::com_ptr<ID3D11Device> d3d;
            winrt::com_ptr<ID3D11DeviceContext> d3dContext;
            D3D_FEATURE_LEVEL level{};
            winrt::check_hresult(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr,
                                                   D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
                                                   d3d.put(), &level, d3dContext.put()));
            auto dxgi = d3d.as<IDXGIDevice>();
            winrt::com_ptr<ID2D1Device> device;
            winrt::check_hresult(factory->CreateDevice(dxgi.get(), device.put()));
            winrt::com_ptr<ID2D1DeviceContext> context;
            winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, context.put()));
            context->SetDpi(96, 96);
            context->SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            constexpr UINT32 width = 1024, height = 256;
            auto properties = D2D1::BitmapProperties1(
                D2D1_BITMAP_OPTIONS_TARGET,
                D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96);
            winrt::com_ptr<ID2D1Bitmap1> target;
            winrt::check_hresult(
                context->CreateBitmap(D2D1::SizeU(width, height), nullptr, 0, properties, target.put()));
            auto anchor = math.anchor;
            constexpr float origin = 32;
            anchor.left += origin;
            anchor.baseline += origin;
            context->SetTarget(target.get());
            context->BeginDraw();
            context->Clear(D2D1::ColorF(0, 0, 0, 0));
            auto status = math.presentation->Drawing()->Replay(context.get(), math.placement, anchor,
                                                               D2D1::ColorF(D2D1::ColorF::White), true);
            auto endStatus = context->EndDraw();
            context->SetTarget(nullptr);
            winrt::check_hresult(status);
            winrt::check_hresult(endStatus);
            properties.bitmapOptions = D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
            winrt::com_ptr<ID2D1Bitmap1> readback;
            winrt::check_hresult(
                context->CreateBitmap(D2D1::SizeU(width, height), nullptr, 0, properties, readback.put()));
            winrt::check_hresult(readback->CopyFromBitmap(nullptr, target.get(), nullptr));
            D2D1_MAPPED_RECT mapped{};
            winrt::check_hresult(readback->Map(D2D1_MAP_OPTIONS_READ, &mapped));
            UINT32 left = width, top = height, right{}, bottom{};
            BaselineInk result;
            for (UINT32 y = 0; y < height; ++y)
            {
                auto row = mapped.bits + static_cast<size_t>(y) * mapped.pitch;
                for (UINT32 x = 0; x < width; ++x)
                {
                    if (!row[static_cast<size_t>(x) * 4 + 3])
                        continue;
                    ++result.pixels;
                    left = std::min(left, x);
                    top = std::min(top, y);
                    right = std::max(right, x + 1);
                    bottom = std::max(bottom, y + 1);
                }
            }
            winrt::check_hresult(readback->Unmap());
            Require(result.pixels > 0 && left > 0 && top > 0 && right < width && bottom < height,
                    L"standalone formula replay produces actual nonzero alpha away from target clipping edges");
            result.pixelsBox = {static_cast<float>(left), static_cast<float>(top), static_cast<float>(right),
                                static_cast<float>(bottom)};
            result.inlineBox = {anchor.left, anchor.baseline - inlineMetrics.baseline,
                                anchor.left + inlineMetrics.width,
                                anchor.baseline - inlineMetrics.baseline + inlineMetrics.height};
            result.textBaseline = origin + baseline;
            // Independent font oracle, read from the unmodified fixed asset:
            // KaTeX_Math-Italic.ttf SHA256 08ce98e51b04d58945a301e639e02b699
            // 8af29fdfd61a7b8afdd07bbfc479d4a, unitsPerEm=1000.
            // a glyph40 bbox=(33,-10,506,441); b glyph41=(40,-11,422,694).
            // The existing fixed `a b` oracle has a.advance=0.52859, no space
            // advance, ascent=0.69444. Do not derive these limits from this
            // artifact's occupied bounds: a wrong origin can inflate that box.
            auto scale = math.placement.emToDips;
            result.expectedTop = result.textBaseline - 0.694f * scale;
            result.expectedBottom = result.textBaseline + 0.011f * scale;
            constexpr float tolerance = 1;
            result.contained = result.pixelsBox.left >= result.inlineBox.left - tolerance &&
                               result.pixelsBox.top >= result.inlineBox.top - tolerance &&
                               result.pixelsBox.right <= result.inlineBox.right + tolerance &&
                               result.pixelsBox.bottom <= result.inlineBox.bottom + tolerance;
            result.matchesText = std::abs(result.pixelsBox.top - result.expectedTop) <= tolerance &&
                                 std::abs(result.pixelsBox.bottom - result.expectedBottom) <= tolerance &&
                                 std::abs(result.pixelsBox.left - (anchor.left + 0.033f * scale)) <= tolerance &&
                                 std::abs(result.pixelsBox.right - (anchor.left + 0.95059f * scale)) <= tolerance;
            return result;
        }
        void RequirePaintClipPixels(PreviewFrame const &ready, ID2D1Factory1 *factory, BaselineInk const &ink)
        {
            MathRaster raster{factory};
            float blockX{}, blockY{};
            for (auto const &block : ready.Blocks())
                for (auto const &text : block.texts)
                    if (!text.math.empty())
                    {
                        blockX = static_cast<float>(block.block.x) + text.x;
                        blockY = static_cast<float>(block.block.y - ready.viewportY) + text.y;
                    }
            // The oracle is the actual standalone formula pixels, not OccupiedEm or cull math.
            auto const inkTop = blockY + ink.pixelsBox.top - 32;
            auto const inkBottom = blockY + ink.pixelsBox.bottom - 32;
            for (unsigned scenario = 0; scenario != 4; ++scenario)
            {
                auto frame = ready;
                auto originY = scenario == 0 ? -0.5f - inkTop
                               : scenario == 1 ? MathRaster::Height + 0.5f - inkBottom : 32.0f;
                if (scenario == 2)
                    frame.viewportHeight = 1; // A larger generic target can still paint below this metadata.
                auto const scaleY = scenario == 3 ? 0.75f : 1.0f;
                auto const transform = D2D1::Matrix3x2F::Scale(1.0f, scaleY);
                auto const clip = D2D1::RectF(0, 0, MathRaster::Width, MathRaster::Height / scaleY);
                auto const uncropped = raster.Paint(frame, originY, 0, 0, {}, transform);
                auto const cropped = raster.Paint(frame, originY, 0, 0, clip, transform);
                Require(uncropped == cropped, L"explicit complete paint clip preserves actual WARP pixels at edges/default/transform");
                uint64_t formulaPixels{};
                auto const left = std::clamp(static_cast<int>(std::floor(blockX + ink.pixelsBox.left - 28)), 0, static_cast<int>(MathRaster::Width));
                auto const right = std::clamp(static_cast<int>(std::ceil(blockX + ink.pixelsBox.right - 28)), 0, static_cast<int>(MathRaster::Width));
                auto const top = std::clamp(static_cast<int>(std::floor((originY + inkTop) * scaleY)), 0, static_cast<int>(MathRaster::Height));
                auto const bottom = std::clamp(static_cast<int>(std::ceil((originY + inkBottom) * scaleY)), 0, static_cast<int>(MathRaster::Height));
                for (auto y = top; y < bottom; ++y)
                    for (auto x = left; x < right; ++x)
                    {
                        auto pixel = uncropped.data() + (static_cast<size_t>(y) * MathRaster::Width + x) * 4;
                        formulaPixels += pixel[0] != 255 || pixel[1] != 255 || pixel[2] != 255;
                    }
                Require(formulaPixels > 0, L"pixel comparison includes real visible formula ink, never two blank targets");
            }
        }
    }

    Results Run(std::shared_ptr<ReadingMemory> const &memory)
    {
        Results results;
        Require(memory && memory->ModelOwner(), L"renderer tests borrow the actual accepted source owner");
        NativeMathControl control;
        auto error = Error();
        Require(control.Initialize(&error) == NOTEPADS_MARKDOWN_CORE_OK, L"renderer tests use real shared runtime control");
        auto baseline = Usage(control.Get());
        {
            auto cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
            auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                                                                    memory, cancel, 41, NextReadingTextToken());
            reading->EnsureMain();
            notepads_markdown_preview_key key{};
            key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
            key.model = memory->Identity();
            key.layout_generation = key.theme_generation = key.measurement_generation = 1;
            key.width = 800;
            key.indent = 24;
            key.gap = 8;
            PreviewMathSnapshot math;
            math.enabled = true;
            math.generation = 1;
            math.key.identity = key.model;
            math.key.sourceGeneration = reading->sourceGeneration;
            math.key.textGeneration = reading->textGeneration;
            math.key.attempt = math.key.factoryEpoch = 1;
            std::atomic<uint64_t> latest{1};
            auto create = [&](PreviewMathSnapshot snapshot, double width = 800)
            {
                auto currentKey = key;
                currentKey.width = width;
                currentKey.indent = std::min(currentKey.indent, width / 4);
                ++key.layout_generation;
                auto renderer = NativeDocumentRenderer::Create({}, cancel, currentKey, PreviewTheme{}, reading, {},
                                                               std::move(snapshot));
                auto frame = renderer->CreateFrame(0, 1000, 1, latest);
                Require(frame && reading->fault == 0, L"one formula cannot poison ordinary source reading");
                return frame;
            };
            auto pending = create(math);
            auto const &pendingVisual = Visual(*pending);
            auto object = pendingVisual.object;
            Require(pendingVisual.state == NativeMathState::Pending && pendingVisual.label &&
                        pending->MathState(object) == NativeMathState::Pending,
                    L"initial U+FFFC becomes a readable pending source label with matching effective UIA state");
            RequireMathHover(*pending, false);
            results.pendingLabel = true;
            Require(reading->Main()->Text() == L"pre \ufffc post" && reading->Descriptors() &&
                        reading->Descriptors()->objects.size() == 1 &&
                        reading->Descriptors()->Copy(*reading->Main(), 0, reading->Main()->Length()) == L"pre $a\r\nb$ post",
                    L"renderer leaves real Main atomic and copy preserves original delimiters and CRLF");
            auto const &descriptor = reading->Descriptors()->objects[0];
            Require(descriptor.syntax == NOTEPADS_MARKDOWN_MATH_DOLLAR_INLINE && descriptor.input == L"a b" &&
                        descriptor.source == L"$a\r\nb$",
                    L"syntax and original complete input/source come from the actual derived descriptor");
            results.sourceAndCopy = true;
            for (auto state : {NativeMathState::Limited, NativeMathState::Unsupported, NativeMathState::Failed,
                               NativeMathState::Unavailable, NativeMathState::Cancelled})
            {
                auto typed = math;
                typed.attempts[0] = state;
                auto frame = create(typed);
                RequireMathHover(*frame, true);
                Require(Visual(*frame).state == state && Visual(*frame).label && frame->MathState(object) == state,
                        L"nonresident typed attempts remain visible and effective status agrees");
            }
            results.typedLabels = true;

            Job job;
            Require(notepads_markdown_math_job_prepare(control.Get(), memory->ModelOwner().Get(), object, &job.value, &error) ==
                            NOTEPADS_MARKDOWN_CORE_OK &&
                        notepads_markdown_math_job_activate(job.value, &error) == NOTEPADS_MARKDOWN_CORE_OK,
                    L"renderer actual formula owns a real prepared and activated job");
            Completion completion;
            auto start = NativeMathWorker::Start(job.value, Completion::Complete, &completion);
            Require(start.status == NOTEPADS_MARKDOWN_CORE_OK, L"renderer starts real controlled builtin-profile worker");
            completion.started = true;
            Require(WaitForSingleObjectEx(completion.done.get(), 10000, FALSE) == WAIT_OBJECT_0 &&
                        completion.status == NOTEPADS_MARKDOWN_CORE_OK && completion.artifact.drawing && completion.artifact.memory,
                    L"renderer receives actual post-signaled funded native geometry");
            auto occurrence = math.key;
            occurrence.object = object;
            occurrence.syntax = descriptor.syntax;
            auto artifact = NativeMathPresentation::CreateReady(occurrence, completion.artifact.memory,
                                                                 completion.artifact.drawing);
            Require(artifact->LinkBefore({}), L"renderer ready presentation freezes its immutable next once");
            math.presentations.Add(math.key, object, artifact);
            math.attempts[0] = NativeMathState::Ready;
            ++math.generation;
            auto ready = create(math);
            auto const &readyVisual = Visual(*ready);
            Require(readyVisual.state == NativeMathState::Ready && !readyVisual.label &&
                        readyVisual.placement.status == MathPlacementStatus::Ready && readyVisual.anchor.baseline > 0 &&
                        ready->MathState(object) == NativeMathState::Ready,
                    L"real artifact installs funded inline metrics and actual DirectWrite baseline");
            RequireMathHover(*ready, false);
            results.nativeReady = true;
            DrawSameFactory(*ready, completion.factory.get(), descriptor.text.start, descriptor.text.end);
            results.sameFactoryDraw = true;
            auto ink = ReadBaselineInk(*ready, completion.factory.get());
            results.baselineInkPixels = ink.pixels;
            results.baselineInkTop = ink.pixelsBox.top;
            results.baselineInkBottom = ink.pixelsBox.bottom;
            results.baselineExpectedTop = ink.expectedTop;
            results.baselineExpectedBottom = ink.expectedBottom;
            results.baselineTextY = ink.textBaseline;
            results.baselineInlineTop = ink.inlineBox.top;
            results.baselineInlineBottom = ink.inlineBox.bottom;
            results.baselineInkWithinInlineBox = ink.contained;
            results.baselineInkMatchesTextBaseline = ink.matchesText;
            auto detail = L"actual math alpha pixels=" + std::to_wstring(ink.pixels) + L" y=[" +
                          std::to_wstring(ink.pixelsBox.top) + L"," + std::to_wstring(ink.pixelsBox.bottom) +
                          L"]; DWrite baseline=" + std::to_wstring(ink.textBaseline) + L"; fixed TTF expected y=[" +
                          std::to_wstring(ink.expectedTop) + L"," + std::to_wstring(ink.expectedBottom) +
                          L"]; inline box y=[" + std::to_wstring(ink.inlineBox.top) + L"," +
                          std::to_wstring(ink.inlineBox.bottom) + L"] (AA tolerance 1px)";
            Require(results.baselineInkWithinInlineBox, detail.c_str());
            Require(results.baselineInkMatchesTextBaseline, detail.c_str());
            RequirePaintClipPixels(*ready, completion.factory.get(), ink);
            auto narrow = create(math, 8);
            Require(Visual(*narrow).state == NativeMathState::Limited && Visual(*narrow).label &&
                        narrow->MathState(object) == NativeMathState::Limited,
                    L"one formula wider than its inline width becomes visible Limited without source fault");
            RequireMathHover(*narrow, true);
            results.widthLimited = true;
            {
                std::array<winrt::com_ptr<IDWriteInlineObject>, NativeMathMemory::InlineSlots> held;
                auto measuredRenderer =
                    NativeDocumentRenderer::Create({}, cancel, key, PreviewTheme{}, reading, {}, math);
                size_t index{};
                auto owner = completion.artifact.memory;
                while (owner->InlineInUse() < NativeMathMemory::InlineSlots)
                    held[index++] = NativeMathDrawing::CreateInlineObject(owner, readyVisual.placement, L"\ufffc", 0);
                auto reused = measuredRenderer->CreateFrame(0, 1000, 1, latest);
                Require(reused && Visual(*reused).state == NativeMathState::Ready &&
                            owner->InlineInUse() == NativeMathMemory::InlineSlots,
                        L"Frame reuses its measured immutable inline even when all later arena slots are occupied");
                auto limited = create(math);
                Require(Visual(*limited).state == NativeMathState::Limited && Visual(*limited).label &&
                            limited->MathState(object) == NativeMathState::Limited && reading->fault == 0,
                        L"exhausted actual funded arena falls back on the single object boundary");
                results.arenaLimited = true;
                Require(ready->MathState(object) == NativeMathState::Ready && !Visual(*ready).label,
                        L"old Frame retains its own immutable ready artifact and placement");
                results.immutableFrame = true;
            }
            auto recovered = create(math);
            Require(Visual(*recovered).state == NativeMathState::Ready && !Visual(*recovered).label,
                    L"last inline release permits later frames to reuse the same artifact and prepaid slots");
            results.arenaRecovered = true;
            auto stale = math;
            ++stale.key.attempt;
            stale.attempts[0] = NativeMathState::Pending;
            auto refused = create(stale);
            Require(Visual(*refused).state == NativeMathState::Pending && Visual(*refused).label &&
                        !Visual(*refused).presentation,
                    L"new attempt never consumes a stale immutable artifact head");
            RequireMathHover(*refused, false);
            results.staleArtifactRejected = true;
            ++stale.key.sourceGeneration;
            try
            {
                (void)create(stale);
            }
            catch (ReadingFailure const &failure)
            {
                results.staleSourceRejected = failure.status == NOTEPADS_MARKDOWN_CORE_STALE_RESULT && reading->fault == 0;
            }
            Require(results.staleSourceRejected,
                    L"source gate rejects stale math before mutating reading source fault");
        }
        auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
        do
        {
            auto usage = Usage(control.Get());
            if (usage.control.bytes == baseline.control.bytes && usage.control.jobs == baseline.control.jobs &&
                usage.control.retained_bytes == baseline.control.retained_bytes)
            {
                results.nativeOwnersDropped = true;
                break;
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(1));
        } while (std::chrono::steady_clock::now() < deadline);
        Require(results.nativeOwnersDropped, L"real renderer Frames/arena/head and worker drop to policy baseline");
        return results;
    }
}
