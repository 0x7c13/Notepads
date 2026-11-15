// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeInlineCodeTests.h"
#include "Rendering/NativeDocumentRenderer.h"
#include "Rendering/NativeMeasurementCache.h"
#include <array>
#include <cstring>
#include <dwrite_1.h>

namespace Notepads::Markdown::Native::InlineCodeTests
{
    namespace
    {
        using Color = winrt::Windows::UI::Color;

        void Require(bool value, wchar_t const *message)
        {
            if (!value)
                throw winrt::hresult_error(E_FAIL, message);
        }

        struct Range
        {
            uint32_t start{}, end{};
        };

        Range Find(std::wstring_view value)
        {
            auto position = FixtureMain.find(value);
            Require(position != std::wstring_view::npos && FixtureMain.find(value, position + 1) == std::wstring_view::npos,
                    L"inline code authored oracle token is unique in fixed Main");
            return {static_cast<uint32_t>(position), static_cast<uint32_t>(position + value.size())};
        }

        auto Ranges()
        {
            auto space = Find(L"space   and");
            auto edge = Find(L"edge é and é.");
            return std::array{Find(L"chip"), Find(L"bold"), Find(L"link"), Find(L"😀 é abc שלום xyz"),
                              Find(L"left"), Find(L"right"), Range{space.start + 6, space.start + 7}, Find(L"x."),
                              Find(L"quote tail"), Find(L"table"),
                              Find(L"wrap one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen"),
                              Find(L"image"), Range{edge.start + 5, edge.start + 6}, Range{edge.start + 13, edge.start + 14}};
        }

        // These ranges are authored from Markdown semantics, never from the new
        // background metadata or renderer run flags.
        std::array<Range, 14> ExpectedRanges()
        {
            auto ranges = Ranges();
            --ranges[7].end; // The period following the one-character code is ordinary.
            ranges[8].end -= 5; // Only `quote`, not its following ordinary text.
            return ranges;
        }

        std::vector<DWRITE_HIT_TEST_METRICS> Hits(IDWriteTextLayout *layout, uint32_t start, uint32_t length)
        {
            Require(length > 0, L"independent DWrite hit oracle receives a nonempty semantic range");
            FLOAT x{}, y{};
            DWRITE_HIT_TEST_METRICS first{}, last{};
            winrt::check_hresult(layout->HitTestTextPosition(start, FALSE, &x, &y, &first));
            winrt::check_hresult(layout->HitTestTextPosition(start + length - 1, FALSE, &x, &y, &last));
            Require(first.isText && last.isText && first.length && last.length &&
                        first.textPosition <= start && first.textPosition + first.length > start &&
                        last.textPosition <= start + length - 1 && last.textPosition + last.length >= start + length,
                    L"semantic hit endpoints belong to real DWrite text clusters");
            auto geometryStart = std::min(start, first.textPosition);
            auto geometryEnd = std::max(start + length, last.textPosition + last.length);
            UINT32 count{};
            auto status = layout->HitTestTextRange(geometryStart, geometryEnd - geometryStart, 0, 0, nullptr, 0, &count);
            if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                winrt::check_hresult(status);
            std::vector<DWRITE_HIT_TEST_METRICS> result(count);
            if (count)
                winrt::check_hresult(layout->HitTestTextRange(geometryStart, geometryEnd - geometryStart, 0, 0,
                                                             result.data(), count, &count));
            result.resize(count);
            for (auto &hit : result)
            {
                Require(std::isfinite(hit.width) && hit.width >= 0,
                        L"independent DWrite hit oracle rejects negative or nonfinite advances");
                if (hit.isText && hit.length && hit.width == 0)
                {
                    DWRITE_HIT_TEST_METRICS cluster{};
                    winrt::check_hresult(layout->HitTestTextPosition(hit.textPosition, FALSE, &x, &y, &cluster));
                    Require(cluster.isText && std::isfinite(cluster.width) && cluster.width >= 0 && cluster.textPosition <= hit.textPosition &&
                                cluster.textPosition + cluster.length >= hit.textPosition + hit.length,
                            L"zero-advance styled subrange preserves its real containing DWrite cluster geometry");
                    hit = cluster;
                }
            }
            return result;
        }

        float Baseline(IDWriteTextLayout *layout, uint32_t position)
        {
            UINT32 count{};
            auto status = layout->GetLineMetrics(nullptr, 0, &count);
            if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                winrt::check_hresult(status);
            std::vector<DWRITE_LINE_METRICS> lines(count);
            if (count)
                winrt::check_hresult(layout->GetLineMetrics(lines.data(), count, &count));
            uint32_t start{};
            float y{};
            for (auto const &line : lines)
            {
                if (position >= start && position < start + line.length)
                    return y + line.baseline;
                start += line.length;
                y += line.height;
            }
            throw winrt::hresult_error(E_FAIL, L"inline code position belongs to an independently queried DWrite line");
        }

