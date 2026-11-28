// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeFinalSurfaceThreadTests.h"
#include "NativeTextBitmapCacheTests.h"
#include "Core/ModuleLifetime.h"
#include "Rendering/NativeDocumentRenderer.h"
#include <d3d11_4.h>
#include <array>
#include <cstring>
#include <exception>

namespace Notepads::Markdown::Native::FinalSurfaceThreadTests
{
    namespace
    {
        using TextBitmapCacheTests::Failure;
        using winrt::Windows::UI::Core::CoreDispatcher;
        using winrt::Windows::UI::Xaml::Window;
        using winrt::Windows::UI::Xaml::UIElement;
        using winrt::Windows::UI::Xaml::Media::Imaging::SurfaceImageSource;

        constexpr LONG Width = 1024, Height = 512;
        constexpr uint64_t PixelLimit = 4 * 1024 * 1024;
        void Require(bool value, std::wstring const &message)
        {
            if (!value)
                throw Failure{message};
        }

        // Successful cleanup assertions must not allocate before required native
        // completion and UI-owned release. Dynamic messages keep the overload above.
        void Require(bool value, wchar_t const *message)
        {
            if (!value)
                throw Failure{message};
        }

        std::wstring Describe(std::exception_ptr const &failure)
        {
            try
            {
                if (failure) std::rethrow_exception(failure);
            }
            catch (Failure const &error) { return error.detail; }
            catch (NativeFailure const &error) { return L"native status=" + std::to_wstring(error.status); }
            catch (winrt::hresult_error const &error)
            {
                return L"HRESULT=" + std::to_wstring(static_cast<int32_t>(error.code())) +
                    L" " + std::wstring{error.message().c_str()};
            }
            catch (...) { return L"unclassified native exception"; }
            return L"none";
        }

        struct Device
        {
            winrt::com_ptr<ID3D11Device> d3d;
            winrt::com_ptr<ID2D1Factory1> factory;
            winrt::com_ptr<ID2D1Device> d2d;

