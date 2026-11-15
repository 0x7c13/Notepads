// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeMathGeometryTests.h"
#include "Resources/Math/NativeMathGeometry.h"
#include <wincodec.h>
#include <array>
#include <cwchar>
#include <string_view>

namespace Notepads::Markdown::Native::MathGeometryTests
{
    namespace
    {
        void Require(bool condition, wchar_t const *detail)
        {
            if (!condition)
                throw winrt::hresult_error(E_FAIL, detail);
        }

        bool Near(double actual, double expected) noexcept
        {
            return std::abs(actual - expected) < 0.002;
        }

        using V = MathPathVerb;
        // Original numeric fixtures, not imported font outlines or math oracles.
        constexpr std::array hole{MathPathCommand{V::Move, 0, -2},     MathPathCommand{V::Line, 2, -2},
                                  MathPathCommand{V::Line, 2, 0},      MathPathCommand{V::Line, 0, 0},
                                  MathPathCommand{V::Close},           MathPathCommand{V::Move, 0.5, -1.5},
                                  MathPathCommand{V::Line, 0.5, -0.5}, MathPathCommand{V::Line, 1.5, -0.5},
                                  MathPathCommand{V::Line, 1.5, -1.5}, MathPathCommand{V::Close}};
        constexpr std::array protruding{MathPathCommand{V::Move, -0.5, -2}, MathPathCommand{V::Line, 2.5, -2},
                                        MathPathCommand{V::Line, 2.5, 1}, MathPathCommand{V::Line, -0.5, 1},
                                        MathPathCommand{V::Close}};

        std::shared_ptr<NativeMathDrawing> Drawing(std::shared_ptr<ReadingMemory> const &memory, ID2D1Factory *factory,
                                                   std::span<MathPathCommand const> commands,
                                                   MathPrimitive primitive = {}, double ascent = 2)
        {
            primitive.count = static_cast<uint32_t>(commands.size());
            return NativeMathDrawing::Create(memory, factory, {2, ascent, 0, {}, {&primitive, 1}, commands});
        }

        class Canvas
        {
            static constexpr uint32_t Extent = 160;
            winrt::com_ptr<IWICBitmap> _bitmap;
            ReadingVector<uint8_t> _pixels;