        void CheckRanges(PreviewFrame const &frame, Results &results)
        {
            auto expected = ExpectedRanges();
            std::vector<uint32_t> coverage(FixtureMain.size());
            std::array<uint32_t, 14> fragments{};
            for (auto const &block : frame.Blocks())
            {
                for (auto const &text : block.texts)
                {
                    for (auto const &code : text.inlineCode)
                    {
                        Require(text.inlineCode.get_allocator().layout != nullptr && code.length > 0 &&
                                    code.start <= text.utf16Length && code.length <= text.utf16Length - code.start,
                                L"inline code retained bounds use the real preview allocator and a valid local UTF16 range");
                        auto start = static_cast<uint32_t>(text.logicalStart + code.start);
                        auto end = start + code.length;
                        auto owner = std::find_if(expected.begin(), expected.end(),
                                                  [&](Range value) { return start >= value.start && end <= value.end; });
                        Require(owner != expected.end(),
                                L"inline code fragment never includes ordinary punctuation, gaps, objects, fenced code or raw HTML");
                        ++fragments[static_cast<size_t>(owner - expected.begin())];
                        for (auto position = start; position < end; ++position)
                            ++coverage[position];
                        auto const &bounds = code.bounds;
                        Require(std::isfinite(bounds.left) && std::isfinite(bounds.top) && std::isfinite(bounds.right) &&
                                    std::isfinite(bounds.bottom) && bounds.left >= 0 && bounds.top >= 0 &&
                                    bounds.right >= bounds.left && bounds.bottom > bounds.top &&
                                    bounds.right <= text.layout->GetMaxWidth() && bounds.bottom <= text.height,
                                L"inline code final local rectangle is finite, nonnegative and clipped to its real text layout");
                        if (bounds.right == bounds.left)
                        {
                            Require(owner == expected.begin() + 13 && code.length == 1 && code.start == 13 &&
                                        start == expected[13].start && end == expected[13].end,
                                    L"only the authored isolated code acute may retain a zero-advance semantic marker");
                            FLOAT x{}, y{};
                            DWRITE_HIT_TEST_METRICS hit{};
                            winrt::check_hresult(text.layout->HitTestTextPosition(code.start, FALSE, &x, &y, &hit));
                            Require(hit.isText && hit.textPosition == code.start && hit.length == 1 && hit.width == 0 &&
                                        bounds.left == hit.left && bounds.right == hit.left,
                                    L"isolated acute marker has exactly the real one-unit zero-width DWrite position hit");
                            UINT32 count{};
                            auto status = text.layout->GetClusterMetrics(nullptr, 0, &count);
                            if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                                winrt::check_hresult(status);
                            std::vector<DWRITE_CLUSTER_METRICS> clusters(count);
                            if (count)
                                winrt::check_hresult(text.layout->GetClusterMetrics(clusters.data(), count, &count));
                            uint32_t clusterStart{};
                            bool orphanCluster{};
                            for (auto const &cluster : clusters)
                            {
                                if (code.start >= clusterStart && code.start < clusterStart + cluster.length)
                                    orphanCluster = clusterStart == code.start && cluster.length == 1 && cluster.width == 0;
                                clusterStart += cluster.length;
                            }
                            Require(orphanCluster,
                                    L"isolated acute is independently confirmed as exactly one zero-width DWrite cluster");
                            UINT32 selectionCount{};
                            status = text.layout->HitTestTextRange(code.start, 1, 0, 0, nullptr, 0, &selectionCount);
                            if (FAILED(status) && status != HRESULT_FROM_WIN32(ERROR_INSUFFICIENT_BUFFER))
                                winrt::check_hresult(status);
                            auto rectangles = NativeDocumentRenderer::Rectangles(frame, start, end);
                            Require(rectangles.size() == selectionCount &&
                                        std::all_of(rectangles.begin(), rectangles.end(), [&](ReadingRect const &rect)
                                            { return rect.width == 0 && rect.start == start && rect.end == end &&
                                                     std::isfinite(rect.x) && std::isfinite(rect.y) && rect.height > 0; }),
                                    L"orphan source selection retains DWrite zero advance without a fabricated rectangle");
                            auto copied = frame.main->CopyText(start, 1);
                            Require(copied.size() == 1 && copied[0] == L'\x0301',
                                    L"orphan semantic marker preserves the original acute in Main copy text");
                            ++results.zeroAdvanceFragments;
                        }
                        else
                        {
                            auto hits = Hits(text.layout.get(), code.start, code.length);
                            Require(hits.size() == 1 && hits[0].isText && hits[0].width > 0 && hits[0].textPosition <= code.start &&
                                        hits[0].textPosition + hits[0].length >= code.start + code.length && bounds.left >= hits[0].left - 0.01f &&
                                        bounds.right <= hits[0].left + hits[0].width + 0.01f &&
                                        bounds.top >= hits[0].top - 0.01f &&
                                        bounds.bottom <= hits[0].top + hits[0].height + 0.01f,
                                    L"each painted badge stays within one independently queried positive-width DWrite fragment");
                            Require(std::abs(bounds.left - std::max(0.0f, hits[0].left)) < 0.01f &&
                                        std::abs(bounds.right - std::min(text.layout->GetMaxWidth(), hits[0].left + hits[0].width)) < 0.01f,
                                    L"badge covers the code advance rather than an entire line or merged surrounding text");
                            ++results.paintedFragments;
                        }
                        Require(bounds.bottom - bounds.top <= frame.theme.fontSize * 1.3f,
                                L"inline badge remains text-sized beside the actual image placeholder");
                        auto baseline = Baseline(text.layout.get(), code.start);
                        Require(bounds.top < baseline && bounds.bottom > baseline &&
                                    bounds.bottom - bounds.top >= frame.theme.fontSize * 0.7f,
                                L"inline badge straddles its real final text baseline with room for visible text");
                        ++results.fragments;
                    }
                }
            }
            for (size_t position = 0; position < coverage.size(); ++position)
            {
                auto isCode = std::any_of(expected.begin(), expected.end(),
                                          [&](Range value) { return position >= value.start && position < value.end; });
                if (coverage[position] != (isCode ? 1u : 0u))
                {
                    auto contextStart = position > 8 ? position - 8 : 0;
                    auto contextEnd = std::min(FixtureMain.size(), position + 9);
                    if (contextStart && FixtureMain[contextStart] >= 0xdc00 && FixtureMain[contextStart] <= 0xdfff)
                        --contextStart;
                    if (contextEnd < FixtureMain.size() && FixtureMain[contextEnd] >= 0xdc00 && FixtureMain[contextEnd] <= 0xdfff)
                        ++contextEnd;
                    std::wstring context;
                    for (auto unit : FixtureMain.substr(contextStart, contextEnd - contextStart))
                    {
                        if (unit == L'\n')
                            context += L"\\n";
                        else if (unit == L'\r')
                            context += L"\\r";
                        else if (unit == L'\t')
                            context += L"\\t";
                        else
                            context += unit;
                    }
                    auto detail = L"coverage position=" +
                                  std::to_wstring(position) + L"; want=" + (isCode ? L"true" : L"false") +
                                  L"; got=" + std::to_wstring(coverage[position]) + L"; u=" +
                                  std::to_wstring(static_cast<uint32_t>(FixtureMain[position])) + L"; Main[" +
                                  std::to_wstring(contextStart) + L"," + std::to_wstring(contextEnd) + L")='" + context + L"'";
                    bool foundText{};
                    for (auto const &block : frame.Blocks())
                        for (auto const &text : block.texts)
                            if (position >= text.logicalStart && position < text.logicalEnd)
                            {
                                foundText = true;
                                auto local = static_cast<uint32_t>(position - text.logicalStart);
                                detail += L"; T=[" + std::to_wstring(text.logicalStart) + L"," +
                                          std::to_wstring(text.logicalEnd) + L"); at=" + std::to_wstring(local);
                                auto spacing = text.layout.try_as<IDWriteTextLayout1>();
                                if (spacing)
                                {
                                    FLOAT leading{}, trailing{}, minimum{};
                                    auto status = spacing->GetCharacterSpacing(local, &leading, &trailing, &minimum, nullptr);
                                    detail += L"; S{h=" + std::to_wstring(static_cast<int32_t>(status)) +
                                              L",L=" + std::to_wstring(leading) + L",R=" +
                                              std::to_wstring(trailing) + L",m=" + std::to_wstring(minimum) + L"}";
                                }
                                else
                                    detail += L"; spacing{IDWriteTextLayout1 unavailable}";
                                UINT32 nameLength{};
                                auto fontStatus = text.layout->GetFontFamilyNameLength(local, &nameLength, nullptr);
                                detail += L"; F0=" + std::to_wstring(static_cast<int32_t>(fontStatus));
                                if (SUCCEEDED(fontStatus))
                                {
                                    std::vector<wchar_t> name(static_cast<size_t>(nameLength) + 1);
                                    fontStatus = text.layout->GetFontFamilyName(local, name.data(), nameLength + 1, nullptr);
                                    detail += L"; F1=" + std::to_wstring(static_cast<int32_t>(fontStatus));
                                    if (SUCCEEDED(fontStatus))
                                        detail += L"; F='" + std::wstring{name.data(), nameLength} + L"'";
                                }
                                UINT32 hitCount{};
                                auto hitStatus = text.layout->HitTestTextRange(local, 1, 0, 0, nullptr, 0, &hitCount);
                                detail += L"; R0{h=" + std::to_wstring(static_cast<int32_t>(hitStatus)) +
                                          L",n=" + std::to_wstring(hitCount) + L"}";
                                std::vector<DWRITE_HIT_TEST_METRICS> rawHits(hitCount);
                                if (hitCount)
                                {
                                    hitStatus = text.layout->HitTestTextRange(local, 1, 0, 0, rawHits.data(), hitCount, &hitCount);
                                    detail += L"; R1=" + std::to_wstring(static_cast<int32_t>(hitStatus));
                                    if (SUCCEEDED(hitStatus))
                                        for (auto const &hit : rawHits)
                                            detail += L"; R{p=" + std::to_wstring(hit.textPosition) +
                                                      L",l=" + std::to_wstring(hit.length) + L",w=" +
                                                      std::to_wstring(hit.width) + L",t=" + std::to_wstring(hit.isText) + L"}";
                                }
                                FLOAT hitX{}, hitY{};
                                DWRITE_HIT_TEST_METRICS positionHit{};
                                hitStatus = text.layout->HitTestTextPosition(local, FALSE, &hitX, &hitY, &positionHit);
                                detail += L"; P{h=" + std::to_wstring(static_cast<int32_t>(hitStatus)) +
                                          L",p=" + std::to_wstring(positionHit.textPosition) + L",l=" +
                                          std::to_wstring(positionHit.length) + L",w=" + std::to_wstring(positionHit.width) +
                                          L",t=" + std::to_wstring(positionHit.isText) + L"}";
                                UINT32 clusterCount{};
                                auto clusterStatus = text.layout->GetClusterMetrics(nullptr, 0, &clusterCount);
                                detail += L"; C0{h=" + std::to_wstring(static_cast<int32_t>(clusterStatus)) +
                                          L",n=" + std::to_wstring(clusterCount) + L"}";
                                std::vector<DWRITE_CLUSTER_METRICS> clusters(clusterCount);
                                if (clusterCount)
                                {
                                    clusterStatus = text.layout->GetClusterMetrics(clusters.data(), clusterCount, &clusterCount);
                                    detail += L"; C1=" + std::to_wstring(static_cast<int32_t>(clusterStatus));
                                    if (SUCCEEDED(clusterStatus))
                                    {
                                        uint32_t clusterStart{};
                                        for (auto const &cluster : clusters)
                                        {
                                            if (local >= clusterStart && local < clusterStart + cluster.length)
                                                detail += L"; C{p=" + std::to_wstring(clusterStart) +
                                                          L",l=" + std::to_wstring(cluster.length) + L",w=" +
                                                          std::to_wstring(cluster.width) + L"}";
                                            clusterStart += cluster.length;
                                        }
                                    }
                                }
                            }
                    if (!foundText)
                        detail += L"; containingText=none";
                    throw winrt::hresult_error(E_FAIL, detail.c_str());
                }
            }
            Require(std::all_of(fragments.begin(), fragments.end(), [](auto count) { return count > 0; }),
                    L"normal, bold, link, unicode, quote, table, whitespace and one-character code retain semantic fragments");
            Require(results.zeroAdvanceFragments == 1 && results.paintedFragments + results.zeroAdvanceFragments == results.fragments,
                    L"semantic coverage includes exactly one proven orphan marker and all other fragments have painted area");
            Require(fragments[10] > 1 && fragments[3] > 1,
                    L"long code wraps and mixed RTL code splits into actual visual fragments");
            results.ranges = expected.size();
            results.wrappedFragments = fragments[10];
            results.bidiFragments = fragments[3];
            results.authoredRanges = results.visualFragments = results.fundedBounds = true;
        }