            Device()
            {
                // Same hardware creation flags/factory type as MarkdownView. A
                // test-owned domain avoids modifying a live control's graphics.
                // Hardware failure is a failure, never a WARP qualification.
                winrt::check_hresult(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr,
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
                    d3d.put(), nullptr, nullptr));
                winrt::com_ptr<ID3D11DeviceContext> immediate;
                d3d->GetImmediateContext(immediate.put());
                auto protectedDevice = immediate.as<ID3D11Multithread>();
                protectedDevice->SetMultithreadProtected(TRUE);
                Require(protectedDevice->GetMultithreadProtected() != FALSE,
                    L"the actual D3D immediate context is multithread protected");
                D2D1_FACTORY_OPTIONS options{};
                winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, options, factory.put()));
                Require(factory.as<ID2D1Multithread>()->GetMultithreadProtected() != FALSE,
                    L"the actual D2D factory is multithread protected");
                winrt::check_hresult(factory->CreateDevice(d3d.as<IDXGIDevice>().get(), d2d.put()));
            }
        };

        // Independent copy of the frozen v3 TablePacketScene's compact geometry.
        // Each instance creates its own ReadingSourceState, renderer and layouts.
        // Create receives no InitialViewport, cache, resource snapshot or predecessor.
        // Neither prefix sharing nor Reposition is used. The renderer outlives all
        // frame allocations, including their allocator-backed control blocks.
        struct Scene
        {
            std::shared_ptr<PreviewCancellation> cancel;
            std::shared_ptr<ReadingSourceState> reading;
            std::shared_ptr<NativeDocumentRenderer> renderer;
            std::shared_ptr<PreviewFrame> frame;

            Scene(std::shared_ptr<ReadingMemory> const &memory, float dpi)
            {
                cancel = std::make_shared<PreviewCancellation>(memory->ModelOwner().Get());
                const auto token = NextReadingTextToken();
                reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                    memory, cancel, 23, token);
                reading->PublishMain(MainTextSnapshot::Build(memory, cancel->Get(), 23, token));
                notepads_markdown_preview_key key{};
                key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
                key.model = reading->Main()->Identity();
                key.layout_generation = key.theme_generation = key.measurement_generation = 1;
                key.width = 480;
                key.indent = 24;
                key.gap = 12;
                PreviewTheme theme;
                theme.fontSize = 9;
                theme.lineHeight = 12;
                theme.physicalScale = dpi / 96.0;
                renderer = NativeDocumentRenderer::Create({}, cancel, key, theme, reading);
                std::atomic<uint64_t> latest{1};
                frame = renderer->CreateFrame(0, 10000, 1, latest);
                Require(frame && frame->main && frame->Blocks().size() == 1 &&
                    frame->Blocks().front().block.kind == 6 && !frame->Blocks().front().htmlTable &&
                    frame->Blocks().front().texts.size() == 36 && frame->main->Text() == TablePacketFixtureMain,
                    L"final-surface fixture is the real, exact independently authored 36-cell Lab table");
                // Ordinary measurement collects a dormant prewarm plan. No Run
                // is admitted by this test; discard that plan before drawing.
                frame->textPrewarm.reset();
                reading->textPrewarm.reset();
                Require(!frame->textPrewarm && !reading->textPrewarm,
                    L"diagnostic drawing owns no text prewarm store");
                uint64_t units{};
                for (auto const &text : frame->Blocks().front().texts)
                {
                    Require(text.layout && text.matchesMainText && text.bitmapCandidate && text.utf16Length &&
                        !text.scriptDrawing && !text.scripts && text.links.empty() && text.inlineCode.empty() &&
                        text.images.empty() && text.math.empty() && text.diagrams.empty(),
                        L"each actual Lab cell is ordinary Main-matching text with no dynamic resource");
                    Require(text.logicalEnd - text.logicalStart == text.utf16Length &&
                        text.logicalEnd <= frame->main->Length(), L"cell text is backed by its real Main range");
                    DWRITE_TEXT_METRICS metrics{};
                    winrt::check_hresult(text.layout->GetMetrics(&metrics));
                    Require(metrics.height > 0 && metrics.layoutWidth > 0, L"each cell is freshly measured");
                    units += text.utf16Length;
                }
                Require(units == 443, L"unchanged Lab fixture contains exactly 443 UTF16 cell units");
            }
        };

        enum class Gate { None, FaultAfterUiBegin, CancelAfterUiSuspend, FaultBeforeWorkerCapture, CancelAfterWorkerSuspend };
        struct Paint
        {
            wchar_t const *name;
            float dpi{96};
            RECT update{0, 0, Width, Height};
            // Preserve separate DIP origin and context phases, not only their sum.
            float originX{}, originY{}, contextX{}, contextY{};
            Gate gate{Gate::None};
        };

        void Draw(PreviewFrame const &frame, ID2D1DeviceContext *context, Paint const &paint, POINT offset)
        {
            const float scale = paint.dpi / 96.0f;
            const float nativeX = (offset.x - paint.update.left) / scale;
            const float nativeY = (offset.y - paint.update.top) / scale;
            context->SetDpi(paint.dpi, paint.dpi);
            context->SetTransform(D2D1::Matrix3x2F::Translation(nativeX, nativeY));
            const auto update = D2D1::RectF(paint.update.left / scale, paint.update.top / scale,
                paint.update.right / scale, paint.update.bottom / scale);
            // Every same-lease pass uses identical physical update clipping.
            // This clip is integral in pixels and unchanged by paint phases.
            context->PushAxisAlignedClip(update, D2D1_ANTIALIAS_MODE_ALIASED);
            struct PopClip
            {
                ID2D1DeviceContext *context;
                ~PopClip() { context->PopAxisAlignedClip(); }
            } pop{context};
            const auto color = frame.theme.background;
            context->Clear(D2D1::ColorF(color.R / 255.0f, color.G / 255.0f, color.B / 255.0f, color.A / 255.0f));
            context->SetTransform(D2D1::Matrix3x2F::Translation(nativeX + paint.contextX, nativeY + paint.contextY));
            const auto clip = D2D1::RectF(update.left - paint.contextX, update.top - paint.contextY,
                update.right - paint.contextX, update.bottom - paint.contextY);
            NativeDocumentRenderer::Draw(frame, context, 16 + paint.originX, 16 + paint.originY,
                0, 0, {}, false, nullptr, false, {}, clip, nullptr, nullptr, false);
        }

        std::vector<uint8_t> Capture(ID2D1DeviceContext *context, ID2D1Bitmap1 *target,
            D2D1_RECT_U rectangle, bool activeBatch = true)
        {
            const auto width = rectangle.right - rectangle.left;
            const auto height = rectangle.bottom - rectangle.top;
            Require(width && height && uint64_t{width} * height * 4 <= PixelLimit,
                L"test-only readback stays inside its fixed four-MiB payload bound");
            const auto format = target->GetPixelFormat();
            Require(format.format == DXGI_FORMAT_B8G8R8A8_UNORM && format.alphaMode == D2D1_ALPHA_MODE_PREMULTIPLIED,
                L"non-opaque SIS uses the original premultiplied BGRA8 format");
            const auto size = target->GetPixelSize();
            Require(rectangle.right <= size.width && rectangle.bottom <= size.height,
                L"actual returned SIS offset identifies an in-bounds update rectangle");
            winrt::com_ptr<ID2D1Bitmap1> readback;
            auto properties = D2D1::BitmapProperties1(D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                format, 96, 96);
            winrt::check_hresult(context->CreateBitmap(D2D1::SizeU(width, height), nullptr, 0, properties, readback.put()));
            // This is solely a test oracle: no readback ever feeds XAML or a
            // production target. CopyFromBitmap may flush an active batch;
            // explicitly Flush first, while the SIS Begin/End lease is valid.
            // https://learn.microsoft.com/windows/win32/api/d2d1/nf-d2d1-id2d1bitmap-copyfrombitmap
            if (activeBatch) winrt::check_hresult(context->Flush());
            winrt::check_hresult(readback->CopyFromBitmap(nullptr, target, &rectangle));
            D2D1_MAPPED_RECT mapped{};
            winrt::check_hresult(readback->Map(D2D1_MAP_OPTIONS_READ, &mapped));
            struct Unmap
            {
                ID2D1Bitmap1 *bitmap;
                ~Unmap() { (void)bitmap->Unmap(); }
            } unmap{readback.get()};
            std::vector<uint8_t> result(static_cast<size_t>(width) * height * 4);
            for (UINT32 row = 0; row != height; ++row)
                std::memcpy(result.data() + static_cast<size_t>(row) * width * 4,
                    mapped.bits + static_cast<size_t>(row) * mapped.pitch, static_cast<size_t>(width) * 4);
            return result;
        }

        struct MtaScope
        {
            MtaScope()
            {
                winrt::init_apartment(winrt::apartment_type::multi_threaded);
            }
            ~MtaScope() { winrt::uninit_apartment(); }
        };

        void SameScene(Scene const &left, Scene const &right)
        {
            auto const &a = *left.frame;
            auto const &b = *right.frame;
            Require(left.renderer != right.renderer && left.reading != right.reading && a.main != b.main &&
                a.measuredBlocks != b.measuredBlocks, L"three scenes own independent renderer/Main/block trees");
            Require(a.main->Text() == b.main->Text() && a.key.width == b.key.width && a.key.indent == b.key.indent &&
                a.key.gap == b.key.gap && a.viewportY == b.viewportY && a.viewportHeight == b.viewportHeight &&
                a.theme.fontFamily == b.theme.fontFamily && a.theme.fontSize == b.theme.fontSize &&
                a.theme.lineHeight == b.theme.lineHeight && a.theme.padding == b.theme.padding &&
                a.theme.physicalScale == b.theme.physicalScale && a.theme.foreground == b.theme.foreground &&
                a.theme.background == b.theme.background && a.theme.borderColor == b.theme.borderColor &&
                a.theme.panelBackground == b.theme.panelBackground && a.theme.highContrast == b.theme.highContrast,
                L"three scenes have equal source, viewport, geometry and table paint theme");
            auto const &ab = a.Blocks().front();
            auto const &bb = b.Blocks().front();
            Require(ab.block.node == bb.block.node && ab.block.kind == bb.block.kind &&
                ab.block.logical_start == bb.block.logical_start && ab.block.logical_end == bb.block.logical_end &&
                ab.block.x == bb.block.x && ab.block.y == bb.block.y && ab.block.width == bb.block.width &&
                ab.block.height == bb.block.height && ab.block.columns == bb.block.columns &&
                ab.block.row_count == bb.block.row_count && ab.block.alignment == bb.block.alignment &&
                ab.measuredHeight == bb.measuredHeight && ab.tableWidth == bb.tableWidth &&
                ab.rowHeights == bb.rowHeights && ab.columnEdges == bb.columnEdges &&
                ab.tableOwners == bb.tableOwners && ab.tableRowFlags == bb.tableRowFlags,
                L"fresh table geometry is exactly equal");
            for (auto const &text : ab.texts)
                for (auto const &other : bb.texts)
                    Require(text.layout.get() != other.layout.get(), L"no mutable DWrite layout is shared across scenes");
            for (size_t index = 0; index != ab.texts.size(); ++index)
            {
                auto const &at = ab.texts[index];
                auto const &bt = bb.texts[index];
                Require(at.logicalStart == bt.logicalStart && at.logicalEnd == bt.logicalEnd &&
                    at.utf16Length == bt.utf16Length && at.terminalBreak == bt.terminalBreak &&
                    at.x == bt.x && at.y == bt.y && at.width == bt.width && at.height == bt.height,
                    L"fresh cell text ranges and origins are exactly equal");
                DWRITE_TEXT_METRICS am{}, bm{};
                DWRITE_OVERHANG_METRICS ao{}, bo{};
                winrt::check_hresult(at.layout->GetMetrics(&am));
                winrt::check_hresult(bt.layout->GetMetrics(&bm));
                winrt::check_hresult(at.layout->GetOverhangMetrics(&ao));
                winrt::check_hresult(bt.layout->GetOverhangMetrics(&bo));
                Require(am.left == bm.left && am.top == bm.top && am.width == bm.width &&
                    am.widthIncludingTrailingWhitespace == bm.widthIncludingTrailingWhitespace && am.height == bm.height &&
                    am.layoutWidth == bm.layoutWidth && am.layoutHeight == bm.layoutHeight &&
                    am.maxBidiReorderingDepth == bm.maxBidiReorderingDepth && am.lineCount == bm.lineCount &&
                    ao.left == bo.left && ao.top == bo.top && ao.right == bo.right && ao.bottom == bo.bottom,
                    L"fresh DirectWrite metrics and ink overhangs are exactly equal");
                for (UINT32 position = 0; position != at.utf16Length; ++position)
                {
                    DWRITE_FONT_WEIGHT aw{}, bw{};
                    DWRITE_FONT_STYLE as{}, bs{};
                    DWRITE_FONT_STRETCH ast{}, bst{};
                    FLOAT az{}, bz{};
                    BOOL au{}, bu{}, ak{}, bk{};
                    UINT32 an{}, bn{};
                    std::array<wchar_t, 128> af{}, bf{};
                    winrt::check_hresult(at.layout->GetFontWeight(position, &aw));
                    winrt::check_hresult(bt.layout->GetFontWeight(position, &bw));
                    winrt::check_hresult(at.layout->GetFontStyle(position, &as));
                    winrt::check_hresult(bt.layout->GetFontStyle(position, &bs));
                    winrt::check_hresult(at.layout->GetFontStretch(position, &ast));
                    winrt::check_hresult(bt.layout->GetFontStretch(position, &bst));
                    winrt::check_hresult(at.layout->GetFontSize(position, &az));
                    winrt::check_hresult(bt.layout->GetFontSize(position, &bz));
                    winrt::check_hresult(at.layout->GetUnderline(position, &au));
                    winrt::check_hresult(bt.layout->GetUnderline(position, &bu));
                    winrt::check_hresult(at.layout->GetStrikethrough(position, &ak));
                    winrt::check_hresult(bt.layout->GetStrikethrough(position, &bk));
                    winrt::check_hresult(at.layout->GetFontFamilyNameLength(position, &an));
                    winrt::check_hresult(bt.layout->GetFontFamilyNameLength(position, &bn));
                    Require(an < af.size() && bn < bf.size(), L"fixture font names fit the bounded style witness");
                    winrt::check_hresult(at.layout->GetFontFamilyName(position, af.data(), static_cast<UINT32>(af.size())));
                    winrt::check_hresult(bt.layout->GetFontFamilyName(position, bf.data(), static_cast<UINT32>(bf.size())));
                    Require(aw == bw && as == bs && ast == bst && az == bz && au == bu && ak == bk &&
                        std::wstring_view{af.data(), an} == std::wstring_view{bf.data(), bn},
                        L"every fresh cell UTF16 position has equal font/style/decoration");
                }
            }
        }

        struct Scenes
        {
            std::array<std::unique_ptr<Scene>, 3> values;
            struct Weak
            {
                std::weak_ptr<PreviewFrame> frame;
                std::weak_ptr<PreviewVector<PreviewBlockVisual> const> blocks;
                std::weak_ptr<MainTextSnapshot const> main;
            };
            // Weak allocator-backed control blocks retire before renderers.
            std::array<Weak, 3> weak;
            Scenes(std::shared_ptr<ReadingMemory> const &memory, float dpi)
            {
                for (size_t index = 0; index != values.size(); ++index)
                {
                    values[index] = std::make_unique<Scene>(memory, dpi);
                    weak[index] = {values[index]->frame, values[index]->frame->measuredBlocks, values[index]->frame->main};
                }
                SameScene(*values[0], *values[1]);
                SameScene(*values[0], *values[2]);
                SameScene(*values[1], *values[2]);
            }
            bool Retire() noexcept
            {
                bool released = true;
                for (size_t index = 0; index != values.size(); ++index)
                {
                    if (!values[index]) continue;
                    auto &scene = *values[index];
                    auto &owner = weak[index];
                    scene.frame.reset();
                    released = owner.frame.expired() && owner.blocks.expired() && released;
                    owner.frame.reset();
                    owner.blocks.reset();
                    scene.renderer.reset();
                    scene.reading.reset();
                    scene.cancel.reset();
                    released = owner.main.expired() && released;
                    owner.main.reset();
                    values[index].reset();
                }
                return released;
            }
        };

        enum class Phase { NotBegun, UiActive, UiSuspended, WorkerActive, WorkerSuspended, SuspensionFailed, Ended };
        struct Call
        {
            DWORD thread{};
            uint32_t attempts{}, successes{};
            HRESULT status{S_OK};
            void Start() noexcept { thread = GetCurrentThreadId(); ++attempts; status = E_PENDING; }
            void Finish(HRESULT result) noexcept { status = result; successes += SUCCEEDED(result); }
        };
        HRESULT CurrentStatus() noexcept
        {
            try { throw; }
            catch (winrt::hresult_error const &error) { return error.code(); }
            catch (...) { return E_FAIL; }
        }
        struct Delta
        {
            bool checked{}, sameSize{}, equal{};
            size_t count{}, first{SIZE_MAX};
            unsigned maximum{};
            int actual{-1}, expected{-1};
        };
        Delta Difference(std::vector<uint8_t> const &actual, std::vector<uint8_t> const &expected) noexcept
        {
            Delta result;
            result.checked = true;
            result.sameSize = actual.size() == expected.size();
            result.equal = actual == expected;
            for (size_t index = 0; index != std::min(actual.size(), expected.size()); ++index)
            {
                const auto change = static_cast<unsigned>(std::abs(int{actual[index]} - int{expected[index]}));
                result.maximum = std::max(result.maximum, change);
                if (!change) continue;
                ++result.count;
                if (result.first == SIZE_MAX)
                {
                    result.first = index;
                    result.actual = actual[index];
                    result.expected = expected[index];
                }
            }
            if (!result.sameSize)
            {
                const auto common = std::min(actual.size(), expected.size());
                result.count += std::max(actual.size(), expected.size()) - common;
                if (result.first == SIZE_MAX)
                {
                    result.first = common;
                    if (common < actual.size()) result.actual = actual[common];
                    if (common < expected.size()) result.expected = expected[common];
                }
            }
            return result;
        }
        struct Trace
        {
            Phase phase{Phase::NotBegun};
            DWORD ui{}, prepareThread{}, workerThread{};
            Call begin, uiSuspend, resume, workerSuspend, end;
            std::array<Call, 3> draw, capture;
            POINT offset{};
            D2D1_SIZE_U extent{};
            uintptr_t deviceId{}, targetId{};
            D2D1_RENDERING_CONTROLS controls{};
            D2D1_PIXEL_FORMAT format{};
            FLOAT bitmapDpiX{}, bitmapDpiY{};
            uint32_t bitmapOptions{}, units{}, blend{}, antialias{}, textAntialias{};
            bool colorContext{};
            uint32_t uiIdentity{}, workerIdentity{};
            uint32_t statesRestored{};
            Delta uiPixels, workerPixels;
            bool injected{}, cancelled{}, framesReleased{}, hostUnchanged{}, mta{};
            std::exception_ptr failure, hostFailure;
            wchar_t const *stage{L"prepare"};
            wchar_t const *failureStage{L"none"};
        };
        void RememberFailure(Trace &trace) noexcept
        {
            if (!trace.failure)
            {
                trace.failure = std::current_exception();
                trace.failureStage = trace.stage;
            }
        }

        struct ScalarState
        {
            D2D1_UNIT_MODE units;
            D2D1_PRIMITIVE_BLEND blend;
            D2D1_ANTIALIAS_MODE antialias;
            D2D1_TEXT_ANTIALIAS_MODE textAntialias;
            D2D1_RENDERING_CONTROLS controls{};
            D2D1_MATRIX_3X2_F transform{};
            FLOAT dpiX{}, dpiY{};
            D2D1_TAG tag1{}, tag2{};
            winrt::com_ptr<IDWriteRenderingParams> parameters;
            explicit ScalarState(ID2D1DeviceContext *context)
                : units(context->GetUnitMode()), blend(context->GetPrimitiveBlend()),
                  antialias(context->GetAntialiasMode()), textAntialias(context->GetTextAntialiasMode())
            {
                context->GetRenderingControls(&controls);
                context->GetTransform(&transform);
                context->GetDpi(&dpiX, &dpiY);
                context->GetTags(&tag1, &tag2);
                context->GetTextRenderingParams(parameters.put());
                Require(units == D2D1_UNIT_MODE_DIPS && blend == D2D1_PRIMITIVE_BLEND_SOURCE_OVER,
                    L"original SIS context uses DIP/source-over state");
            }
            void Restore(ID2D1DeviceContext *context) const
            {
                context->SetUnitMode(units);
                context->SetPrimitiveBlend(blend);
                context->SetAntialiasMode(antialias);
                context->SetTextAntialiasMode(textAntialias);
                context->SetTextRenderingParams(parameters.get());
                context->SetRenderingControls(&controls);
                context->SetDpi(dpiX, dpiY);
                context->SetTransform(transform);
                context->SetTags(tag1, tag2);
                ScalarState check{context};
                Require(check.units == units && check.blend == blend && check.antialias == antialias &&
                    check.textAntialias == textAntialias && check.parameters.get() == parameters.get() &&
                    check.controls.bufferPrecision == controls.bufferPrecision &&
                    check.controls.tileSize.width == controls.tileSize.width && check.controls.tileSize.height == controls.tileSize.height &&
                    check.dpiX == dpiX && check.dpiY == dpiY && check.tag1 == tag1 && check.tag2 == tag2 &&
                    check.transform._11 == transform._11 && check.transform._12 == transform._12 &&
                    check.transform._21 == transform._21 && check.transform._22 == transform._22 &&
                    check.transform._31 == transform._31 && check.transform._32 == transform._32,
                    L"captured scalar drawing state was not restored exactly");
            }
        };

        struct TargetIdentity
        {
            winrt::com_ptr<IUnknown> device, target, color;
            D2D1_SIZE_U extent{};
            D2D1_PIXEL_FORMAT format{};
            D2D1_BITMAP_OPTIONS options{};
            FLOAT dpiX{}, dpiY{};
        };
        TargetIdentity Observe(ID2D1DeviceContext *context, winrt::com_ptr<ID2D1Bitmap1> &bitmap)
        {
            TargetIdentity result;
            winrt::com_ptr<ID2D1Device> device;
            context->GetDevice(device.put());
            result.device = device.as<IUnknown>();
            winrt::com_ptr<ID2D1Image> image;
            context->GetTarget(image.put());
            Require(image != nullptr, L"active same-lease SIS has an actual target");
            bitmap = image.as<ID2D1Bitmap1>();
            result.target = bitmap.as<IUnknown>();
            result.extent = bitmap->GetPixelSize();
            result.format = bitmap->GetPixelFormat();
            result.options = bitmap->GetOptions();
            bitmap->GetDpi(&result.dpiX, &result.dpiY);
            winrt::com_ptr<ID2D1ColorContext> color;
            bitmap->GetColorContext(color.put());
            if (color) result.color = color.as<IUnknown>();
            return result;
        }

        struct NativeLease
        {
            winrt::com_ptr<ISurfaceImageSourceNativeWithD2D> native;
            winrt::com_ptr<ID2D1DeviceContext> context;
            std::optional<ScalarState> state;
            TargetIdentity original;
            RECT update{};
            POINT offset{};
            void Begin(Paint const &paint, Trace &trace)
            {
                Require(GetCurrentThreadId() == trace.ui && trace.phase == Phase::NotBegun && !context,
                    L"one native lease begins only on its owning UI thread");
                update = paint.update;
                offset = {};
                trace.begin.Start();
                trace.begin.Finish(native->BeginDraw(update, __uuidof(ID2D1DeviceContext), context.put_void(), &offset));
                if (SUCCEEDED(trace.begin.status)) trace.phase = Phase::UiActive;
                trace.offset = offset;
                winrt::check_hresult(trace.begin.status);
            }
            void Freeze(Device const &device, Trace &trace)
            {
                state.emplace(context.get());
                winrt::com_ptr<ID2D1Bitmap1> bitmap;
                original = Observe(context.get(), bitmap);
                Require(original.device.get() == device.d2d.as<IUnknown>().get(), L"SIS uses the configured protected device");
                trace.extent = original.extent;
                trace.deviceId = reinterpret_cast<uintptr_t>(original.device.get());
                trace.targetId = reinterpret_cast<uintptr_t>(original.target.get());
                trace.controls = state->controls;
                trace.format = original.format;
                trace.bitmapDpiX = original.dpiX;
                trace.bitmapDpiY = original.dpiY;
                trace.bitmapOptions = static_cast<uint32_t>(original.options);
                trace.units = static_cast<uint32_t>(state->units);
                trace.blend = static_cast<uint32_t>(state->blend);
                trace.antialias = static_cast<uint32_t>(state->antialias);
                trace.textAntialias = static_cast<uint32_t>(state->textAntialias);
                trace.colorContext = original.color != nullptr;
            }
            void Suspend(Trace &trace) noexcept
            {
                const auto phase = trace.phase;
                if (phase != Phase::UiActive && phase != Phase::WorkerActive) return;
                auto &call = phase == Phase::UiActive ? trace.uiSuspend : trace.workerSuspend;
                if (call.attempts) return;
                call.Start();
                call.Finish(native->SuspendDraw());
                if (FAILED(call.status) && !trace.failure)
                    trace.failureStage = phase == Phase::UiActive ? L"UI SuspendDraw" : L"worker SuspendDraw";
                trace.phase = FAILED(call.status) ? Phase::SuspensionFailed :
                    phase == Phase::UiActive ? Phase::UiSuspended : Phase::WorkerSuspended;
            }
            void Resume(Trace &trace)
            {
                Require(trace.phase == Phase::UiSuspended && GetCurrentThreadId() != trace.ui,
                    L"worker resumes only the suspended UI-begun lease");
                trace.resume.Start();
                trace.resume.Finish(native->ResumeDraw());
                if (SUCCEEDED(trace.resume.status)) trace.phase = Phase::WorkerActive;
                winrt::check_hresult(trace.resume.status);
            }
            uint32_t IdentityMask(TargetIdentity const &now, Paint const &paint, Trace const &trace) const noexcept
            {
                return (now.device.get() == original.device.get() ? 1u : 0u) |
                    (now.target.get() == original.target.get() ? 2u : 0u) |
                    (now.extent.width == original.extent.width && now.extent.height == original.extent.height ? 4u : 0u) |
                    (now.format.format == original.format.format && now.format.alphaMode == original.format.alphaMode ? 8u : 0u) |
                    (now.dpiX == original.dpiX && now.dpiY == original.dpiY ? 16u : 0u) |
                    (now.options == original.options ? 32u : 0u) |
                    (now.color.get() == original.color.get() ? 64u : 0u) |
                    (update.left == paint.update.left && update.top == paint.update.top && update.right == paint.update.right &&
                        update.bottom == paint.update.bottom && offset.x == trace.offset.x && offset.y == trace.offset.y ? 128u : 0u);
            }
            std::vector<uint8_t> Pass(PreviewFrame const &frame, Paint const &paint, size_t pass, Trace &trace)
            {
                Require((pass < 2 && trace.phase == Phase::UiActive && GetCurrentThreadId() == trace.ui) ||
                    (pass == 2 && trace.phase == Phase::WorkerActive && GetCurrentThreadId() == trace.workerThread),
                    L"only the active lease owner accesses drawing state");
                trace.stage = pass < 2 ? L"UI identity/state" : L"worker identity/state";
                winrt::com_ptr<ID2D1Bitmap1> bitmap;
                const auto now = Observe(context.get(), bitmap);
                auto &mask = pass < 2 ? trace.uiIdentity : trace.workerIdentity;
                mask = IdentityMask(now, paint, trace);
                Require(mask == 255, L"same-lease target/device/configuration identity changed");
                state->Restore(context.get());
                ++trace.statesRestored;
                trace.stage = pass < 2 ? L"UI Draw" : L"worker Draw";
                trace.draw[pass].Start();
                try { Draw(frame, context.get(), paint, offset); }
                catch (...)
                {
                    trace.draw[pass].status = CurrentStatus();
                    throw;
                }
                trace.draw[pass].Finish(S_OK);
                if (pass == 2 && paint.gate == Gate::FaultBeforeWorkerCapture)
                    throw Injected{};
                trace.stage = pass < 2 ? L"UI capture" : L"worker capture";
                Require(offset.x >= 0 && offset.y >= 0, L"same-lease update has nonnegative actual offset");
                const auto right = uint64_t{static_cast<UINT32>(offset.x)} + update.right - update.left;
                const auto bottom = uint64_t{static_cast<UINT32>(offset.y)} + update.bottom - update.top;
                Require(right <= UINT32_MAX && bottom <= UINT32_MAX, L"same-lease update bounds are representable");
                trace.capture[pass].Start();
                std::vector<uint8_t> pixels;
                try
                {
                    pixels = Capture(context.get(), bitmap.get(), D2D1::RectU(static_cast<UINT32>(offset.x),
                        static_cast<UINT32>(offset.y), static_cast<UINT32>(right), static_cast<UINT32>(bottom)));
                }
                catch (...)
                {
                    trace.capture[pass].status = CurrentStatus();
                    throw;
                }
                trace.capture[pass].Finish(S_OK);
                return pixels;
            }
            void End(Trace &trace) noexcept
            {
                if (!trace.begin.successes || trace.end.attempts) return;
                // Release all borrowed draw objects inside this lease, before
                // native EndDraw. No context access occurs while suspended.
                context = nullptr;
                state.reset();
                original = {};
                trace.end.Start();
                trace.end.Finish(native->EndDraw());
                if (FAILED(trace.end.status) && !trace.failure) trace.failureStage = L"UI EndDraw";
                trace.phase = Phase::Ended;
            }
            struct Injected {};
        };
        struct ActiveScope
        {
            NativeLease &lease;
            Trace &trace;
            ~ActiveScope() { lease.Suspend(trace); } // Nonallocating, even on a thrown allocation failure.
        };
        struct UiOwner
        {
            ModuleLifetime module;
            Window window{nullptr};
            UIElement content{nullptr};
            SurfaceImageSource surface{nullptr};
            NativeLease lease;
            explicit UiOwner(Device const &device)
            {
                window = Window::Current();
                Require(window && window.Visible() && window.CoreWindow().ActivationMode() !=
                    winrt::Windows::UI::Core::CoreWindowActivationMode::Deactivated,
                    L"UI-begun SIS proof requires a visible active test window");
                content = window.Content();
                surface = SurfaceImageSource(Width, Height, false);
                lease.native = surface.as<ISurfaceImageSourceNativeWithD2D>();
                winrt::check_hresult(lease.native->SetDevice(device.d2d.get()));
            }
            void CheckHost(Trace &trace) noexcept
            {
                try { trace.hostUnchanged = window.Content() == content; }
                catch (...) { trace.hostFailure = std::current_exception(); }
            }
        };

        void UiReference(UiOwner &owner, Device const &device, Scenes const &scenes, Paint const &paint,
            std::array<std::vector<uint8_t>, 3> &pixels, Trace &trace) noexcept
        {
            ActiveScope active{owner.lease, trace};
            try
            {
                trace.stage = L"UI BeginDraw";
                owner.lease.Begin(paint, trace);
                if (paint.gate == Gate::FaultAfterUiBegin) throw NativeLease::Injected{};
                owner.lease.Freeze(device, trace);
                pixels[0] = owner.lease.Pass(*scenes.values[0]->frame, paint, 0, trace);
                Require(std::any_of(pixels[0].begin(), pixels[0].end(), [](uint8_t value) { return value != 255; }),
                    L"UI reference contains actual table and glyph ink");
                pixels[1] = owner.lease.Pass(*scenes.values[1]->frame, paint, 1, trace);
                trace.stage = L"UI-repeat bytes";
                trace.uiPixels = Difference(pixels[0], pixels[1]);
                Require(trace.uiPixels.equal, L"UI U0 differs from UI U1; reference unstable, no worker claim");
            }
            catch (NativeLease::Injected const &) { trace.injected = true; }
            catch (...) { RememberFailure(trace); }
        }

        void WorkerResume(NativeLease &lease, Scenes const &scenes, Paint const &paint,
            std::atomic<bool> const &cancelRequested, std::array<std::vector<uint8_t>, 3> &pixels, Trace &trace) noexcept
        {
            try
            {
                trace.stage = L"worker MTA";
                MtaScope apartment;
                ActiveScope active{lease, trace}; // Suspend before uninitializing this worker's MTA scope.
                trace.workerThread = GetCurrentThreadId();
                Require(trace.workerThread != trace.ui, L"worker resume actually executes off UI");
                APTTYPE type{};
                APTTYPEQUALIFIER qualifier{};
                winrt::check_hresult(CoGetApartmentType(&type, &qualifier));
                trace.mta = type == APTTYPE_MTA;
                Require(trace.mta, L"resumed drawing has an actual MTA apartment");
                if (cancelRequested.load(std::memory_order_acquire))
                {
                    trace.cancelled = true;
                    return;
                }
                trace.stage = L"worker ResumeDraw";
                lease.Resume(trace);
                pixels[2] = lease.Pass(*scenes.values[2]->frame, paint, 2, trace);
                trace.stage = L"worker/UI bytes";
                trace.workerPixels = Difference(pixels[2], pixels[0]);
                Require(trace.workerPixels.equal && pixels[2] == pixels[1],
                    L"worker W differs from stable UI U0/U1 in the same native lease");
            }
            catch (NativeLease::Injected const &) { trace.injected = true; }
            catch (...) { RememberFailure(trace); }
        }

        void Validate(Trace const &trace, Paint const &paint)
        {
            Require(trace.framesReleased, L"all fresh frames, block trees and Main owners retired");
            Require(trace.hostUnchanged && !trace.hostFailure, L"original Window.Content changed or could not be checked");
            Require(trace.begin.attempts == 1 && trace.begin.successes == 1 && trace.begin.thread == trace.ui &&
                trace.end.attempts == 1 && trace.end.successes == 1 && trace.end.thread == trace.ui,
                L"exactly one successful UI BeginDraw receives exactly one successful UI EndDraw");
            Require(trace.uiSuspend.attempts == 1 && trace.uiSuspend.successes == 1 && trace.uiSuspend.thread == trace.ui,
                L"UI suspension succeeds exactly once on UI");
            if (trace.workerSuspend.attempts)
                Require(trace.workerSuspend.successes == 1 && trace.workerSuspend.thread == trace.workerThread,
                    L"worker suspension succeeds on the actual worker");
            if (trace.failure) std::rethrow_exception(trace.failure);
            Require(trace.prepareThread != trace.ui, L"all three independent scenes are built off UI before BeginDraw");
            if (paint.gate == Gate::FaultAfterUiBegin)
            {
                Require(trace.injected && !trace.draw[0].attempts && !trace.resume.attempts && !trace.workerSuspend.attempts,
                    L"post-Begin fault is suspended and ended without optional passes");
                return;
            }
            Require(trace.uiPixels.equal && trace.uiIdentity == 255 &&
                trace.draw[0].attempts == 1 && trace.draw[1].attempts == 1 &&
                trace.capture[0].attempts == 1 && trace.capture[1].attempts == 1 &&
                trace.draw[0].successes == 1 && trace.draw[1].successes == 1 &&
                trace.capture[0].successes == 1 && trace.capture[1].successes == 1 &&
                trace.draw[0].thread == trace.ui && trace.draw[1].thread == trace.ui &&
                trace.capture[0].thread == trace.ui && trace.capture[1].thread == trace.ui,
                L"two independent original UI passes give exactly equal bytes on the original target");
            if (paint.gate == Gate::CancelAfterUiSuspend)
            {
                Require(trace.cancelled && trace.mta && trace.workerThread != trace.ui && trace.statesRestored == 2 &&
                    !trace.resume.attempts && !trace.draw[2].attempts && !trace.workerSuspend.attempts,
                    L"cancelled suspended lease ends without worker resume/draw");
                return;
            }
            Require(trace.mta && trace.workerThread != trace.ui && trace.resume.attempts == 1 &&
                trace.resume.successes == 1 && trace.resume.thread == trace.workerThread && trace.workerIdentity == 255 &&
                trace.statesRestored == 3 && trace.workerSuspend.attempts == 1 && trace.workerSuspend.successes == 1 &&
                trace.draw[2].attempts == 1 && trace.draw[2].successes == 1 && trace.draw[2].thread == trace.workerThread,
                L"one MTA worker resumes, restores exact state, draws and suspends the same lease");
            if (paint.gate == Gate::FaultBeforeWorkerCapture)
                Require(trace.injected && !trace.capture[2].attempts, L"post-resume draw fault skips capture and still terminates");
            else
            {
                Require(trace.workerPixels.equal && trace.capture[2].attempts == 1 && trace.capture[2].successes == 1 &&
                    trace.capture[2].thread == trace.workerThread,
                    L"resumed worker bytes exactly equal both stable UI references");
                Require(trace.cancelled == (paint.gate == Gate::CancelAfterWorkerSuspend),
                    L"only the authored post-worker-suspend gate cancels terminal work");
            }
        }

        std::wstring DeltaText(Delta const &delta, Paint const &paint)
        {
            if (!delta.checked) return L"?";
            auto result = std::to_wstring(delta.count) + L"/" + std::to_wstring(delta.maximum);
            if (!delta.sameSize) result += L"!len";
            if (delta.first != SIZE_MAX)
            {
                const auto pixel = delta.first / 4;
                const auto width = static_cast<size_t>(paint.update.right - paint.update.left);
                result += L"@" + std::to_wstring(pixel % width) + L"," + std::to_wstring(pixel / width) + L",";
                result += L"BGRA"[delta.first % 4];
                result += L"=" + std::to_wstring(delta.actual) + L"," + std::to_wstring(delta.expected);
            }
            return result;
        }
        std::wstring Prefix(Trace const &trace, Paint const &paint)
        {
            return L"UIBegin/WorkerResume d=" + std::to_wstring(static_cast<UINT32>(paint.dpi)) +
                L" r=" + std::to_wstring(paint.update.left) + L"," + std::to_wstring(paint.update.top) + L"," +
                std::to_wstring(paint.update.right) + L"," + std::to_wstring(paint.update.bottom) +
                L" o=" + std::to_wstring(trace.offset.x) + L"," + std::to_wstring(trace.offset.y) +
                L" n=" + std::to_wstring(trace.extent.width) + L"x" + std::to_wstring(trace.extent.height) +
                L" UI=" + DeltaText(trace.uiPixels, paint) + L" W=" + DeltaText(trace.workerPixels, paint) +
                L" id=" + std::to_wstring(trace.uiIdentity) + L"/" + std::to_wstring(trace.workerIdentity) +
                L" hr(B/Su/R/Sw/E)=" + std::to_wstring(trace.begin.status) + L"/" + std::to_wstring(trace.uiSuspend.status) +
                L"/" + std::to_wstring(trace.resume.status) + L"/" + std::to_wstring(trace.workerSuspend.status) + L"/" +
                std::to_wstring(trace.end.status) + L" E=" + std::to_wstring(trace.end.attempts) +
                L" host=" + std::to_wstring(trace.hostUnchanged && !trace.hostFailure) + L" phase=" + std::to_wstring(static_cast<unsigned>(trace.phase)) +
                L" stage=" + trace.failureStage + L"; ";
        }
        std::wstring CallText(Call const &call)
        {
            return std::to_wstring(call.thread) + L":" + std::to_wstring(call.attempts) + L"/" +
                std::to_wstring(call.successes) + L":" + std::to_wstring(call.status);
        }
        std::wstring Evidence(Trace const &trace, Paint const &paint)
        {
            auto result = Prefix(trace, paint) + paint.name + L" dpi=" + std::to_wstring(paint.dpi) +
                L" offset=" + std::to_wstring(trace.offset.x) + L"," + std::to_wstring(trace.offset.y) +
                L" target=" + std::to_wstring(trace.extent.width) + L"x" + std::to_wstring(trace.extent.height) +
                L" COMdevice/target=" + std::to_wstring(trace.deviceId) + L"/" + std::to_wstring(trace.targetId) +
                L" prepare=" + std::to_wstring(trace.prepareThread) + L" B=" + CallText(trace.begin) +
                L" Su=" + CallText(trace.uiSuspend) + L" R=" + CallText(trace.resume) +
                L" Sw=" + CallText(trace.workerSuspend) + L" E=" + CallText(trace.end);
            for (size_t index = 0; index != 3; ++index)
                result += L" pass" + std::to_wstring(index) + L"draw=" + CallText(trace.draw[index]) + L"capture=" + CallText(trace.capture[index]);
            return result + L" controls(precision,tile)=" + std::to_wstring(static_cast<unsigned>(trace.controls.bufferPrecision)) +
                L"," + std::to_wstring(trace.controls.tileSize.width) + L"," + std::to_wstring(trace.controls.tileSize.height) +
                L" state(units,blend,AA,textAA)=" + std::to_wstring(trace.units) + L"," + std::to_wstring(trace.blend) +
                L"," + std::to_wstring(trace.antialias) + L"," + std::to_wstring(trace.textAntialias) +
                L" bitmap(format,alpha,options,DPIx,DPIy,color)=" + std::to_wstring(static_cast<unsigned>(trace.format.format)) +
                L"," + std::to_wstring(static_cast<unsigned>(trace.format.alphaMode)) + L"," + std::to_wstring(trace.bitmapOptions) +
                L"," + std::to_wstring(trace.bitmapDpiX) + L"," + std::to_wstring(trace.bitmapDpiY) + L"," +
                std::to_wstring(trace.colorContext) + L" stateRestores=" + std::to_wstring(trace.statesRestored) + L" retired=" +
                std::to_wstring(trace.framesReleased) + L" cancelled=" + std::to_wstring(trace.cancelled) +
                L" injected=" + std::to_wstring(trace.injected);
        }
    }

    winrt::Windows::Foundation::IAsyncAction RunAsync(std::shared_ptr<ReadingMemory> memory,
        CoreDispatcher dispatcher, std::wstring &detail, std::exception_ptr &failure)
    {
        ModuleLifetime module;
        detail.clear();
        failure = nullptr;
        std::unique_ptr<Device> device;
        std::unique_ptr<UiOwner> owner;
        std::unique_ptr<Scenes> scenes;
        std::array<std::vector<uint8_t>, 3> pixels;
        std::atomic<bool> cancelRequested{};
        Trace trace;
        Paint current{L"setup"};
        DWORD uiThread{};
        uint32_t passed{};
        try
        {
            Require(memory && dispatcher && dispatcher.HasThreadAccess(), L"UI-begun proof requires UI entry and real ReadingMemory");
            uiThread = GetCurrentThreadId();
            device = std::make_unique<Device>();
            owner = std::make_unique<UiOwner>(*device);
            std::vector<Paint> paints;
            paints.push_back({L"full-initial-update", 96});
            for (float dpi : {96.0f, 192.0f})
            {
                paints.push_back({L"fractional-DIP-origin", dpi, {0, 0, Width, Height}, .25f, .125f});
                paints.push_back({L"fractional-DIP-context", dpi, {31, 11, 1007, 501}, 0, 0, .25f, .125f});
                paints.push_back({L"partial-update", dpi, {31, 11, 1007, 501}});
                paints.push_back({L"full-after-partial", dpi});
                paints.push_back({L"three-physical-pixel-context", dpi, {0, 0, Width, Height}, 0, 0, 3 * 96.0f / dpi, 3 * 96.0f / dpi});
                paints.push_back({L"same-DIP-phase-plus-two-physical-pixels", dpi, {0, 0, Width, Height}, 0, 0, 5 * 96.0f / dpi, 5 * 96.0f / dpi});
            }
            for (auto gate : {Gate::FaultAfterUiBegin, Gate::CancelAfterUiSuspend,
                Gate::FaultBeforeWorkerCapture, Gate::CancelAfterWorkerSuspend})
            {
                paints.push_back({L"same-lease-fault-or-cancel", 96, {0, 0, Width, Height}, 0, 0, 0, 0, gate});
                paints.push_back({L"same-SIS-success-after-cleanup", 96});
            }
            for (auto const &paint : paints)
            {
                current = paint;
                trace = {};
                trace.ui = uiThread;
                pixels = {};
                cancelRequested.store(false, std::memory_order_release);
                co_await winrt::resume_background();
                {
                    MtaScope apartment;
                    trace.prepareThread = GetCurrentThreadId();
                    Require(trace.prepareThread != uiThread, L"scene preparation executes off UI");
                    scenes = std::make_unique<Scenes>(memory, paint.dpi);
                }
                co_await winrt::resume_foreground(dispatcher);
                Require(GetCurrentThreadId() == uiThread, L"UI BeginDraw returns to the original dispatcher thread");
                UiReference(*owner, *device, *scenes, paint, pixels, trace);
                if (!trace.failure && !trace.injected && trace.phase == Phase::UiSuspended)
                {
                    if (paint.gate == Gate::CancelAfterUiSuspend)
                        cancelRequested.store(true, std::memory_order_release);
                    auto &lease = owner->lease;
                    co_await winrt::resume_background();
                    WorkerResume(lease, *scenes, paint, cancelRequested, pixels, trace);
                    co_await winrt::resume_foreground(dispatcher);
                    Require(GetCurrentThreadId() == uiThread, L"terminal native completion returns to UI");
                    if (paint.gate == Gate::CancelAfterWorkerSuspend && trace.phase == Phase::WorkerSuspended)
                        cancelRequested.store(true, std::memory_order_release);
                }
                trace.cancelled = cancelRequested.load(std::memory_order_acquire);
                owner->lease.End(trace);
                trace.framesReleased = scenes->Retire();
                scenes.reset();
                owner->CheckHost(trace);
                Validate(trace, paint);
                detail += L"\n" + Evidence(trace, paint);
                ++passed;
            }
        }
        catch (...)
        {
            failure = trace.failure ? trace.failure : std::current_exception();
        }

        // No allocation or diagnostic formatting precedes mandatory completion.
        std::exception_ptr dispatchFailure;
        bool unsupportedShutdown{}, cleanupFailed{};
        bool onUi = uiThread && GetCurrentThreadId() == uiThread;
        if (owner && !onUi)
        {
            try
            {
                co_await winrt::resume_foreground(dispatcher);
                onUi = GetCurrentThreadId() == uiThread;
            }
            catch (...) { dispatchFailure = std::current_exception(); }
        }
        if (owner && onUi)
        {
            owner->lease.End(trace);
            owner->CheckHost(trace);
            cleanupFailed = (trace.begin.successes && (trace.end.attempts != 1 || trace.end.successes != 1)) ||
                (trace.uiSuspend.attempts && !trace.uiSuspend.successes) ||
                (trace.workerSuspend.attempts && !trace.workerSuspend.successes) || !trace.hostUnchanged || trace.hostFailure;
            owner.reset(); // XAML and all retained native lease references release on UI.
        }
        else if (owner)
        {
            // Unsupported, never a qualified cleanup. Retain only the minimal
            // XAML/native lease/module owner; it owns no scene/model/tree. An
            // accepted then abandoned dispatch can also strand this coroutine.
            (void)owner.release();
            unsupportedShutdown = true;
        }
        if (scenes)
        {
            trace.framesReleased = scenes->Retire();
            scenes.reset();
        }
        pixels = {};
        memory.reset();
        device.reset();
        if (!failure && trace.hostFailure) failure = trace.hostFailure;
        if (!failure && dispatchFailure) failure = dispatchFailure;
        auto const primaryFailure = failure;
        try
        {
            if (unsupportedShutdown)
                failure = std::make_exception_ptr(Failure{L"UNSUPPORTED dispatcher shutdown; UI EndDraw/release not qualified; " + Describe(failure)});
            else if (cleanupFailed && !failure)
                failure = std::make_exception_ptr(Failure{L"same-lease UI cleanup failed"});
            if (failure)
                failure = std::make_exception_ptr(Failure{Prefix(trace, current) + current.name + L": " + Describe(failure)});
            detail += L"\nUI-begun/worker-resumed passed=" + std::to_wstring(passed) + L"/21; SIS=2097152 bytes; "
                L"one update readback <=4194304 bytes, three CPU captures and fresh Scenes may coexist; "
                L"no independent atlas canvas; native atlas allocation is opaque; Window.Content never assigned. "
                L"UI U0/U1 references and readback prevent performance claims. Worker BeginDraw, shutdown, production "
                L"publication, screen pixels, device loss, retained residency and cold latency remain unqualified.";
            if (failure) detail += L"\nfailure=" + Describe(failure) + L"; " + Evidence(trace, current);
        }
        catch (...) { if (!failure) failure = std::current_exception(); }
        if (primaryFailure)
        {
            try { std::rethrow_exception(failure); }
            catch (Failure const &) { }
            catch (...) { failure = primaryFailure; }
        }
        co_return;
    }
}