          public:
            winrt::com_ptr<ID2D1Factory> factory;
            winrt::com_ptr<ID2D1RenderTarget> target;
            explicit Canvas(std::shared_ptr<ReadingMemory> const &memory) : _pixels(ReadingAllocator<uint8_t>{memory})
            {
                winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, factory.put()));
                winrt::com_ptr<IWICImagingFactory> wic;
                winrt::check_hresult(CoCreateInstance(CLSID_WICImagingFactory2, nullptr, CLSCTX_INPROC_SERVER,
                                                      __uuidof(IWICImagingFactory), wic.put_void()));
                winrt::check_hresult(wic->CreateBitmap(Extent, Extent, GUID_WICPixelFormat32bppPBGRA,
                                                       WICBitmapCacheOnLoad, _bitmap.put()));
                auto properties = D2D1::RenderTargetProperties(
                    D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                    D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96);
                winrt::check_hresult(factory->CreateWicBitmapRenderTarget(_bitmap.get(), properties, target.put()));
                _pixels.resize(Extent * Extent * 4);
            }
            void Paint(NativeMathDrawing const &drawing, bool force = false,
                       D2D1_COLOR_F foreground = D2D1::ColorF(D2D1::ColorF::Black), MathTextAnchor anchor = {16, 96},
                       D2D1_COLOR_F background = D2D1::ColorF(D2D1::ColorF::White), double fontDips = 40)
            {
                auto placement = drawing.Place(fontDips, 144, MathPlacementMode::Inline);
                Require(placement.status == MathPlacementStatus::Ready, L"geometry fixture must fit");
                target->BeginDraw();
                target->Clear(background);
                auto status = drawing.Replay(target.get(), placement, anchor, foreground, force);
                auto endStatus = target->EndDraw();
                winrt::check_hresult(status);
                winrt::check_hresult(endStatus);
                winrt::check_hresult(
                    _bitmap->CopyPixels(nullptr, Extent * 4, static_cast<UINT>(_pixels.size()), _pixels.data()));
            }
            void Pixel(uint32_t x, uint32_t y, uint8_t r, uint8_t g, uint8_t b, wchar_t const *detail)
            {
                auto index = (y * Extent + x) * 4;
                Require(_pixels[index] == b && _pixels[index + 1] == g && _pixels[index + 2] == r &&
                            _pixels[index + 3] == 255,
                        detail);
            }
            void Contained(MathPlacement const &placement, MathTextAnchor anchor)
            {
                // A pixel intersects ink at its area, with up to one pixel of
                // antialias coverage. The independent oracle is the real WIC
                // output, not a second replay of the geometry bound algorithm.
                double left = std::floor(anchor.left) - 1;
                double top = std::floor(anchor.baseline - placement.metrics.baseline) - 1;
                double right = std::ceil(anchor.left + placement.metrics.width) + 1;
                double bottom = std::ceil(anchor.baseline - placement.metrics.baseline + placement.metrics.height) + 1;
                size_t painted{};
                for (uint32_t y = 0; y != Extent; ++y)
                {
                    for (uint32_t x = 0; x != Extent; ++x)
                    {
                        auto index = (y * Extent + x) * 4;
                        if (_pixels[index] == 255 && _pixels[index + 1] == 255 && _pixels[index + 2] == 255)
                            continue;
                        ++painted;
                        Require(x >= left && x < right && y >= top && y < bottom,
                                L"real stroked Bezier pixels must remain inside the measured box");
                    }
                }
                Require(painted > 0, L"stroke containment cannot pass with an empty image");
            }
        };

        void Pixels(std::shared_ptr<ReadingMemory> const &memory, Canvas &canvas)
        {
            auto borrowed = hole;
            auto drawing = Drawing(memory, canvas.factory.get(), borrowed);
            borrowed[1].x1 = 100;
            Require(drawing->Artifact().commands[1].x1 == 2,
                    L"native artifact must own its arrays after the borrowed batch changes");
            canvas.Paint(*drawing);
            canvas.Pixel(20, 20, 0, 0, 0, L"outer contour must fill");
            canvas.Pixel(56, 56, 255, 255, 255, L"opposite winding must leave the complete hole empty");
            canvas.Pixel(12, 56, 255, 255, 255, L"outside outline must remain empty");
            auto sameWinding = hole;
            sameWinding[6] = {V::Line, 1.5, -1.5};
            sameWinding[8] = {V::Line, 0.5, -0.5};
            canvas.Paint(*Drawing(memory, canvas.factory.get(), sameWinding));
            canvas.Pixel(56, 56, 0, 0, 0, L"same winding must fill: even-odd is not permitted");

            constexpr std::array quadratic{MathPathCommand{V::Move}, MathPathCommand{V::Quad, 1, -2, 2, 0},
                                           MathPathCommand{V::Close}};
            auto quad = Drawing(memory, canvas.factory.get(), quadratic, {}, 0);
            Require(Near(quad->OccupiedEm().top, -1), L"quadratic ink must include its true interior extremum");
            canvas.Paint(*quad);
            canvas.Pixel(56, 76, 0, 0, 0, L"quadratic interior must paint");
            canvas.Pixel(56, 44, 255, 255, 255, L"quadratic control point must not become a line vertex");
            constexpr std::array cubic{MathPathCommand{V::Move}, MathPathCommand{V::Cubic, 0, -2, 2, -2, 2, 0},
                                       MathPathCommand{V::Close}};
            auto curve = Drawing(memory, canvas.factory.get(), cubic, {}, 0);
            Require(Near(curve->OccupiedEm().top, -1.5), L"cubic ink must include its true interior extremum");
            canvas.Paint(*curve);
            canvas.Pixel(56, 44, 0, 0, 0, L"cubic interior must paint above the quadratic apex");

            constexpr std::array segment{MathPathCommand{V::Move, 0.5, -1}, MathPathCommand{V::Line, 1.5, -1}};
            MathPrimitive stroke;
            stroke.fill = false;
            stroke.strokeWidthEm = 0.5;
            canvas.Paint(*Drawing(memory, canvas.factory.get(), segment, stroke));
            canvas.Pixel(40, 56, 0, 0, 0, L"stroked open path must paint");
            canvas.Pixel(32, 56, 255, 255, 255, L"butt cap must not extend before its endpoint");
            canvas.Pixel(80, 56, 255, 255, 255, L"butt cap must not extend after its endpoint");
            constexpr std::array corner{MathPathCommand{V::Move}, MathPathCommand{V::Line, 1, 0},
                                        MathPathCommand{V::Line, 1, 1}};
            auto bevel = Drawing(memory, canvas.factory.get(), corner, stroke);
            // The conservative stroke hull adds a 10-DIP left bearing. Keep
            // the original path origin (16,36) and all original pixel oracles.
            canvas.Paint(*bevel, false, D2D1::ColorF(D2D1::ColorF::Black), {6, 36});
            canvas.Pixel(59, 33, 0, 0, 0, L"bevel join inner triangle must fill");
            canvas.Pixel(63, 29, 255, 255, 255, L"bevel join must exclude round/miter outer corner");

            MathPrimitive red;
            red.paint = MathPaintRole::Rgba;
            red.color = {1, 0, 0, 1};
            auto colored = Drawing(memory, canvas.factory.get(), hole, red);
            canvas.Paint(*colored, false, D2D1::ColorF(D2D1::ColorF::Lime));
            canvas.Pixel(20, 20, 255, 0, 0, L"explicit RGBA must survive normal theme");
            canvas.Paint(*colored, true, D2D1::ColorF(D2D1::ColorF::Lime));
            canvas.Pixel(20, 20, 0, 255, 0, L"forced foreground must override explicit color");
            canvas.Paint(*drawing, false, D2D1::ColorF(D2D1::ColorF::White), {16, 96},
                         D2D1::ColorF(D2D1::ColorF::Black));
            canvas.Pixel(20, 20, 255, 255, 255, L"semantic foreground must respond to theme");
            canvas.Pixel(56, 56, 0, 0, 0, L"hole must expose the changed background");

            D2D1_MATRIX_3X2_F saved = D2D1::Matrix3x2F::Translation(2, 3);
            canvas.target->SetTransform(saved);
            canvas.Paint(*drawing);
            D2D1_MATRIX_3X2_F restored;
            canvas.target->GetTransform(&restored);
            Require(restored._31 == 2 && restored._32 == 3, L"replay must restore the caller transform");
            canvas.target->SetTransform(D2D1::Matrix3x2F::Identity());

            Canvas foreign(memory);
            auto placement = drawing->Place(40, 144, MathPlacementMode::Inline);
            Require(drawing->Replay(foreign.target.get(), placement, {16, 96}, D2D1::ColorF(D2D1::ColorF::Black),
                                    false) == D2DERR_WRONG_FACTORY,
                    L"paths must not be replayed into a different D2D factory");
            placement.emToDips = std::numeric_limits<float>::infinity();
            Require(drawing->Replay(canvas.target.get(), placement, {16, 96}, D2D1::ColorF(D2D1::ColorF::Black),
                                    false) == E_INVALIDARG,
                    L"invalid placement must not reach the drawing target");
        }

        void StrokeCurves(std::shared_ptr<ReadingMemory> const &memory, Canvas &canvas)
        {
            constexpr std::array quadratic{MathPathCommand{V::Move}, MathPathCommand{V::Quad, 1, -2, 2, 0}};
            constexpr std::array cubic{MathPathCommand{V::Move}, MathPathCommand{V::Cubic, 0, -2, 2, -2, 2, 0}};
            constexpr std::array filled{MathPathCommand{V::Move}, MathPathCommand{V::Quad, 1, -2, 2, 0},
                                        MathPathCommand{V::Close}};
            MathPrimitive stroke;
            stroke.fill = false;
            stroke.strokeWidthEm = 0.5;
            auto quad = Drawing(memory, canvas.factory.get(), quadratic, stroke, 0);
            auto curve = Drawing(memory, canvas.factory.get(), cubic, stroke, 0);
            stroke.fill = true;
            auto both = Drawing(memory, canvas.factory.get(), filled, stroke, 0);
            for (auto const &drawing : {quad, curve, both})
            {
                auto bounds = drawing->OccupiedEm();
                Require(bounds.left <= -0.25 && bounds.top <= -2.25 && bounds.right >= 2.25 && bounds.bottom >= 0.25,
                        L"stroke metrics must contain the fixed Bezier control hull expanded by radius 0.25em");
            }
            for (uint32_t scale : {12u, 24u, 40u})
            {
                MathTextAnchor anchor{16, 112};
                auto paint = [&](NativeMathDrawing const &drawing)
                {
                    canvas.Paint(drawing, false, D2D1::ColorF(D2D1::ColorF::Black), anchor,
                                 D2D1::ColorF(D2D1::ColorF::White), scale);
                    canvas.Contained(drawing.Place(scale, 144, MathPlacementMode::Inline), anchor);
                };
                uint32_t x = 16 + scale * 5 / 4;
                paint(*quad);
                canvas.Pixel(x, 112 - scale, 0, 0, 0, L"pure quadratic stroke must cross its analytic midpoint");
                canvas.Pixel(x, 112 - scale * 3 / 2, 255, 255, 255,
                             L"quadratic stroke must not turn its control point into a polygon vertex");
                canvas.Pixel(x, 112 - scale / 2, 255, 255, 255, L"pure quadratic stroke must leave its interior empty");
                paint(*curve);
                canvas.Pixel(x, 112 - scale * 3 / 2, 0, 0, 0,
                             L"pure cubic stroke must cross its different analytic midpoint");
                paint(*both);
                canvas.Pixel(x, 112 - scale, 0, 0, 0, L"filled plus stroked quadratic must retain its stroke");
                canvas.Pixel(x, 112 - scale / 2, 0, 0, 0, L"filled plus stroked quadratic must fill its interior");
            }
        }

        template <typename Action> void Rejected(Action action, wchar_t const *detail)
        {
            bool rejected{};
            try
            {
                action();
            }
            catch (ReadingFailure const &)
            {
                rejected = true;
            }
            Require(rejected, detail);
        }

        void Invalid(std::shared_ptr<ReadingMemory> const &memory, ID2D1Factory *factory)
        {
            auto bad = hole;
            bad.back().verb = static_cast<V>(12345);
            Rejected([&] { Drawing(memory, factory, bad); }, L"unknown last command must reject the entire object");
            bad = hole;
            bad[1].x1 = std::numeric_limits<double>::infinity();
            Rejected([&] { Drawing(memory, factory, bad); }, L"infinite path coordinate must be rejected");
            bad[1].x1 = std::numeric_limits<double>::quiet_NaN();
            Rejected([&] { Drawing(memory, factory, bad); }, L"NaN path coordinate must be rejected");
            bad[1].x1 = std::numeric_limits<double>::max();
            Rejected([&] { Drawing(memory, factory, bad); }, L"f64 to f32 overflow must be rejected");
            bad[1].x1 = std::numeric_limits<double>::denorm_min();
            Rejected([&] { Drawing(memory, factory, bad); }, L"nonzero f64 must not silently become zero");
            bad = hole;
            bad[0].verb = V::Line;
            Rejected([&] { Drawing(memory, factory, bad); }, L"line before move must be rejected");
            bad = hole;
            bad[5].verb = V::Close;
            Rejected([&] { Drawing(memory, factory, bad); }, L"second close without move must be rejected");
            MathPrimitive p;
            p.start = UINT32_MAX;
            p.count = 2;
            Rejected([&] { NativeMathDrawing::Create(memory, factory, {2, 2, 0, {}, {&p, 1}, hole}); },
                     L"start/count addition wrap must be rejected");
            p.start = 1;
            p.count = static_cast<uint32_t>(hole.size());
            Rejected([&] { NativeMathDrawing::Create(memory, factory, {2, 2, 0, {}, {&p, 1}, hole}); },
                     L"range extending beyond command table must be rejected");
            p.start = 0;
            p.strokeWidthEm = -1;
            Rejected([&] { NativeMathDrawing::Create(memory, factory, {2, 2, 0, {}, {&p, 1}, hole}); },
                     L"negative stroke must be rejected");
            p.strokeWidthEm = 0;
            p.paint = static_cast<MathPaintRole>(99);
            Rejected([&] { NativeMathDrawing::Create(memory, factory, {2, 2, 0, {}, {&p, 1}, hole}); },
                     L"unknown paint role must reject the object");
            p.paint = MathPaintRole::Rgba;
            p.color.red = 2;
            Rejected([&] { NativeMathDrawing::Create(memory, factory, {2, 2, 0, {}, {&p, 1}, hole}); },
                     L"out of range color must be rejected");
            ReadingVector<MathPathCommand> commands{ReadingAllocator<MathPathCommand>{memory}};
            commands.resize(NativeMathDrawing::MaximumCommands + 1);
            Rejected([&] { NativeMathDrawing::Create(memory, factory, {2, 2, 0, {}, {}, commands}); },
                     L"command limit must apply before native creation");
            ReadingVector<MathPrimitive> primitives{ReadingAllocator<MathPrimitive>{memory}};
            primitives.resize(NativeMathDrawing::MaximumPrimitives + 1);
            Rejected([&] { NativeMathDrawing::Create(memory, factory, {2, 2, 0, {}, primitives, hole}); },
                     L"primitive limit must apply before native creation");
            p = {};
            p.count = static_cast<uint32_t>(hole.size());
            Rejected([&] { NativeMathDrawing::Create(memory, factory, {-1, 2, 0, {}, {&p, 1}, hole}); },
                     L"negative advance must be rejected");
            Rejected([&] { NativeMathDrawing::Create(memory, factory, {2, 2, 0, {1, 0, -1, 0}, {&p, 1}, hole}); },
                     L"inverted declared ink must be rejected");
            constexpr double maximum = std::numeric_limits<float>::max();
            constexpr std::array expansion{MathPathCommand{V::Move}, MathPathCommand{V::Line, maximum, 0}};
            p.fill = false;
            p.strokeWidthEm = maximum;
            Rejected([&] { Drawing(memory, factory, expansion, p, 0); },
                     L"stroke hull expansion beyond float range must fail explicitly");
        }

        void Metrics(std::shared_ptr<ReadingMemory> const &memory, ID2D1Factory *factory)
        {
            auto drawing = Drawing(memory, factory, protruding, {}, 1);
            auto occupied = drawing->OccupiedEm();
            Require(Near(occupied.left, -0.5) && Near(occupied.top, -2) && Near(occupied.right, 2.5) &&
                        Near(occupied.bottom, 1),
                    L"actual ink must enlarge all four logical sides");
            auto in = drawing->Place(20, 100, MathPlacementMode::Inline);
            Require(in.status == MathPlacementStatus::Ready && Near(in.metrics.width, 60) &&
                        Near(in.metrics.height, 60) && Near(in.metrics.baseline, 40) && Near(in.paintOriginX, 10),
                    L"inline metrics must contain negative bearings, ascent and descent");
            auto display = drawing->Place(20, 100, MathPlacementMode::Display, 4, 6);
            Require(display.status == MathPlacementStatus::Ready && Near(display.metrics.width, 100) &&
                        Near(display.metrics.height, 70) && Near(display.metrics.baseline, 44) &&
                        Near(display.paintOriginX, 30),
                    L"display must center complete ink and include theme spacing");
            auto fit = drawing->Place(20, 45, MathPlacementMode::Display);
            Require(fit.status == MathPlacementStatus::Ready && Near(fit.emToDips, 15),
                    L"exactly 75 percent display scale must remain eligible");
            Require(drawing->Place(20, 44.9, MathPlacementMode::Display).status == MathPlacementStatus::Limited &&
                        drawing->Place(20, 59, MathPlacementMode::Inline).status == MathPlacementStatus::Limited,
                    L"oversize object must return typed Limited rather than clipped Ready");
            Require(drawing->Place(std::numeric_limits<double>::max(), 100, MathPlacementMode::Inline).status ==
                            MathPlacementStatus::Invalid &&
                        drawing->Place(20, 0, MathPlacementMode::Display).status == MathPlacementStatus::Invalid,
                    L"overflow or invalid available width must not become Ready");
            auto object = NativeMathDrawing::CreateInlineObject(memory, display, L"\ufffcs", 0);
            DWRITE_INLINE_OBJECT_METRICS metrics{};
            DWRITE_OVERHANG_METRICS overhang{};
            DWRITE_BREAK_CONDITION before{}, after{};
            winrt::check_hresult(object->GetMetrics(&metrics));
            winrt::check_hresult(object->GetOverhangMetrics(&overhang));
            winrt::check_hresult(object->GetBreakConditions(&before, &after));
            Require(Near(metrics.baseline, 44) && Near(metrics.height, 70) && overhang.left == 0 && overhang.top == 0 &&
                        overhang.right == 0 && overhang.bottom == 0 && before == DWRITE_BREAK_CONDITION_MUST_BREAK &&
                        after == DWRITE_BREAK_CONDITION_MUST_BREAK,
                    L"COM inline metrics and display break contract must match the complete box");
            Require(object->GetMetrics(nullptr) == E_POINTER && object->GetOverhangMetrics(nullptr) == E_POINTER &&
                        object->GetBreakConditions(nullptr, &after) == E_POINTER,
                    L"COM null outputs must be rejected");
            auto terminal = NativeMathDrawing::CreateInlineObject(memory, display, L"p\ufffc", 1);
            winrt::check_hresult(terminal->GetBreakConditions(&before, &after));
            Require(before == DWRITE_BREAK_CONDITION_MUST_BREAK && after == DWRITE_BREAK_CONDITION_NEUTRAL,
                    L"only the actual terminal display must suppress the extra DWrite empty line");
            Rejected([&] { NativeMathDrawing::CreateInlineObject(memory, display, L"p", 0); },
                     L"inline context must point to an actual U+FFFC");
            Rejected([&] { NativeMathDrawing::CreateInlineObject(memory, display, L"\ufffc", 1); },
                     L"inline context must reject an offset at text end");
            Rejected([&] { NativeMathDrawing::CreateInlineObject(memory, display, L"\ufffc", UINT32_MAX); },
                     L"inline context must reject an overflowing offset");
            Rejected([&] { NativeMathDrawing::CreateInlineObject(memory, display, {}, 0); },
                     L"inline context must reject empty text");
            auto inlineObject = NativeMathDrawing::CreateInlineObject(memory, in, L"\ufffc", 0);
            winrt::check_hresult(inlineObject->GetBreakConditions(&before, &after));
            Require(before == DWRITE_BREAK_CONDITION_NEUTRAL && after == DWRITE_BREAK_CONDITION_NEUTRAL,
                    L"inline formula must retain normal wrapping conditions");
        }

        void Layouts(std::shared_ptr<ReadingMemory> const &memory, ID2D1Factory *factory)
        {
            winrt::com_ptr<IDWriteFactory> dwrite;
            winrt::check_hresult(DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED, __uuidof(IDWriteFactory),
                                                     reinterpret_cast<IUnknown **>(dwrite.put())));
            winrt::com_ptr<IDWriteTextFormat> format;
            winrt::check_hresult(dwrite->CreateTextFormat(L"Segoe UI", nullptr, DWRITE_FONT_WEIGHT_NORMAL,
                                                          DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 12,
                                                          L"en-US", format.put()));
            auto drawing = Drawing(memory, factory, protruding, {}, 1);
            struct Fixture
            {
                std::wstring_view text;
                MathPlacementMode mode;
                uint32_t lines;
                float width{240};
            };
            constexpr auto in = MathPlacementMode::Inline;
            constexpr auto display = MathPlacementMode::Display;
            constexpr std::array fixtures{Fixture{L"\ufffc", in, 1},
                                          Fixture{L"p\ufffc", in, 1},
                                          Fixture{L"\ufffcs", in, 1},
                                          Fixture{L"p\ufffcs", in, 1},
                                          Fixture{L"\ufffc\ufffc", in, 1},
                                          Fixture{L"p\ufffc\ufffcs", in, 1},
                                          Fixture{L"prefix \ufffc suffix", in, 3, 65},
                                          Fixture{L"\ufffc", display, 1},
                                          Fixture{L"p\ufffc", display, 2},
                                          Fixture{L"\ufffcs", display, 2},
                                          Fixture{L"p\ufffcs", display, 3},
                                          Fixture{L"\ufffc\ufffc", display, 2},
                                          Fixture{L"p\ufffc\ufffcs", display, 4}};
            for (size_t fixtureIndex = 0; fixtureIndex != fixtures.size(); ++fixtureIndex)
            {
                auto const &fixture = fixtures[fixtureIndex];
                winrt::com_ptr<IDWriteTextLayout> layout;
                winrt::check_hresult(dwrite->CreateTextLayout(fixture.text.data(),
                                                              static_cast<UINT32>(fixture.text.size()), format.get(),
                                                              fixture.width, 1000, layout.put()));
                winrt::check_hresult(layout->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_DEFAULT, 0, 0));
                auto placement = drawing->Place(20, fixture.width, fixture.mode);
                Require(placement.status == MathPlacementStatus::Ready, L"layout fixture must fit");
                for (uint32_t position = 0; position != fixture.text.size(); ++position)
                {
                    if (fixture.text[position] == L'\ufffc')
                    {
                        auto object = NativeMathDrawing::CreateInlineObject(memory, placement, fixture.text, position);
                        winrt::check_hresult(layout->SetInlineObject(object.get(), {position, 1}));
                    }
                }
                UINT32 count{};
                auto status = layout->GetLineMetrics(nullptr, 0, &count);
                Require(status == HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER),
                        L"DWrite must report line buffer size");
                wchar_t lineDetail[160]{};
                swprintf_s(lineDetail, L"fixed display visual line count differs: fixture=%zu expected=%u actual=%u",
                           fixtureIndex, fixture.lines, count);
                Require(count == fixture.lines, lineDetail);
                ReadingVector<DWRITE_LINE_METRICS> lines{ReadingAllocator<DWRITE_LINE_METRICS>{memory}};
                lines.resize(count);
                winrt::check_hresult(layout->GetLineMetrics(lines.data(), count, &count));
                uint32_t start{};
                float top{};
                for (auto const &line : lines)
                {
                    Require(line.length > 0 && line.newlineLength == 0 && line.length <= fixture.text.size() - start,
                            L"visual wrapping must not insert text or create empty display lines");
                    uint32_t objectCount{};
                    for (uint32_t position = start; position != start + line.length; ++position)
                    {
                        if (fixture.text[position] != L'\ufffc')
                            continue;
                        ++objectCount;
                        auto anchor = drawing->TextAnchor(layout.get(), position);
                        Require(Near(anchor.baseline, top + line.baseline) &&
                                    line.baseline + 0.002f >= placement.metrics.baseline &&
                                    line.height - line.baseline + 0.002f >=
                                        placement.metrics.height - placement.metrics.baseline,
                                L"actual DWrite baseline/line height must contain the complete formula ink");
                        if (fixture.mode == display)
                            Require(line.length == 1 && Near(anchor.left, 0),
                                    L"display must occupy its own visual line");
                    }
                    if (fixture.mode == display)
                        Require(objectCount <= 1, L"adjacent displays must break once between the two atoms");
                    start += line.length;
                    top += line.height;
                }
                Require(start == fixture.text.size(), L"Main UTF16 length must remain unchanged by visual line breaks");
                UINT32 clusters{};
                layout->GetClusterMetrics(nullptr, 0, &clusters);
                ReadingVector<DWRITE_CLUSTER_METRICS> clusterMetrics{ReadingAllocator<DWRITE_CLUSTER_METRICS>{memory}};
                clusterMetrics.resize(clusters);
                winrt::check_hresult(layout->GetClusterMetrics(clusterMetrics.data(), clusters, &clusters));
                uint32_t length{};
                for (auto const &cluster : clusterMetrics)
                    length += cluster.length;
                Require(length == fixture.text.size(), L"DWrite clusters must retain exactly the original Main length");
            }
        }

        class BaselineObserver final : public IDWriteTextRenderer
        {
            std::atomic<ULONG> _references{1};

          public:
            struct Event
            {
                bool object{};
                uint32_t position{};
                float left{}, baseline{}, top{};
            };
            std::array<Event, 16> events{};
            size_t count{};

            static void *operator new(size_t bytes, ReadingMemory const &memory)
            {
                return memory.Allocate(bytes);
            }
            static void operator delete(void *memory) noexcept
            {
                ReadingMemory::Free(memory);
            }
            static void operator delete(void *memory, ReadingMemory const &) noexcept
            {
                ReadingMemory::Free(memory);
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
                auto remaining = --_references;
                if (!remaining)
                    delete this;
                return remaining;
            }
            HRESULT STDMETHODCALLTYPE IsPixelSnappingDisabled(void *, BOOL *output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = TRUE;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetCurrentTransform(void *, DWRITE_MATRIX *output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = {1, 0, 0, 1, 0, 0};
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetPixelsPerDip(void *, FLOAT *output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = 1;
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE DrawGlyphRun(void *, FLOAT x, FLOAT y, DWRITE_MEASURING_MODE,
                                                   DWRITE_GLYPH_RUN const *,
                                                   DWRITE_GLYPH_RUN_DESCRIPTION const *description,
                                                   IUnknown *) noexcept override
            {
                if (!description || count == events.size())
                    return E_FAIL;
                events[count++] = {false, description->textPosition, x, y, 0};
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
            HRESULT STDMETHODCALLTYPE DrawInlineObject(void *, FLOAT x, FLOAT y, IDWriteInlineObject *object, BOOL,
                                                       BOOL, IUnknown *) noexcept override
            {
                if (!object || count == events.size())
                    return E_FAIL;
                DWRITE_INLINE_OBJECT_METRICS metrics{};
                auto status = object->GetMetrics(&metrics);
                if (FAILED(status))
                    return status;
                // DWrite supplies this callback's actual object top-left.
                // Add the object's own baseline, independently of line metrics
                // or GetMetrics().top used by the production helper.
                events[count++] = {true, 1, x, y + metrics.baseline, y};
                return S_OK;
            }
        };

        void AlignedBaselines(std::shared_ptr<ReadingMemory> const &memory, ID2D1Factory *factory)
        {
            winrt::com_ptr<IDWriteFactory> dwrite;
            winrt::check_hresult(DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED, __uuidof(IDWriteFactory),
                                                     reinterpret_cast<IUnknown **>(dwrite.put())));
            winrt::com_ptr<IDWriteTextFormat> format;
            winrt::check_hresult(dwrite->CreateTextFormat(L"Segoe UI", nullptr, DWRITE_FONT_WEIGHT_NORMAL,
                                                          DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL, 12,
                                                          L"en-US", format.put()));
            constexpr std::array low{MathPathCommand{V::Move, 0, -0.25}, MathPathCommand{V::Line, 1, -0.25},
                                     MathPathCommand{V::Line, 1, 2}, MathPathCommand{V::Line, 0, 2},
                                     MathPathCommand{V::Close}};
            auto tall = Drawing(memory, factory, protruding, {}, 1);
            auto lowBaseline = Drawing(memory, factory, low, {}, 0.25);
            constexpr std::wstring_view text = L"p\ufffcs";
            for (float font : {12.0f, 48.0f})
            {
                for (auto const &drawing : {tall, lowBaseline})
                {
                    for (auto mode : {MathPlacementMode::Inline, MathPlacementMode::Display})
                    {
                        for (auto alignment : {DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
                                               DWRITE_PARAGRAPH_ALIGNMENT_FAR})
                        {
                            winrt::com_ptr<IDWriteTextLayout> layout;
                            winrt::check_hresult(dwrite->CreateTextLayout(text.data(), static_cast<UINT32>(text.size()),
                                                                          format.get(), 240, 1000, layout.put()));
                            winrt::check_hresult(layout->SetFontSize(font, {0, static_cast<uint32_t>(text.size())}));
                            winrt::check_hresult(layout->SetParagraphAlignment(alignment));
                            winrt::check_hresult(layout->SetLineSpacing(DWRITE_LINE_SPACING_METHOD_DEFAULT, 0, 0));
                            auto placement = drawing->Place(20, 240, mode);
                            auto object = NativeMathDrawing::CreateInlineObject(memory, placement, text, 1);
                            winrt::check_hresult(layout->SetInlineObject(object.get(), {1, 1}));
                            winrt::com_ptr<BaselineObserver> observer;
                            observer.attach(new (*memory) BaselineObserver());
                            winrt::check_hresult(layout->Draw(nullptr, observer.get(), 0, 0));
                            BaselineObserver::Event const *objectEvent{}, *prefix{}, *suffix{};
                            for (auto const &event : std::span{observer->events}.first(observer->count))
                            {
                                if (event.object)
                                {
                                    Require(!objectEvent, L"baseline fixture must draw one object");
                                    objectEvent = &event;
                                }
                                else if (event.position == 0)
                                    prefix = &event;
                                else if (event.position == 2)
                                    suffix = &event;
                            }
                            Require(objectEvent && prefix && suffix,
                                    L"independent DWrite renderer must observe the object and both text runs");
                            auto anchor = drawing->TextAnchor(layout.get(), 1);
                            Require(Near(anchor.left, objectEvent->left) &&
                                        Near(anchor.baseline, objectEvent->baseline),
                                    L"TextAnchor must match the actual DWrite renderer in NEAR/CENTER/FAR alignment");
                            if (mode == MathPlacementMode::Inline)
                            {
                                Require(
                                    Near(prefix->baseline, anchor.baseline) && Near(suffix->baseline, anchor.baseline),
                                    L"different body sizes and formula baselines must share the actual text baseline");
                                if (font == 48)
                                {
                                    DWRITE_TEXT_METRICS metrics{};
                                    winrt::check_hresult(layout->GetMetrics(&metrics));
                                    Require(objectEvent->top > metrics.top + 1,
                                            L"large body text must move the object down within its natural line");
                                }
                            }
                            else
                                Require(
                                    prefix->baseline < anchor.baseline && suffix->baseline > anchor.baseline,
                                    L"mixed display callbacks must place prefix/object/suffix on their visual lines");
                        }
                    }
                }
            }
        }
    }

    void Run(std::shared_ptr<ReadingMemory> const &memory)
    {
        Require(static_cast<bool>(memory), L"native math geometry tests require real model-root funding");
        Canvas canvas(memory);
        Invalid(memory, canvas.factory.get());
        Metrics(memory, canvas.factory.get());
        Pixels(memory, canvas);
        StrokeCurves(memory, canvas);
        Layouts(memory, canvas.factory.get());
        AlignedBaselines(memory, canvas.factory.get());
    }
}