        void SameGeometry(PreviewFrame const &a, PreviewFrame const &b)
        {
            Require(a.main->Text() == FixtureMain && b.main->Text() == FixtureMain &&
                        a.totalHeight == b.totalHeight && a.Blocks().size() == b.Blocks().size() &&
                        a.navigation->LineCount() == b.navigation->LineCount(),
                    L"inline background preserves exact source Main, total height and fresh/warm line count");
            bool sawTableBacktrack{}, sawTablePlateau{};
            for (size_t i = 0; i < a.navigation->LineCount(); ++i)
            {
                auto const &x = a.navigation->Line(i);
                auto const &y = b.navigation->Line(i);
                Require(x.text.start == y.text.start && x.text.end == y.text.end && x.block == y.block &&
                            x.x == y.x && x.localY == y.localY && x.y == y.y && x.width == y.width &&
                            x.height == y.height && x.tableCell == y.tableCell && x.followY == y.followY,
                        L"inline code fresh/warm exact navigation line geometry and source offsets");
                if (i && x.tableCell && a.navigation->Line(i - 1).tableCell)
                {
                    auto const &previous = a.navigation->Line(i - 1);
                    sawTableBacktrack |= x.y < previous.y;
                    sawTablePlateau |= x.y < x.followY && x.followY == previous.followY;
                }
            }
            Require(sawTableBacktrack && sawTablePlateau,
                    L"wrapped table cell keeps its real y backtrack and monotone navigation follow plateau");
            for (size_t i = 0; i < a.Blocks().size(); ++i)
            {
                auto const &x = a.Blocks()[i];
                auto const &y = b.Blocks()[i];
                Require(x.block.y == y.block.y && x.measuredHeight == y.measuredHeight && x.texts.size() == y.texts.size(),
                        L"inline code fresh/warm exact block geometry");
                for (size_t j = 0; j < x.texts.size(); ++j)
                {
                    auto const &tx = x.texts[j];
                    auto const &ty = y.texts[j];
                    Require(tx.layout.get() != ty.layout.get() && tx.logicalStart == ty.logicalStart &&
                                tx.logicalEnd == ty.logicalEnd && tx.x == ty.x && tx.y == ty.y &&
                                tx.width == ty.width && tx.height == ty.height && tx.inlineCode.size() == ty.inlineCode.size(),
                            L"warm code frame owns a fresh DWrite layout with identical text geometry and badges");
                    for (size_t k = 0; k < tx.inlineCode.size(); ++k)
                    {
                        auto const &p = tx.inlineCode[k];
                        auto const &q = ty.inlineCode[k];
                        Require(p.start == q.start && p.length == q.length && p.bounds.left == q.bounds.left &&
                                    p.bounds.top == q.bounds.top && p.bounds.right == q.bounds.right && p.bounds.bottom == q.bounds.bottom,
                                L"inline code final frame metadata rebinds identically after a measurement cache hit");
                    }
                }
            }
            for (auto range : ExpectedRanges())
            {
                auto x = NativeDocumentRenderer::Rectangles(a, range.start, range.end);
                auto y = NativeDocumentRenderer::Rectangles(b, range.start, range.end);
                Require(x.size() == y.size(), L"inline code fresh/warm selection rectangle count");
                for (size_t i = 0; i < x.size(); ++i)
                    Require(x[i].x == y[i].x && x[i].y == y[i].y && x[i].width == y[i].width &&
                                x[i].height == y[i].height && x[i].start == y[i].start && x[i].end == y[i].end,
                            L"inline code fresh/warm exact selection rectangles");
            }
        }

        struct PixelImage
        {
            static constexpr uint32_t width = 512, height = 256;
            std::vector<uint32_t> values;
            uint32_t At(uint32_t x, uint32_t y) const
            {
                return values[static_cast<size_t>(y) * width + x];
            }
        };

        uint32_t Pixel(Color color)
        {
            Require(color.A == 255, L"inline code exact-color oracle uses explicit opaque colors");
            return uint32_t{color.B} | (uint32_t{color.G} << 8) | (uint32_t{color.R} << 16) | 0xff000000u;
        }

        class WarpTarget
        {
            winrt::com_ptr<ID3D11Device> _d3d;
            winrt::com_ptr<ID2D1DeviceContext> _context;
            winrt::com_ptr<ID2D1Bitmap1> _target, _readback;

          public:
            WarpTarget()
            {
                winrt::com_ptr<ID3D11DeviceContext> immediate;
                D3D_FEATURE_LEVEL level{};
                winrt::check_hresult(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr,
                                                       D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION,
                                                       _d3d.put(), &level, immediate.put()));
                winrt::com_ptr<ID2D1Factory1> factory;
                winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, factory.put()));
                winrt::com_ptr<ID2D1Device> device;
                winrt::check_hresult(factory->CreateDevice(_d3d.as<IDXGIDevice>().get(), device.put()));
                winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, _context.put()));
                _context->SetDpi(96, 96);
                _context->SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
                auto properties = D2D1::BitmapProperties1(
                    D2D1_BITMAP_OPTIONS_TARGET,
                    D2D1::PixelFormat(DXGI_FORMAT_B8G8R8A8_UNORM, D2D1_ALPHA_MODE_PREMULTIPLIED), 96, 96);
                winrt::check_hresult(_context->CreateBitmap(D2D1::SizeU(PixelImage::width, PixelImage::height),
                                                            nullptr, 0, properties, _target.put()));
                properties.bitmapOptions = D2D1_BITMAP_OPTIONS_CPU_READ | D2D1_BITMAP_OPTIONS_CANNOT_DRAW;
                winrt::check_hresult(_context->CreateBitmap(D2D1::SizeU(PixelImage::width, PixelImage::height),
                                                            nullptr, 0, properties, _readback.put()));
            }

            PixelImage Draw(PreviewFrame const &frame, uint32_t start = 0, uint32_t end = 0, bool outline = false,
                            float originY = 8)
            {
                _context->SetAntialiasMode(outline ? D2D1_ANTIALIAS_MODE_ALIASED : D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
                _context->SetTarget(_target.get());
                _context->BeginDraw();
                auto c = frame.theme.background;
                _context->Clear(outline ? D2D1::ColorF(c.R / 255.0f, c.G / 255.0f, c.B / 255.0f, 1) : D2D1::ColorF(0, 0, 0, 0));
                try
                {
                    NativeDocumentRenderer::Draw(frame, _context.get(), 8, originY, start, end);
                }
                catch (...)
                {
                    (void)_context->EndDraw();
                    _context->SetTarget(nullptr);
                    throw;
                }
                auto status = _context->EndDraw();
                _context->SetTarget(nullptr);
                winrt::check_hresult(status);
                winrt::check_hresult(_readback->CopyFromBitmap(nullptr, _target.get(), nullptr));
                PixelImage result;
                result.values.resize(static_cast<size_t>(PixelImage::width) * PixelImage::height);
                D2D1_MAPPED_RECT mapped{};
                winrt::check_hresult(_readback->Map(D2D1_MAP_OPTIONS_READ, &mapped));
                for (uint32_t y = 0; y < PixelImage::height; ++y)
                    std::memcpy(result.values.data() + static_cast<size_t>(y) * PixelImage::width,
                                mapped.bits + static_cast<size_t>(y) * mapped.pitch, PixelImage::width * sizeof(uint32_t));
                winrt::check_hresult(_readback->Unmap());
                return result;
            }
        };

        D2D1_RECT_F Region(PreviewFrame const &frame, Range range)
        {
            for (auto const &block : frame.Blocks())
                for (auto const &text : block.texts)
                    if (range.start >= text.logicalStart && range.end <= text.logicalEnd)
                    {
                        auto hits = Hits(text.layout.get(), static_cast<uint32_t>(range.start - text.logicalStart), range.end - range.start);
                        Require(hits.size() == 1, L"pixel oracle uses one real DWrite range, independently of badge metadata");
                        auto const &hit = hits[0];
                        auto x = 8 + static_cast<float>(block.block.x) + text.x;
                        auto y = 8 + static_cast<float>(block.block.y - frame.viewportY) + text.y;
                        return {x + hit.left, y + hit.top, x + hit.left + hit.width, y + hit.top + hit.height};
                    }
            throw winrt::hresult_error(E_FAIL, L"inline code pixel range belongs to an actual rendered text");
        }

        float PlainChipWidth()
        {
            winrt::com_ptr<IDWriteFactory> factory;
            winrt::check_hresult(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory),
                                                     reinterpret_cast<IUnknown **>(factory.put())));
            winrt::com_ptr<IDWriteTextFormat> format;
            winrt::check_hresult(factory->CreateTextFormat(L"Consolas", nullptr, DWRITE_FONT_WEIGHT_NORMAL,
                                                           DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_STRETCH_NORMAL,
                                                           18, L"", format.put()));
            winrt::com_ptr<IDWriteTextLayout> layout;
            winrt::check_hresult(factory->CreateTextLayout(L"chip", 4, format.get(), 2048, 128, layout.put()));
            DWRITE_TEXT_METRICS metrics{};
            winrt::check_hresult(layout->GetMetrics(&metrics));
            Require(metrics.lineCount == 1 && metrics.widthIncludingTrailingWhitespace > 0,
                    L"independent plain Consolas18 chip has one positive-width natural glyph run");
            return metrics.widthIncludingTrailingWhitespace;
        }

        template <typename Predicate> uint64_t Count(PixelImage const &pixels, D2D1_RECT_F region, Predicate predicate)
        {
            auto left = static_cast<uint32_t>(std::max(0.0f, std::ceil(region.left)));
            auto top = static_cast<uint32_t>(std::max(0.0f, std::ceil(region.top)));
            auto right = static_cast<uint32_t>(std::min(float{PixelImage::width}, std::floor(region.right)));
            auto bottom = static_cast<uint32_t>(std::min(float{PixelImage::height}, std::floor(region.bottom)));
            Require(left < right && top < bottom, L"inline code pixel oracle has a nonempty unclipped region");
            uint64_t count{};
            for (auto y = top; y < bottom; ++y)
                for (auto x = left; x < right; ++x)
                    count += predicate(pixels.At(x, y), x, y);
            return count;
        }

        void CheckTable(PreviewFrame const &frame, WarpTarget &target, Results &results)
        {
            PreviewBlockVisual const *table{};
            for (auto const &block : frame.Blocks())
                if (block.block.kind == 6)
                {
                    Require(!table, L"rounded-table oracle has exactly one real native table");
                    table = &block;
                }
            Require(table && table->block.columns == 2 && table->rowHeights.size() == 2,
                    L"rounded-table oracle uses the authored two-column table with a wrapped body cell");
            Require(table->markers.size() == table->decorations.size(),
                    L"first-body list table pairs every native decoration with its actual marker slot");
            bool listMarker{};
            for (size_t i = 0; i < table->decorations.size(); ++i)
                listMarker |= (table->decorations[i].kind == 1 || table->decorations[i].kind == 2) &&
                              static_cast<bool>(table->markers[i]);
            Require(listMarker, L"first-body list table has an actual bullet or ordinal decoration and a non-null DWrite marker");
            auto viewport = frame.Reposition(table->block.y - 4, PixelImage::height);
            Require(viewport && viewport->main == frame.main && viewport->navigation == frame.navigation &&
                        viewport->main->Text() == FixtureMain && viewport->totalHeight == frame.totalHeight,
                    L"moving the table into the small WARP viewport preserves Main, navigation and document extent");
            auto pixels = target.Draw(*viewport);
            auto selected = target.Draw(*viewport, static_cast<uint32_t>(table->block.logical_start),
                                          static_cast<uint32_t>(table->block.logical_end));
            auto x = 8 + static_cast<float>(table->block.x);
            auto y = 8 + static_cast<float>(table->block.y - viewport->viewportY);
            auto width = static_cast<float>(table->block.width);
            auto height = static_cast<float>(table->measuredHeight);
            auto gutter = D2D1::RectF(x - static_cast<float>(frame.key.indent), y, x - 2, y + frame.theme.lineHeight);
            auto foregroundInk = [&](uint32_t pixel, auto, auto)
            {
                auto alpha = (pixel >> 24) & 255;
                if (!alpha)
                    return false;
                auto nearChannel = [&](unsigned shift, uint8_t channel)
                {
                    auto actual = (pixel >> shift) & 255;
                    return std::abs(static_cast<int>(actual * 255) - static_cast<int>(channel * alpha)) <= 255;
                };
                return nearChannel(0, frame.theme.foreground.B) && nearChannel(8, frame.theme.foreground.G) &&
                       nearChannel(16, frame.theme.foreground.R);
            };
            results.tableMarkerPixels = Count(pixels, gutter, foregroundInk);
            Require(results.tableMarkerPixels > 0 && Count(selected, gutter, foregroundInk) > 0,
                    L"first-body table bullet retains foreground ink in the list gutter outside the rounded body clip, including selection");
            results.tableMarkersOutsideClip = true;
            auto left = static_cast<uint32_t>(std::ceil(x));
            auto top = static_cast<uint32_t>(std::ceil(y));
            auto right = static_cast<uint32_t>(std::floor(x + width)) - 1;
            auto bottom = static_cast<uint32_t>(std::floor(y + height)) - 1;
            Require(right > left + 20 && bottom > top + 20 && right < PixelImage::width && bottom < PixelImage::height,
                    L"all four rounded table corners fit the existing bounded WARP target");
            for (auto corner : std::array{std::pair{left, top}, std::pair{right, top},
                                          std::pair{left, bottom}, std::pair{right, bottom}})
                Require(pixels.At(corner.first, corner.second) == 0 && selected.At(corner.first, corner.second) == 0,
                        L"all four table corners remain transparent with ordinary and selected content");
            const auto color = frame.theme.panelBackground;
            const auto opacity = 0.45f * color.A / 255.0f;
            auto isStripe = [&](uint32_t pixel)
            {
                // The target is transparent, so the stripe is premultiplied.
                auto channelMatches = [&](uint32_t shift, uint8_t channel)
                {
                    return std::abs(static_cast<long>((pixel >> shift) & 255) - std::lround(channel * opacity)) <= 1;
                };
                return channelMatches(0, color.B) && channelMatches(8, color.G) &&
                       channelMatches(16, color.R) && channelMatches(24, 255);
            };
            Require(isStripe(pixels.At(left + 6, top + 3)) && isStripe(pixels.At(right - 6, top + 3)),
                    L"the subdued header fill reaches inside both clipped rounded top corners");
            auto headerBottom = y + table->rowHeights[0];
            auto internal = [&](uint32_t pixel, auto, auto) { return pixel != 0 && !isStripe(pixel); };
            auto columnGrid = Count(pixels, {x + width / 2 - 1, y + 6, x + width / 2 + 1, headerBottom - 6}, internal);
            auto rowGrid = Count(pixels, {x + 12, headerBottom - 1, x + width - 12, headerBottom + 1}, internal);
            Require(columnGrid > 8 && rowGrid > 20,
                    L"rounding the table outer boundary preserves its interior column and row grid pixels");
            auto bodyRange = Find(L"table");
            auto selectedCode = Region(*viewport, bodyRange);
            Require(Count(selected, selectedCode,
                          [&](uint32_t pixel, auto, auto) { return pixel == Pixel(frame.theme.selectionBackground); }) > 20,
                    L"rounded table selection really draws inside the clipped body instead of merely testing an empty selection");
            results.tableGridPixels = columnGrid + rowGrid;
            results.roundedTable = true;
        }

        PreviewTheme Theme(bool dark)
        {
            PreviewTheme value;
            value.fontFamily = L"Segoe UI";
            value.codeFontFamily = L"Consolas";
            value.fontSize = 18;
            value.lineHeight = 27;
            value.foreground = dark ? Color{255, 240, 241, 242} : Color{255, 21, 27, 33};
            value.background = dark ? Color{255, 17, 23, 31} : Color{255, 255, 255, 255};
            value.panelBackground = dark ? Color{255, 53, 63, 77} : Color{255, 225, 233, 241};
            value.borderColor = dark ? Color{255, 197, 207, 217} : Color{255, 71, 83, 97};
            value.selectionBackground = {255, 37, 101, 181};
            value.selectionForeground = {255, 255, 255, 255};
            return value;
        }

        void CheckHighlights(PreviewFrame const &light, PreviewFrame const &warm,
                             PreviewFrame const &dark, PreviewFrame const &highContrast, WarpTarget &target)
        {
            auto markdown = Find(L"Gold note");
            auto html = Find(L"HTML note");
            auto raised = Find(L"raised");
            auto viewport = [&](PreviewFrame const &frame)
            {
                for (auto const &block : frame.Blocks())
                    for (auto const &text : block.texts)
                        if (text.logicalStart == markdown.start)
                            return frame.Reposition(block.block.y - 4, PixelImage::height);
                return std::shared_ptr<PreviewFrame>{};
            };
            auto plain = viewport(light);
            auto cached = viewport(warm);
            auto alternate = viewport(dark);
            auto contrast = viewport(highContrast);
            Require(plain && cached && alternate && contrast, L"all four frames contain the authored highlighted lines");
            auto lightPixels = target.Draw(*plain, 0, 0, true);
            Require(lightPixels.values == target.Draw(*cached, 0, 0, true).values,
                    L"actual warm measurement retains the same author background and glyph pixels");
            auto darkPixels = target.Draw(*alternate, 0, 0, true);
            auto hcPixels = target.Draw(*contrast, 0, 0, true);
            auto selected = target.Draw(*plain, markdown.start, markdown.end, true);
            auto paleYellow = [](uint32_t pixel, auto, auto)
            {
                return ((pixel >> 16) & 255) >= 252 && ((pixel >> 8) & 255) >= 238 &&
                    ((pixel >> 8) & 255) <= 248 && (pixel & 255) >= 182 && (pixel & 255) <= 198;
            };
            for (auto range : {markdown, html})
            {
                auto region = Region(*plain, range);
                Require(Count(lightPixels, region, paleYellow) > 20 && Count(darkPixels, region, paleYellow) == 0,
                        L"both Markdown and HTML draw light yellow only in the light theme");
                auto mutedGold = [](uint32_t pixel, auto, auto)
                {
                    return ((pixel >> 16) & 255) >= 63 && ((pixel >> 16) & 255) <= 72 &&
                        ((pixel >> 8) & 255) >= 51 && ((pixel >> 8) & 255) <= 61 && (pixel & 255) <= 28;
                };
                Require(Count(darkPixels, region, mutedGold) > 20 &&
                            Count(hcPixels, region, [](uint32_t pixel, auto, auto)
                                  { return std::abs(int(pixel & 255) - 73) <= 2; }) > 20,
                        L"the same range paints a dark background and a separate high-contrast system tint");
            }
            Require(Count(lightPixels, Region(*plain, {markdown.end, markdown.end + 2}), paleYellow) == 0,
                    L"ordinary spaces next to a highlighted range are clear");
            auto selection = Pixel(light.theme.selectionBackground);
            Require(Count(selected, Region(*plain, markdown),
                          [&](uint32_t pixel, auto, auto) { return pixel == selection; }) > 20,
                    L"selection visibly covers the authored background");
            bool script{};
            for (auto const &block : plain->Blocks())
                for (auto const &text : block.texts)
                    if (text.logicalStart == raised.start && text.scripts && !text.highlights.empty())
                    {
                        auto bounds = text.highlights.front();
                        const auto x = 8 + static_cast<float>(block.block.x) + text.x;
                        const auto y = 8 + static_cast<float>(block.block.y - plain->viewportY) + text.y;
                        bounds.left += x;
                        bounds.right += x;
                        bounds.top += y;
                        bounds.bottom += y;
                        script = Count(lightPixels, bounds, paleYellow) > 20;
                    }
            Require(script, L"raised HTML highlight has real painted pixels at adjusted script geometry");
        }
    }

    Results Run(std::shared_ptr<ReadingMemory> const &memory)
    {
        Require(memory && memory->ModelOwner(), L"inline code tests borrow the real accepted fixture model");
        Results results;
        auto cache = NativeMeasurementCache::Create();
        Require(static_cast<bool>(cache), L"inline code oracle enables real native measurement cache");
        std::atomic<uint64_t> latest{1};
        auto make = [&](std::shared_ptr<NativeMeasurementCache> const &metrics, PreviewTheme theme, double width = 260.0, double height = 2000.0)
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
            auto renderer = NativeDocumentRenderer::Create({}, cancel, key, std::move(theme), reading, {}, {}, {}, metrics);
            auto frame = renderer->CreateFrame(0, height, 1, latest);
            Require(frame && frame->main && frame->navigation && reading->fault == 0 && frame->main->Text() == FixtureMain,
                    L"inline code creates the fixed source Main and a complete native frame without reading faults");
            return frame;
        };
        auto fresh = make({}, Theme(false));
        auto cold = make(cache, Theme(false));
        auto before = cache->Stats();
        auto warm = make(cache, Theme(false));
        auto after = cache->Stats();
        Require(after.hits > before.hits, L"inline code warm oracle actually hits measurement cache");
        results.cacheHits = after.hits - before.hits;
        CheckRanges(*fresh, results);
        SameGeometry(*fresh, *cold);
        SameGeometry(*fresh, *warm);
        results.freshWarmGeometry = true;

        auto narrow = make({}, Theme(false), 80.0, 100000.0);
        auto narrowFence = std::find_if(narrow->Blocks().begin(), narrow->Blocks().end(),
                                        [](auto const &visual) { return visual.block.kind == 3; });
        Require(narrowFence != narrow->Blocks().end() && narrowFence->codeGutterWidth == 0 &&
                !narrowFence->texts.empty() &&
                narrowFence->texts.front().x == CodeBlockStyle::LeftPadding(static_cast<float>(narrowFence->block.width)),
                L"a rejected line-number gutter preserves the ordinary code inset and wrapping width");

        WarpTarget target;
        auto fence = std::find_if(fresh->Blocks().begin(), fresh->Blocks().end(),
                                  [](auto const &visual) { return visual.block.kind == 3; });
        Require(fence != fresh->Blocks().end() && fence->codeGutterWidth > 0 && fence->codeLines.size() == 3,
                L"three authored code lines have a measured gutter with no extra terminal row");
        auto const &codeLines = fence->codeLines;
        Require(codeLines[0].number == 98 && codeLines[1].number == 99 && codeLines[2].number == 100 &&
                codeLines[0].highlighted && codeLines[1].highlighted && !codeLines[2].highlighted &&
                codeLines[0].bottom - codeLines[0].top > 2 * fresh->theme.lineHeight,
                L"wraps and U+2028 stay in the same physical line; blank line has its own number and highlight");
        auto codeViewport = fresh->Reposition(fence->block.y - 4, PixelImage::height);
        auto codePixels = target.Draw(*codeViewport);
        auto sampleColumn = static_cast<uint32_t>(8 + fence->block.x + fence->block.width - 10);
        auto sampleRow = [&](float local)
        {
            return static_cast<uint32_t>(8 + fence->block.y - codeViewport->viewportY + local);
        };
        auto plainPixel = codePixels.At(sampleColumn, sampleRow((codeLines[2].top + codeLines[2].bottom) * 0.5f));
        auto blankPixel = codePixels.At(sampleColumn, sampleRow((codeLines[1].top + codeLines[1].bottom) * 0.5f));
        auto wrappedPixel = codePixels.At(sampleColumn, sampleRow(codeLines[0].bottom - fresh->theme.lineHeight * 0.5f));
        Require(plainPixel != blankPixel && blankPixel == wrappedPixel,
                L"real WARP highlights fill the full blank line and wrapped continuation before drawing code glyphs");
        // Move the second authored line's bottom from surface Y=76 to Y=12.
        // Its number and band still paint above the 16-DIP document origin.
        auto fullCode = fresh->Reposition(fence->block.y + codeLines[1].bottom - 60, PixelImage::height);
        auto clippedCode = fullCode->Reposition(fullCode->viewportY + 64, PixelImage::height);
        auto fullPixels = target.Draw(*fullCode, 0, 0, false, 16);
        auto clippedPixels = target.Draw(*clippedCode, 0, 0, false, 16);
        const auto gutterStart = static_cast<uint32_t>(8 + fence->block.x +
            CodeBlockStyle::LeftPadding(static_cast<float>(fence->block.width)));
        const auto gutterEnd = static_cast<uint32_t>(gutterStart + fence->codeGutterWidth - 8);
        uint32_t numberInk{};
        for (uint32_t row = 0; row < 11; ++row)
        {
            Require(clippedPixels.At(sampleColumn, row) == fullPixels.At(sampleColumn, row + 64),
                    L"the highlighted blank code line paints through the top padding after scrolling");
            for (uint32_t column = gutterStart; column < gutterEnd; ++column)
            {
                numberInk += fullPixels.At(column, row + 64) != fullPixels.At(sampleColumn, row + 64);
                Require(clippedPixels.At(column, row) == fullPixels.At(column, row + 64),
                        L"the clipped gutter number matches the same physical pixels before scrolling");
            }
        }
        Require(numberInk > 0, L"the clipped number pixel comparison contains actual DirectWrite glyph ink");
        auto chip = Find(L"chip");
        auto codeRegion = Region(*fresh, chip);
        results.plainChipWidth = PlainChipWidth();
        results.paddedChipWidth = codeRegion.right - codeRegion.left;
        Require(std::abs(results.paddedChipWidth - results.plainChipWidth - 7.2f) < 0.1f,
                L"inline chip advances by its independent natural glyph width plus 3.6 DIP breathing room at each edge");
        for (auto const &block : fresh->Blocks())
            for (auto const &text : block.texts)
                if (chip.start >= text.logicalStart && chip.end <= text.logicalEnd)
                {
                    auto spacing = text.layout.as<IDWriteTextLayout1>();
                    for (uint32_t position : {chip.start + 1, chip.start + 2})
                    {
                        FLOAT leading{}, trailing{}, minimum{};
                        winrt::check_hresult(spacing->GetCharacterSpacing(
                            static_cast<uint32_t>(position - text.logicalStart), &leading, &trailing, &minimum, nullptr));
                        Require(leading == 0 && trailing == 0 && minimum == 0,
                                L"inline code breathing room leaves interior character tracking unchanged");
                    }
                }
        results.spanEdgePadding = true;
        auto normalRegion = Region(*fresh, {chip.end + 1, chip.end + 3});
        auto light = target.Draw(*fresh);
        auto panel = Pixel(fresh->theme.panelBackground);
        results.lightPixels = Count(light, codeRegion, [&](uint32_t pixel, auto, auto) { return pixel == panel; });
        Require(results.lightPixels > 20, L"WARP has exact light panel fill pixels inside real inline code advances");
        Require(Count(light, normalRegion, [](uint32_t pixel, auto, auto) { return pixel != 0; }) == 0,
                L"ordinary spaces immediately beside inline code have neither background nor border pixels");
        auto selected = target.Draw(*fresh, chip.start, chip.end);
        auto selection = Pixel(fresh->theme.selectionBackground);
        results.selectedPixels = Count(light, codeRegion,
            [&](uint32_t pixel, uint32_t x, uint32_t y) { return pixel == panel && selected.At(x, y) == selection; });
        Require(results.selectedPixels > 20 && results.selectedPixels * 100 >= results.lightPixels * 95,
                L"selection paints over inline panel pixels before glyphs, allowing only text AA quantization");
        results.selectionOverBackground = true;
        auto darkFrame = make(cache, Theme(true));
        SameGeometry(*fresh, *darkFrame);
        auto highlightContrast = Theme(true);
        highlightContrast.highContrast = true;
        CheckHighlights(*fresh, *warm, *darkFrame, *make(cache, highlightContrast), target);
        auto dark = target.Draw(*darkFrame);
        auto darkPanel = Pixel(darkFrame->theme.panelBackground);
        results.darkPixels = Count(dark, codeRegion, [&](uint32_t pixel, auto, auto) { return pixel == darkPanel; });
        Require(results.darkPixels > 20 && darkPanel != panel &&
                    Count(dark, codeRegion, [&](uint32_t pixel, auto, auto) { return pixel == panel; }) == 0,
                L"dark theme uses its explicit current panel fill at unchanged geometry");
        auto restored = target.Draw(*fresh);
        Require(Count(restored, codeRegion, [&](uint32_t pixel, auto, auto) { return pixel == panel; }) == results.lightPixels,
                L"drawing dark then retained light frame does not retain a stale inline code brush");
        results.themedPixels = true;

        auto edgeTheme = Theme(true);
        edgeTheme.foreground = edgeTheme.linkForeground = edgeTheme.panelBackground = edgeTheme.background;
        edgeTheme.borderColor = {255, 247, 229, 71};
        auto normalEdge = make(cache, edgeTheme);
        edgeTheme.highContrast = true;
        auto highContrastEdge = make(cache, edgeTheme);
        auto normal = target.Draw(*normalEdge, 0, 0, true);
        auto highContrast = target.Draw(*highContrastEdge, 0, 0, true);
        auto edge = Pixel(edgeTheme.borderColor);
        auto borderRegion = codeRegion;
        borderRegion.left -= 1;
        borderRegion.right += 1;
        results.outlinePixels = Count(highContrast, borderRegion, [&](uint32_t pixel, auto, auto) { return pixel == edge; });
        Require(results.outlinePixels > 20 &&
                    Count(normal, borderRegion, [&](uint32_t pixel, auto, auto) { return pixel == edge; }) == 0,
                L"high contrast has a visible fully opaque outline while normal theme does not use an opaque edge");
        auto blended = [](uint8_t foreground, uint8_t background)
        { return static_cast<uint8_t>(std::lround(foreground * 0.25 + background * 0.75)); };
        auto bg = edgeTheme.background;
        auto border = edgeTheme.borderColor;
        auto expectedEdge = Pixel({255, blended(border.R, bg.R), blended(border.G, bg.G), blended(border.B, bg.B)});
        auto nearPixel = [](uint32_t pixel, uint32_t expected)
        {
            for (unsigned shift : {0u, 8u, 16u, 24u})
                if (std::abs(int((pixel >> shift) & 255) - int((expected >> shift) & 255)) > 1)
                    return false;
            return true;
        };
        Require(Count(normal, borderRegion, [&](uint32_t pixel, auto, auto) { return nearPixel(pixel, expectedEdge); }) > 20,
                L"normal inline code outline has the explicit quarter-opacity border color");
        results.highContrastOutline = true;
        CheckTable(*fresh, target, results);
        auto tallTheme = Theme(false);
        tallTheme.lineHeight = 90;
        auto tall = make({}, tallTheme);
        Require(tall->navigation->Line(0).height >= 89 && tall->totalHeight > fresh->totalHeight,
                L"independent tall-line oracle really increases the actual DWrite line geometry");
        uint64_t tallFragments{};
        for (auto const &block : tall->Blocks())
            for (auto const &text : block.texts)
                for (auto const &code : text.inlineCode)
                {
                    Require(code.bounds.bottom - code.bounds.top < 30,
                            L"inline badge stays text-sized in a real ninety-DIP line instead of filling its hit rectangle");
                    ++tallFragments;
                }
        Require(tallFragments > 0, L"ninety-DIP line oracle contains real code background fragments");
        results.tallLineCapped = true;
        results.detail = L"inline-code authored UTF16 ranges=" + std::to_wstring(results.ranges) +
                         L"; semantic fragments=" + std::to_wstring(results.fragments) + L", painted=" +
                         std::to_wstring(results.paintedFragments) + L", zero-advance marker=" +
                         std::to_wstring(results.zeroAdvanceFragments) + L" (no box; native acute and copy preserved); wrap=" +
                         std::to_wstring(results.wrappedFragments) + L", bidi=" + std::to_wstring(results.bidiFragments) +
                         L"; fresh/cold/warm exact Main+lines+selection; cache hits=" + std::to_wstring(results.cacheHits) +
                         L"; WARP exact light/dark/selected/HC pixels=" + std::to_wstring(results.lightPixels) + L"/" +
                         std::to_wstring(results.darkPixels) + L"/" + std::to_wstring(results.selectedPixels) + L"/" +
                         std::to_wstring(results.outlinePixels) + L"; ordinary gaps clear; quarter-opacity edge; 90-DIP line capped; chip natural/padded=" +
                         std::to_wstring(results.plainChipWidth) + L"/" + std::to_wstring(results.paddedChipWidth) +
                         L"; rounded table ordinary/selected transparent corners and interior grid pixels=" + std::to_wstring(results.tableGridPixels) +
                         L"; first-body list-table marker foreground gutter pixels=" + std::to_wstring(results.tableMarkerPixels) +
                         L"; Markdown and HTML highlight WARP light/dark/HC/selection, warm geometry and raised text. "
                         L"Fenced code paints numbered wrapped, Unicode-separated and blank lines with stable Main text.";
        return results;
    }

    std::wstring RunHtmlTables(std::shared_ptr<MainTextSnapshot const> const &main, notepads_markdown_core_cancel *sourceFlag)
    {
        Require(main && main->Text() == HtmlTableFixtureMain && sourceFlag,
                L"HTML table diagnostics use the independent authored TSV Main and actual accepted source");
        auto const &memory = main->Memory();
        auto references = memory.use_count();
        uint32_t aliases{}, ownerHits{}, metadataNegatives{};
        {
            std::atomic<uint64_t> latest{1};
            auto cache = NativeMeasurementCache::Create();
            auto make = [&](std::shared_ptr<NativeMeasurementCache> const &metrics, float width)
            {
                auto cancel = std::make_shared<PreviewCancellation>(main->ModelOwner().Get());
                auto reading = std::allocate_shared<ReadingSourceState>(ReadingAllocator<ReadingSourceState>{memory},
                    memory, cancel, main->SourceGeneration(), main->TextGeneration());
                reading->PublishMain(main);
                notepads_markdown_preview_key key{};
                key.header = {NOTEPADS_MARKDOWN_PREVIEW_ABI_VERSION, sizeof(key)};
                key.model = main->Identity();
                key.layout_generation = key.theme_generation = key.measurement_generation = 1;
                key.width = width;
                key.indent = 28;
                key.gap = 12;
                auto renderer = NativeDocumentRenderer::Create({}, cancel, key, Theme(false), reading, {}, {}, {}, metrics);
                auto frame = renderer->CreateFrame(0, 2000, 1, latest);
                Require(frame && !reading->fault && frame->main == main && frame->Blocks().size() == 4,
                        L"four authored HTML tables publish complete native Frames without fallback faults");
                return frame;
            };
            auto fresh = make({}, 320);
            auto cold = make(cache, 320);
            auto before = cache->Stats();
            auto warm = make(cache, 320);
            Require(cache->Stats().hits > before.hits, L"HTML table warm regression actually uses cached text metrics");
            for (auto const *candidate : {cold.get(), warm.get()})
            {
                Require(candidate->totalHeight == fresh->totalHeight &&
                            candidate->navigation->LineCount() == fresh->navigation->LineCount(),
                        L"HTML table cache hit/miss preserves measured height and exact reading line count");
                for (size_t block = 0; block < fresh->Blocks().size(); ++block)
                {
                    auto const &a = fresh->Blocks()[block];
                    auto const &b = candidate->Blocks()[block];
                    Require(a.rowHeights == b.rowHeights && a.columnEdges == b.columnEdges &&
                                a.tableCells.size() == b.tableCells.size() && a.texts.size() == b.texts.size(),
                            L"HTML caption/spans cache hit/miss preserves explicit row/column/owner geometry");
                    for (size_t text = 0; text < a.texts.size(); ++text)
                        Require(a.texts[text].logicalStart == b.texts[text].logicalStart &&
                                    a.texts[text].logicalEnd == b.texts[text].logicalEnd &&
                                    a.texts[text].x == b.texts[text].x && a.texts[text].y == b.texts[text].y &&
                                    a.texts[text].width == b.texts[text].width && a.texts[text].height == b.texts[text].height,
                                L"HTML text visual geometry is identical when metrics come from cache");
                }
            }
            auto const &empty = fresh->Blocks()[0];
            auto const &table = fresh->Blocks()[1];
            Require(empty.htmlTable && table.htmlTable && empty.tableCells.size() == 2 &&
                        empty.texts.size() == 1 && empty.rowHeights[0] == 0 && empty.rowHeights[1] > 0 &&
                        empty.tableCells[0].textIndex == NOTEPADS_MARKDOWN_PREVIEW_NONE &&
                        table.block.columns == 2 && table.block.row_count == 4 &&
                        table.block.cells.count == 8 && table.tableCells.size() == 6 && table.texts.size() == 6,
                    L"empty caption keeps its role with no visual row; caption/span covered slots have no text visual");
            auto const &rowspans = fresh->Blocks()[2];
            auto const &merged = rowspans.tableCells.front();
            Require(rowspans.htmlTable && rowspans.block.columns == 3 && rowspans.block.row_count == 3 &&
                        rowspans.tableCells.size() == 4 && merged.rowSpan == 2 && merged.columnSpan == 2 &&
                        merged.bounds.bottom == rowspans.rowHeights[0] + rowspans.rowHeights[1] &&
                        rowspans.tableOwners[3] == rowspans.tableOwners[0] && rowspans.tableOwners[4] == rowspans.tableOwners[0],
                    L"closed tbody extent and combined rowspan and colspan share one native owner across four slots");
            Require(fresh->descriptors->tables.size() == 1 &&
                        fresh->descriptors->tables[0].node == table.block.node &&
                        fresh->descriptors->tables[0].caption.start == table.block.logical_start &&
                        fresh->descriptors->Copy(*main, fresh->descriptors->tables[0].caption.start,
                            fresh->descriptors->tables[0].caption.end) == L"HTML 表格与跨列",
                    L"semantic table records one exact caption owner and omits the empty and absent captions");
            auto const &captionCell = table.tableCells[0];
            auto const &spanningCell = table.tableCells[3];
            auto const &caption = table.texts[captionCell.textIndex];
            auto const &spanning = table.texts[spanningCell.textIndex];
            Require(captionCell.columnSpan == 2 && spanningCell.columnSpan == 2 &&
                        captionCell.bounds.right - captionCell.bounds.left == table.tableWidth &&
                        spanningCell.bounds.right - spanningCell.bounds.left == table.tableWidth &&
                        caption.layout->GetTextAlignment() == DWRITE_TEXT_ALIGNMENT_CENTER &&
                        spanning.layout->GetMaxWidth() == table.tableWidth - 12,
                    L"caption and data colspan genuinely share both columns and one full-width text layout");
            DWRITE_FONT_WEIGHT captionWeight{}, headerWeight{}, bodyWeight{};
            winrt::check_hresult(caption.layout->GetFontWeight(0, &captionWeight, nullptr));
            winrt::check_hresult(table.texts[table.tableCells[1].textIndex].layout->GetFontWeight(0, &headerWeight, nullptr));
            winrt::check_hresult(spanning.layout->GetFontWeight(0, &bodyWeight, nullptr));
            Require(captionWeight == DWRITE_FONT_WEIGHT_NORMAL && headerWeight == DWRITE_FONT_WEIGHT_SEMI_BOLD &&
                        bodyWeight == DWRITE_FONT_WEIGHT_NORMAL,
                    L"per-cell HTML th is bold while synthetic caption and body td do not inherit GFM first-row headers");
            auto verify = [&](PreviewFrame const &frame)
            {
                auto const &navigation = *frame.navigation;
                Require(navigation.CanonicalCaret(0) == 1 && navigation.Snap(0, false) == 1 &&
                            navigation.Snap(0, true) == 1,
                        L"document-offset-zero empty caption resolves to the real empty body cell");
                auto selectText = [&](uint32_t start, uint32_t end)
                {
                    Require(start < end && end <= main->Length(), L"raw table selection probe is nonempty");
                    auto expected = main->Text().substr(start, end - start);
                    for (bool reversed : {false, true})
                    {
                        auto anchor = reversed ? end : start;
                        auto active = reversed ? start : end;
                        navigation.SnapSelection(anchor, active);
                        Require(anchor == (reversed ? end : start) && active == (reversed ? start : end) &&
                                    main->CopyText(std::min(anchor, active), end - start) == expected &&
                                    frame.descriptors->Copy(*main, std::min(anchor, active), std::max(anchor, active)) == expected,
                                L"nonempty selection/reinstallation preserves exact TSV tab/LF endpoints and direction, independently of caret aliases");
                    }
                };
                selectText(0, 1); // The omitted caption's real Main row LF.
                selectText(0, main->Length());
                using Unit = winrt::Windows::UI::Xaml::Automation::Text::TextUnit;
                for (auto unit : {Unit::Character, Unit::Word, Unit::Line, Unit::Paragraph, Unit::Document})
                {
                    auto range = navigation.Unit(0, unit);
                    Require(unit == Unit::Document ? range.start == 0 && range.end == main->Length()
                                                   : range.start >= 1 && range.end >= range.start,
                            L"full document unit retains Main TSV; smaller units retain canonical caption owners");
                    for (int32_t count : {-1, 0, 1})
                    {
                        int32_t moved{};
                        auto position = navigation.Move(0, unit, count, moved);
                        Require(position >= 1 && position <= main->Length(),
                                L"all movement counts normalize empty-caption aliases before predecessor/successor lookup");
                    }
                }
                for (auto const &block : frame.Blocks())
                {
                    for (auto const &alias : block.tableAliases)
                    {
                        if (alias.kind == ReadingTableAliasKind::ColumnSpan)
                        {
                            selectText(alias.target, alias.last); // Exact covered-tab-only range.
                            auto owner = std::find_if(block.texts.begin(), block.texts.end(), [&](auto const &text)
                                                      { return text.logicalEnd == alias.target; });
                            Require(owner != block.texts.end(), L"covered tabs have one visible owner for selection probes");
                            selectText(static_cast<uint32_t>(owner->logicalStart), alias.last); // Text plus its exact suffix tabs.
                        }
                        if (alias.kind == ReadingTableAliasKind::RowSpan)
                        {
                            selectText(std::min(alias.first, alias.target), std::max(alias.last, alias.target));
                            if (alias.target > alias.last)
                            {
                                int32_t moved{};
                                Require(alias.first > 0 &&
                                            navigation.Move(alias.first - 1, Unit::Character, 1, moved) == alias.target && moved == 1,
                                        L"Right jumps over vertically covered slots to the next authored cell");
                            }
                        }
                        for (uint32_t position = alias.first; position <= alias.last; ++position)
                        {
                            Require(navigation.CanonicalCaret(position) == alias.target &&
                                        navigation.CanonicalCaret(alias.target) == alias.target &&
                                        navigation.SnapTextBoundary(position, false) == position &&
                                        navigation.SnapTextBoundary(position, true) == position &&
                                        navigation.Snap(position, false) == alias.target &&
                                        navigation.Snap(position, true) == alias.target &&
                                        navigation.LineAt(position) == navigation.LineAt(alias.target) &&
                                        navigation.RevealY(position) == navigation.RevealY(alias.target) &&
                                        navigation.FollowY(position) == navigation.FollowY(alias.target) &&
                                        navigation.ContinuousFollowY(position) == navigation.ContinuousFollowY(alias.target),
                                    L"covered/caption alias is idempotent in all native caret/reveal/follow consumers");
                            auto a = NativeDocumentRenderer::Rectangles(frame, position, position);
                            auto b = NativeDocumentRenderer::Rectangles(frame, alias.target, alias.target);
                            Require(a.size() == 1 && b.size() == 1 && a[0].x == b[0].x && a[0].y == b[0].y &&
                                        a[0].height > 0 && a[0].height == b[0].height,
                                    L"covered/caption source caret has its real owner boundary rectangle");
                            ++aliases;
                        }
                    }
                    for (auto const &cell : block.tableCells)
                    {
                        if (cell.textIndex == NOTEPADS_MARKDOWN_PREVIEW_NONE)
                            continue;
                        auto const &text = block.texts[cell.textIndex];
                        for (float fraction : {0.05f, 0.5f, 0.95f})
                        {
                            auto x = static_cast<float>(block.block.x) + cell.bounds.left +
                                (cell.bounds.right - cell.bounds.left) * fraction;
                            auto y = static_cast<float>(block.block.y - frame.viewportY) + cell.bounds.bottom - 2;
                            auto hit = NativeDocumentRenderer::HitTest(frame, x, y);
                            Require(hit && !hit->inside && hit->position >= text.logicalStart &&
                                        hit->position <= text.logicalEnd && hit->cluster.start >= text.logicalStart &&
                                        hit->cluster.end <= text.logicalEnd,
                                    L"both halves and bottom padding of each physical cell hit its actual owner, never a covered/next-row cell");
                            ++ownerHits;
                        }
                    }
                }
                auto inverse = navigation.ContinuousFollowSpan(0);
                Require(inverse.start >= 1 && inverse.end >= inverse.start,
                        L"inverse follow does not reintroduce the omitted caption document start");
                Require(frame.main->CopyText(0, main->Length()) == HtmlTableFixtureMain &&
                            frame.descriptors->Copy(*main, 0, main->Length()) == HtmlTableFixtureMain,
                        L"raw Main TSV and semantic copy retain covered tabs independently of presentation aliases");
            };
            verify(*fresh);
            verify(*cold);
            verify(*warm);
            auto narrow = make(cache, 120);
            verify(*narrow);
            auto repositioned = fresh->Reposition(10, 250);
            verify(*repositioned);
            WarpTarget target;
            auto pixels = target.Draw(*fresh);
            auto x = 8 + static_cast<float>(table.block.x);
            auto y = 8 + static_cast<float>(table.block.y);
            auto blank = spanningCell.bounds;
            auto vertical = Count(pixels, {x + table.tableWidth / 2 - 1, y + blank.bottom - 4,
                                          x + table.tableWidth / 2 + 1, y + blank.bottom - 2},
                                  [](uint32_t pixel, auto, auto) { return pixel != 0; });
            auto headerBottom = y + table.rowHeights[0] + table.rowHeights[1];
            Require(vertical == 0 && Count(pixels, {x + 12, headerBottom - 1, x + table.tableWidth - 12, headerBottom + 1},
                        [](uint32_t pixel, auto, auto) { return pixel != 0; }) > 20,
                    L"HTML WARP paints the header/body horizontal separator without an internal vertical rule through colspan");

            auto spanViewport = fresh->Reposition(rowspans.block.y - 4, PixelImage::height);
            auto spanPixels = target.Draw(*spanViewport);
            auto spanX = 8 + static_cast<float>(rowspans.block.x);
            auto dividerY = 8 + static_cast<float>(rowspans.block.y - spanViewport->viewportY) + rowspans.rowHeights[0];
            auto separated = Count(spanPixels,
                {spanX + rowspans.columnEdges[2] + 8, dividerY - 1, spanX + rowspans.tableWidth - 8, dividerY + 1},
                [](uint32_t pixel, auto, auto) { return pixel != 0; });
            auto inside = Count(spanPixels,
                {spanX + rowspans.columnEdges[1], dividerY - 1, spanX + rowspans.columnEdges[2] - 8, dividerY + 1},
                [](uint32_t pixel, auto, auto) { return pixel != 0; });
            Require(separated > 20 && inside == 0,
                    L"HTML WARP splits a horizontal divider at the edges of a vertically merged owner");

            notepads_markdown_preview_cell cell{};
            cell.node = 9;
            notepads_markdown_preview_html_cell valid{9, 2, NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_HTML, 0, 1, 0};
            ValidatePreviewHtmlCell(cell, valid);
            auto reject = [&](auto change)
            {
                auto value = valid;
                change(value);
                try { ValidatePreviewHtmlCell(cell, value); }
                catch (ReadingFailure const &failure)
                {
                    Require(failure.status == NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE, L"invalid HTML cell wire fails closed");
                    ++metadataNegatives;
                    return;
                }
                throw winrt::hresult_error(E_FAIL, L"invalid HTML cell wire was accepted");
            };
            reject([](auto &value) { value.node = 10; });
            reject([](auto &value) { value.reserved = 1; });
            reject([](auto &value) { value.flags = 8; });
            reject([](auto &value) { value.flags = NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_COVERED; });
            reject([](auto &value) { value.colspan = 0; });
            reject([](auto &value) { value.colspan = 65; });
            reject([](auto &value) { value.flags = 0; });
            reject([](auto &value) { value.flags |= NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_CAPTION; });
            std::array<notepads_markdown_preview_cell, 2> row{};
            row[0].node = 10;
            row[0].row_node = row[1].row_node = 50;
            row[1].node = 11;
            row[1].column = 1;
            std::array<notepads_markdown_preview_html_cell, 2> rowMetadata{
                notepads_markdown_preview_html_cell{10, 2, NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_HTML, 0, 1, 0},
                notepads_markdown_preview_html_cell{11, 0, NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_HTML | NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_COVERED, 0, 0, 0}};
            std::array<uint32_t, 2> owners{};
            ValidatePreviewHtmlTable(row, rowMetadata, 1, 2, owners);
            auto rejectRow = [&](auto change)
            {
                auto badCells = row;
                auto badMetadata = rowMetadata;
                change(badCells, badMetadata);
                try { ValidatePreviewHtmlTable(badCells, badMetadata, 1, 2, owners); }
                catch (ReadingFailure const &failure)
                {
                    Require(failure.status == NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE, L"malformed HTML row topology fails closed");
                    ++metadataNegatives;
                    return;
                }
                throw winrt::hresult_error(E_FAIL, L"invalid HTML row partition was accepted");
            };
            rejectRow([](auto &, auto &values) { values[0].colspan = 0; values[0].flags |= NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_COVERED; });
            rejectRow([](auto &, auto &values) { values[1].colspan = 1; values[1].flags = NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_HTML; });
            rejectRow([](auto &values, auto &) { values[1].row = 1; });
            rejectRow([](auto &, auto &values) { values[0].colspan = 3; });
            rejectRow([](auto &, auto &values) { values[1].flags |= NOTEPADS_MARKDOWN_PREVIEW_HTML_CELL_CAPTION; });
            rejectRow([](auto &values, auto &) { values[1].row_node = 51; });
            rejectRow([](auto &, auto &values) { values[0].rowspan = 2; });
            rejectRow([](auto &, auto &values) { values[1].rowspan = 1; });
            Require(notepads_markdown_core_cancel_request(sourceFlag) == NOTEPADS_MARKDOWN_CORE_OK, L"HTML source producer is canceled before old Frame reads");
            verify(*fresh);
        }
        Require(memory.use_count() == references && metadataNegatives == 16 && aliases > 0 && ownerHits > 0,
                L"HTML table diagnostics execute all exact probes and release funded owner/alias/Frame storage");
        return L" HTML tables: independent TSV/8-slot 6-owner CASE plus zero-height offset-zero empty caption; "
               L"true colspan/caption width and per-th font; cache hit/miss; owner padding hits and alias "
               L"Unit/Move/Snap/Reveal/Follow/rectangles; narrow/Reposition/canceled-source retained Frames; "
               L"exact forward/reverse tab-only, suffix-tab and offset-zero LF range endpoints; "
               L"raw TSV/semantic copy; WARP grid skips vertical spans; sixteen malformed wire/topology facts; "
               L"all funded owners retired. Content-fit and physical UI qualification are not implied.";
    }
}
