// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeImageReuseControlTests.h"
#include "NativeImageFontControlTests.h"
#include "Automation/ReadingObjectProvider.h"
#include "Control/MarkdownView.h"
#include "Resources/Images/NativeImageViewport.h"
#include <winrt/Windows.UI.Xaml.Controls.h>
#include <winrt/Windows.Foundation.Collections.h>
#include <functional>
#include <cmath>
#include <source_location>
#include <robuffer.h>
#include <utility>
#include <sstream>
#include <iomanip>

namespace winrt::Notepads::Markdown::implementation
{
    bool ImageReuseControlTestAccess::HoverPendingImage(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        if (!control->ReadingAvailable())
            return false;
        auto const &presented = control->_presented;
        auto const &frame = *presented.frame;
        for (auto const &block : frame.Blocks())
            for (auto const &text : block.texts)
                for (auto const &image : text.images)
                {
                    auto x = static_cast<float>(block.block.x) + text.x + image.x + std::min(image.width / 2, 4.f);
                    auto y = static_cast<float>(block.block.y - frame.viewportY) + text.y + image.y +
                             std::min(image.height / 2, 4.f);
                    auto hit = ::Notepads::Markdown::Native::NativeDocumentRenderer::ImageAt(frame, x, y);
                    if (!hit || hit->ready)
                        continue;
                    Windows::Foundation::Point point{
                        static_cast<float>(presented.geometry.x) + control->_padding + x,
                        static_cast<float>(presented.geometry.y) + control->_padding + y};
                    control->_readingLinkPoint = view.TransformToVisual(nullptr).TransformPoint(point);
                    control->TrackReadingHover(point, control->ReadingLinkAtPointer(presented));
                    return control->_readingHover &&
                           control->_readingHover->kind == MarkdownView::ReadingHoverTarget::Kind::Fallback;
                }
        return false;
    }

    hstring ImageReuseControlTestAccess::HoverText(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        auto const &tooltip = control->_readingHoverToolTip;
        if (!tooltip || !tooltip.IsOpen())
            return {};
        return tooltip.Content().as<Windows::UI::Xaml::Controls::TextBlock>().Text();
    }

    bool ImageReuseControlTestAccess::ReadyImageAutomation(Markdown::MarkdownView const &view)
    {
        namespace Native = ::Notepads::Markdown::Native;
        auto control = get_self<MarkdownView>(view);
        auto snapshot = control->AutomationSnapshot();
        auto &published = control->_publishedImagePresentation;
        if (!snapshot || snapshot->descriptors->objects.size() != 1 || !published || published->entries.size() != 1)
            return false;
        auto const &object = snapshot->descriptors->objects.front();
        if (object.kind != Native::ReadingObjectKind::Image || object.object != published->entries.front().state.object ||
            published->entries.front().VisibleState() != NOTEPADS_MARKDOWN_IMAGE_READY)
            return false;

        auto peer = control->ReadingPeer(snapshot, Native::ReadingChildKind::Object, 0);
        auto testPresentation = std::allocate_shared<Native::ImagePresentationSnapshot>(
            Native::ReadingAllocator<Native::ImagePresentationSnapshot>{published->entries.get_allocator()}, *published);
        // Keep a real ready thumbnail and live peer while exercising the status variants.
        struct RestorePresentation
        {
            std::shared_ptr<Native::ImagePresentationSnapshot const> &published;
            std::shared_ptr<Native::ImagePresentationSnapshot const> original;
            ~RestorePresentation() { published = std::move(original); }
        } restore{published, published};

        auto &entry = testPresentation->entries.front();
        entry.state.status_flags |= NOTEPADS_MARKDOWN_IMAGE_TARGET_LIMITED;
        published = testPresentation;
        if (peer->GetNameCore() != L"same, image loaded" || peer->GetHelpTextCore() != L"Image loaded: resolution limited")
            return false;
        entry.state.failure = NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR;
        if (peer->GetNameCore() != L"same, image loaded" || peer->GetHelpTextCore() != L"Image loaded: resolution update failed")
            return false;
        entry.pixels.reset();
        return peer->GetNameCore() == L"same, image not loaded" && peer->GetHelpTextCore() == L"Image not loaded: released";
    }

    void ImageReuseControlTestAccess::PrepareInitialViewport(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        if (!control->_loaded || !control->_previewScroll || control->_documentAccepted)
            throw hresult_error{E_FAIL, L"Initial image viewport requires a loaded empty view"};
        control->_previewScroll.VerticalScrollMode(Windows::UI::Xaml::Controls::ScrollMode::Enabled);
        control->_previewScroll.VerticalScrollBarVisibility(Windows::UI::Xaml::Controls::ScrollBarVisibility::Visible);
        view.UpdateLayout();
    }
    bool ImageReuseControlTestAccess::InitialViewportReady(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        if (!control->_loaded || !control->_previewScroll || control->_documentAccepted)
            return false;
        auto scroll = control->_previewScroll;
        auto actualWidth = view.ActualWidth();
        auto viewportWidth = scroll.ViewportWidth();
        auto viewportHeight = scroll.ViewportHeight();
        auto zoom = scroll.ZoomFactor();
        return scroll.VerticalScrollMode() == Windows::UI::Xaml::Controls::ScrollMode::Enabled &&
            scroll.VerticalScrollBarVisibility() == Windows::UI::Xaml::Controls::ScrollBarVisibility::Visible &&
            scroll.ComputedVerticalScrollBarVisibility() == Windows::UI::Xaml::Visibility::Visible &&
            std::isfinite(actualWidth) && std::isfinite(viewportWidth) && std::isfinite(viewportHeight) &&
            std::isfinite(zoom) && actualWidth > 0 && viewportWidth > 0 && viewportWidth < actualWidth &&
            viewportHeight > 0 && zoom > 0;
    }
    bool ImageReuseControlTestAccess::ReplacesPendingWheelAtTop(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        auto scroll = control->ReadingScrollViewer();
        if (!scroll || scroll.VerticalOffset() != 0 || scroll.ScrollableHeight() <= 0)
            return false;
        // Seed the wheel target without relying on the OS animation clock.
        control->_wheelScroll.Publish(scroll.ScrollableHeight(), -120);
        return view.TrySetVerticalScrollProgress(0) && !control->_wheelScroll.Active() &&
            scroll.VerticalOffset() == 0;
    }

    bool ImageReuseControlTestAccess::PumpImageAtCurrentViewport(Markdown::MarkdownView const &view, bool offscreen)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        auto state = Observe(view);
        auto frame = control->_presented.frame;
        if (!state.current || !state.found || !frame || !frame->navigation || !control->_previewScroll ||
            !control->CurrentViewportFrame(*frame, control->_viewportGeometry))
            return false;
        int64_t pixelRow{};
        double presentY{};
        if (!::Notepads::Markdown::Native::QuantizeViewportY(control->_previewScroll.VerticalOffset(),
            control->_viewportGeometry.scale, pixelRow, presentY) || presentY != frame->viewportY)
            return false;
        auto viewport = ::Notepads::Markdown::Native::ImageViewportWindow::FromCanvas(
            frame->viewportY, frame->viewportHeight, frame->theme.padding);
        auto const &line = frame->navigation->Line(frame->navigation->LineAt(state.textStart));
        if (!viewport || viewport->Near(line.y, line.y + line.height) == offscreen)
            return false;
        // Run the real warm trim at this published viewport. No row, lease,
        // pixels or admission state is manufactured by the test.
        control->PumpImages();
        return true;
    }
}

namespace Notepads::Markdown::Native::ImageReuseControlTests
{
    namespace
    {
        using namespace winrt;
        using namespace winrt::Windows::Foundation;
        using namespace winrt::Windows::Storage::Streams;
        using View = winrt::Notepads::Markdown::MarkdownView;
        using Access = winrt::Notepads::Markdown::implementation::ImageReuseControlTestAccess;
        using FontAccess = winrt::Notepads::Markdown::implementation::ImageFontControlTestAccess;
        void Require(bool value, std::source_location location = std::source_location::current())
        {
            if (!value) throw hresult_error{E_FAIL, L"Image reuse control assertion at " + std::to_wstring(location.line())};
        }
        // Diagnostic storage contains values and a weak UI link only. No task,
        // request, source/model, Frame or payload owner enters this transcript.
        struct ValueTrace
        {
            ImageReuseControlObservation state{};
            double actualWidth{}, actualHeight{}, viewportWidth{}, measuredWidth{};
            uint64_t layout{}, loadedGeneration{}, font{}, barrier{};
            bool observed{};
            HRESULT diagnosticStatus{S_OK};
            notepads_markdown_image_process_capacity physical{};
            int32_t physicalStatus{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        };
        struct OpenTrace
        {
            winrt::guid document{};
            uint64_t revision{}, text{}, image{}, request{}, provider{};
            bool onUi{}, pendingMatches{};
            ValueTrace value;
            notepads_markdown_image_raster_plan plan{};
            int32_t planStatus{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        };
        ValueTrace CaptureValues(winrt::weak_ref<View> const &weak) noexcept
        {
            ValueTrace value;
            try
            {
                if (auto view = weak.get())
                {
                    value.state = Access::Observe(view);
                    auto geometry = FontAccess::Observe(view);
                    value.actualWidth = geometry.actualWidth; value.actualHeight = view.ActualHeight();
                    value.viewportWidth = geometry.viewportWidth; value.measuredWidth = geometry.measuredWidth;
                    value.layout = geometry.layoutGeneration; value.loadedGeneration = geometry.loadedGeneration;
                    value.font = geometry.fontGeneration; value.barrier = geometry.barrier;
                    value.observed = true;
                }
                value.physical.header = {2, sizeof(value.physical)};
                notepads_markdown_core_error error{};
                error.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(error)};
                value.physicalStatus = notepads_markdown_image_get_process_capacity(&value.physical, &error);
            }
            catch (...) { value.diagnosticStatus = winrt::to_hresult(); }
            return value;
        }
        struct Counts
        {
            guid document{};
            uint64_t generation{1};
            uint32_t ui{GetCurrentThreadId()};
            uint8_t blue{0x20};
            uint32_t width{1024}, height{1024};
            std::atomic<uint32_t> opens{}, reads{}, returned{}, closes{}, badClose{}, wrongRequest{};
            bool gateNext{};
            bool animated{};
            winrt::handle entered{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
            winrt::handle proceed{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
            std::function<void()> onOpen;
            winrt::weak_ref<View> view;
            std::mutex traceMutex;
            std::array<OpenTrace, 32> openTrace;
            uint32_t traceCount{}, traceOverflow{};
        };
        void RecordOpen(Counts &counts, winrt::Notepads::Markdown::MarkdownImageRequest const &request) noexcept
        {
            OpenTrace trace;
            try
            {
                trace.document = request.DocumentId(); trace.revision = request.Revision(); trace.text = request.TextGeneration();
                trace.image = request.ImageId(); trace.request = request.RequestId(); trace.provider = request.ProviderGeneration();
                trace.onUi = GetCurrentThreadId() == counts.ui;
                if (trace.onUi)
                {
                    trace.value = CaptureValues(counts.view);
                    auto const &state = trace.value.state;
                    trace.pendingMatches = trace.value.observed && state.found && state.row.pending_request == trace.request &&
                        state.row.object == trace.image && state.text == trace.text && state.epoch.source.revision == trace.revision &&
                        state.epoch.provider_generation == trace.provider &&
                        std::memcmp(state.epoch.source.document, &trace.document, sizeof(trace.document)) == 0;
                    // Start captured desired_target immediately before the
                    // synchronous provider call, with no UI await between them.
                    // Only a matching current pending row is used as its target.
                    if (trace.pendingMatches)
                    {
                        auto intrinsic = MakeRasterImageMetadata({counts.width, counts.height, counts.width, counts.height, 1, counts.width * 4});
                        trace.plan.header = {2, sizeof(trace.plan)};
                        notepads_markdown_core_error error{};
                        error.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(error)};
                        trace.planStatus = notepads_markdown_image_plan_raster(&intrinsic, &state.row.desired_target, &trace.plan, &error);
                    }
                }
            }
            catch (...) { trace.value.diagnosticStatus = winrt::to_hresult(); }
            std::lock_guard guard(counts.traceMutex);
            if (counts.traceCount < counts.openTrace.size()) counts.openTrace[counts.traceCount++] = trace;
            else ++counts.traceOverflow;
        }
        void DescribeTarget(std::wostringstream &out, wchar_t const *name, notepads_markdown_image_target const &target)
        {
            out << name << L"(font/generation/width/scale)=" << target.font_generation << L'/' << target.target_generation
                << L'/' << target.available_width_dip << L'/' << target.effective_physical_scale;
        }
        void DescribeValues(std::wostringstream &out, ValueTrace const &value)
        {
            auto const &s = value.state; auto const &row = s.row;
            out << L" observed/diag=" << value.observed << L'/' << value.diagnosticStatus
                << L" geometry(actualW/H/viewport/measured)=" << value.actualWidth << L'/' << value.actualHeight << L'/'
                << value.viewportWidth << L'/' << value.measuredWidth << L" layout/loaded/font/barrier=" << value.layout
                << L'/' << value.loadedGeneration << L'/' << value.font << L'/' << value.barrier
                << L" loaded/current/found=" << s.loaded << L'/' << s.current << L'/' << s.found
                << L" epoch(revision/source/provider)/text=" << s.epoch.source.revision << L'/' << s.epoch.source_generation
                << L'/' << s.epoch.provider_generation << L'/' << s.text << L" sourceDocumentBytes=";
            for (auto byte : s.epoch.source.document) out << std::hex << std::setw(2) << std::setfill(L'0') << unsigned(byte);
            out << std::dec << std::setfill(L' ') << L" row(object/state/published/pending/flags)=" << row.object << L'/' << row.state
                << L'/' << row.published_request << L'/' << row.pending_request << L'/' << row.status_flags
                << L" failure/error/detail=" << row.failure << L'/' << row.error << L'/' << row.detail
                << L" ready=" << (row.state == NOTEPADS_MARKDOWN_IMAGE_READY) << L' ';
            DescribeTarget(out, L"desired", row.desired_target); out << L' '; DescribeTarget(out, L"published", row.published_target);
            out << L" decoded(W/H/stride/flags)=" << row.metadata.decoded_width << L'/' << row.metadata.decoded_height
                << L'/' << row.metadata.stride << L'/' << row.metadata.flags << L" cache(count/scope/metadata/encoded)/rowEncoded="
                << s.cacheCount << L'/' << s.cacheScope << L'/' << s.cacheMetadata << L'/' << s.cacheEncoded << L'/' << s.rowEncoded
                << L" usage(active/reservations/encoded/decoded)=" << s.usage.active_requests << L'/' << s.usage.reservations
                << L'/' << s.usage.encoded_bytes << L'/' << s.usage.decoded_bytes << L" physical(status/gen/encoded/decoded)="
                << value.physicalStatus << L'/' << value.physical.generation << L'/' << value.physical.encoded_bytes << L'/' << value.physical.decoded_bytes;
        }
        std::wstring DescribeDiagnostics(Counts &counts, wchar_t const *name)
        {
            std::wostringstream out; out << std::setprecision(17) << L"; " << name;
            std::lock_guard guard(counts.traceMutex);
            out << L" counts(opens/returned/reads/closes/badClose/wrongRequest)="
                << counts.opens.load() << L'/' << counts.returned.load() << L'/' << counts.reads.load() << L'/'
                << counts.closes.load() << L'/' << counts.badClose.load() << L'/' << counts.wrongRequest.load();
            out << L" OpenTrace(count/overflow)=" << counts.traceCount << L'/' << counts.traceOverflow;
            for (uint32_t index = 0; index < counts.traceCount; ++index)
            {
                auto const &trace = counts.openTrace[index];
                out << L"\nOpen[" << index + 1 << L"] document=" << winrt::to_hstring(trace.document).c_str()
                    << L" publicKey(rev/text/image/request/provider)=" << trace.revision << L'/' << trace.text << L'/' << trace.image
                    << L'/' << trace.request << L'/' << trace.provider << L" ui/pendingMatches=" << trace.onUi << L'/' << trace.pendingMatches;
                DescribeValues(out, trace.value);
                out << L" physicalPlan(status/W/H/stride/maxSide/bytes)=" << trace.planStatus << L'/' << trace.plan.width << L'/'
                    << trace.plan.height << L'/' << trace.plan.stride << L'/' << trace.plan.maximum_side << L'/' << trace.plan.bytes;
            }
            out << L"\nCurrent[" << name << L']'; DescribeValues(out, CaptureValues(counts.view));
            return out.str();
        }
        struct Stream : implements<Stream, IRandomAccessStream, IInputStream, IOutputStream, IClosable>
        {
            std::vector<uint8_t> bytes;
            uint64_t position{};
            std::shared_ptr<Counts> counts;
            std::atomic<uint32_t> closed{};
            Stream(std::vector<uint8_t> value, std::shared_ptr<Counts> state) : bytes(std::move(value)), counts(std::move(state)) {}
            bool CanRead() const { return true; }
            bool CanWrite() const { return false; }
            uint64_t Position() const { return position; }
            uint64_t Size() const { return bytes.size(); }
            void Size(uint64_t) const { throw hresult_not_implemented(); }
            void Seek(uint64_t value) { Require(value <= bytes.size()); position = value; }
            IInputStream GetInputStreamAt(uint64_t) const { throw hresult_not_implemented(); }
            IOutputStream GetOutputStreamAt(uint64_t) const { throw hresult_not_implemented(); }
            IRandomAccessStream CloneStream() const { throw hresult_not_implemented(); }
            IAsyncOperationWithProgress<IBuffer, uint32_t> ReadAsync(IBuffer const &buffer, uint32_t count, InputStreamOptions)
            {
                auto self = get_strong();
                ++counts->reads;
                Require(!closed.load() && position <= bytes.size());
                auto length = static_cast<uint32_t>((std::min)(uint64_t{count}, bytes.size() - position));
                Require(length <= buffer.Capacity());
                BYTE *target{};
                winrt::check_hresult(buffer.as<::Windows::Storage::Streams::IBufferByteAccess>()->Buffer(&target));
                Require(target || !length);
                if (length) std::memcpy(target, bytes.data() + position, length);
                buffer.Length(length); position += length;
                co_return buffer;
            }
            IAsyncOperationWithProgress<uint32_t, uint32_t> WriteAsync(IBuffer const &) const { throw hresult_not_implemented(); }
            IAsyncOperation<bool> FlushAsync() const { throw hresult_not_implemented(); }
            void Close()
            {
                ++counts->closes;
                if (closed.fetch_add(1)) ++counts->badClose;
            }
        };
        std::vector<uint8_t> Bitmap(uint8_t blue, uint32_t width, uint32_t height)
        {
            auto stride = (width * 3 + 3) & ~3u;
            std::vector<uint8_t> bytes(54 + height * stride);
            auto put = [&](size_t at, uint32_t value)
                { for (size_t part = 0; part < 4; ++part) bytes[at + part] = static_cast<uint8_t>(value >> (8 * part)); };
            bytes[0] = 'B'; bytes[1] = 'M'; put(2, static_cast<uint32_t>(bytes.size())); put(10, 54); put(14, 40);
            put(18, width); put(22, height); bytes[26] = 1; bytes[28] = 24; put(34, height * stride);
            for (size_t at = 54; at < bytes.size(); at += 3)
            { bytes[at] = blue; bytes[at + 1] = 0x80; bytes[at + 2] = 0xe0; }
            return bytes;
        }
        std::vector<uint8_t> AnimatedBitmap()
        {
            // Original 1x1 black/white GIF89a; two 100-ms frames loop forever.
            return {'G', 'I', 'F', '8', '9', 'a', 1, 0, 1, 0, 0x80, 0, 0,
                0, 0, 0, 255, 255, 255,
                0x21, 0xff, 11, 'N', 'E', 'T', 'S', 'C', 'A', 'P', 'E', '2', '.', '0', 3, 1, 0, 0, 0,
                0x21, 0xf9, 4, 4, 10, 0, 0, 0, 0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 1, 0,
                0x21, 0xf9, 4, 4, 10, 0, 0, 0, 0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x4c, 1, 0, 0x3b};
        }
        struct Provider : implements<Provider, winrt::Notepads::Markdown::IMarkdownImageProvider>
        {
            std::shared_ptr<Counts> counts;
            explicit Provider(std::shared_ptr<Counts> state) : counts(std::move(state)) {}
            IAsyncOperation<IRandomAccessStream> OpenAsync(winrt::Notepads::Markdown::MarkdownImageRequest request)
            {
                auto self = get_strong();
                ++counts->opens;
                RecordOpen(*counts, request);
                if (GetCurrentThreadId() != counts->ui || request.DocumentId() != counts->document ||
                    request.ProviderGeneration() != counts->generation || !request.Revision() ||
                    !request.TextGeneration() || !request.RequestId() || request.Destination() != L"reuse-control:bmp")
                    ++counts->wrongRequest;
                auto action = std::move(counts->onOpen);
                if (action) action(); // Real synchronous host reentry, before any await.
                auto gate = std::exchange(counts->gateNext, false);
                // This completed ordinary provider path has no prior await,
                // so onOpen above is true synchronous Open reentry. The gate
                // variant separately exercises late physical return.
                auto output = make<Stream>(counts->animated ? AnimatedBitmap() :
                    Bitmap(counts->blue, counts->width, counts->height), counts);
                if (gate)
                {
                    check_bool(SetEvent(counts->entered.get()));
                    co_await resume_on_signal(counts->proceed.get());
                }
                ++counts->returned;
                co_return output;
            }
        };
        IAsyncAction WaitUi(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                            std::shared_ptr<CoreValidation::Context> context, uint64_t deadline,
                            std::function<bool()> predicate)
        {
            while (!predicate())
            {
                context->Arbitrate();
                if (GetTickCount64() >= deadline) throw hresult_error{HRESULT_FROM_WIN32(ERROR_TIMEOUT), L"Image reuse UI predicate timed out"};
                co_await resume_after(std::chrono::milliseconds(10));
                co_await resume_foreground(dispatcher);
            }
        }
        bool Ready(View const &view, uint64_t revision, uint64_t provider)
        {
            auto value = Access::Observe(view);
            return value.current && value.found && value.epoch.source.revision == revision &&
                value.epoch.provider_generation == provider && value.row.state == NOTEPADS_MARKDOWN_IMAGE_READY &&
                value.row.published_request && Access::Pixels(view) && !value.rowEncoded;
        }
        notepads_markdown_image_process_capacity PhysicalCapacity()
        {
            notepads_markdown_image_process_capacity value{};
            value.header = {2, sizeof(value)};
            notepads_markdown_core_error error{};
            error.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(error)};
            Require(notepads_markdown_image_get_process_capacity(&value, &error) == NOTEPADS_MARKDOWN_CORE_OK);
            return value;
        }
        IAsyncAction OffscreenReadyAsync(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                         std::shared_ptr<CoreValidation::Context> context,
                                         std::shared_ptr<FailureDiagnostics> diagnostics)
        {
            using namespace winrt::Windows::UI::Xaml::Controls;
            auto deadline = GetTickCount64() + 45000;
            auto parent = winrt::Windows::UI::Xaml::Window::Current().Content().as<Panel>();
            Grid host;
            View view{nullptr};
            auto counts = std::make_shared<Counts>();
            std::weak_ptr<ImagePixels const> pixels;
            std::exception_ptr failure;
            std::wstring stage = L"setup";
            bool inserted{};
            try
            {
                co_await WaitUi(dispatcher, context, deadline, []
                    { auto p = PhysicalCapacity(); return !p.encoded_bytes && !p.decoded_bytes; });
                GUID document{}; check_hresult(CoCreateGuid(&document));
                std::memcpy(&counts->document, &document, sizeof(document));
                counts->width = counts->height = 2048;
                host.Width(2200); host.Height(240); Grid::SetRow(host, 2);
                view = winrt::make<winrt::Notepads::Markdown::implementation::MarkdownView>();
                view.Width(2200); view.Height(240); view.CacheDocumentOnUnload(false);
                counts->view = winrt::make_weak(view);
                view.SetImageProvider(winrt::make<Provider>(counts), 1);
                host.Children().Append(view); parent.Children().Append(host); inserted = true;
                co_await WaitUi(dispatcher, context, deadline, [&]
                    { return Access::Observe(view).loaded && view.ActualWidth() > 2048; });
                Access::PrepareInitialViewport(view);
                co_await WaitUi(dispatcher, context, deadline, [&]
                    { host.UpdateLayout(); return Access::InitialViewportReady(view); });
                std::string source = "![same](reuse-control:bmp)\n\n";
                for (uint32_t index = 0; index < 96; ++index)
                    source += "A paragraph below the retained image.\n\n";
                stage = L"initial immutable pixels above the former warm target";
                auto update = view.SetDocumentAsync(counts->document, 1,
                    {reinterpret_cast<uint8_t const *>(source.data()), static_cast<uint32_t>(source.size())});
                co_await update; update = nullptr;
                co_await WaitUi(dispatcher, context, deadline, [&]
                    { return Ready(view, 1, 1) && !Access::Observe(view).usage.active_requests && counts->closes == counts->opens; });
                auto first = Access::Pixels(view);
                auto before = Access::Observe(view);
                Require(first && first->IsFrozen() && first->pixels.capacity() > 8ull * 1024 * 1024 &&
                    first->pixels.capacity() + before.cacheEncoded + before.cacheMetadata < 128ull * 1024 * 1024);
                Require(counts->opens == 1 && counts->returned == 1 && counts->reads >= 2 && counts->closes == 1);
                pixels = first;
                first.reset(); // The test keeps no payload owner across either scroll.
                auto opens = counts->opens.load(), reads = counts->reads.load(), closes = counts->closes.load();
                auto unchanged = [&]
                {
                    auto state = Access::Observe(view);
                    auto retained = Access::Pixels(view);
                    auto original = pixels.lock();
                    Require(Ready(view, 1, 1) && retained && original && retained == original &&
                        !retained.owner_before(original) && !original.owner_before(retained));
                    Require(state.text == before.text && state.epoch.source_generation == before.epoch.source_generation &&
                        state.row.published_request == before.row.published_request && !state.row.pending_request &&
                        !state.usage.active_requests && counts->opens == opens && counts->reads == reads && counts->closes == closes &&
                        !counts->badClose && !counts->wrongRequest);
                };
#if !defined(NOTEPADS_MARKDOWN_DISABLE_VIEWPORT_RASTER_BAND)
                stage = L"retained raster small-scroll seed";
                auto scroll = winrt::get_self<winrt::Notepads::Markdown::implementation::MarkdownView>(view)->ReadingScrollViewer();
                Require(scroll && scroll.ScrollableHeight() > 40);
                auto moveRaster = [&](double y)
                {
                    Require(view.TrySetVerticalScrollProgress(y / scroll.ScrollableHeight()));
                };
                moveRaster(10);
                co_await WaitUi(dispatcher, context, deadline, [&]
                {
                    auto value = Access::ObserveRasterBand(view);
                    return value.current && value.cropMatches && std::abs(value.viewportY - 10) < 0.75 / value.scale;
                });
                auto const rasterSeed = Access::ObserveRasterBand(view);
                Require(rasterSeed.surface && rasterSeed.pixelHeight > rasterSeed.viewportPixelHeight &&
                        rasterSeed.bytes <= 16ull * 1024 * 1024);
                for (double y : {20.0, 10.0})
                {
                    stage = L"retained raster same-window forward/reverse crop";
                    moveRaster(y);
                    co_await WaitUi(dispatcher, context, deadline, [&]
                    {
                        auto value = Access::ObserveRasterBand(view);
                        return value.current && value.cropMatches && std::abs(value.viewportY - y) < 0.75 / value.scale;
                    });
                    Require(Access::ObserveRasterBand(view).surface == rasterSeed.surface);
                    unchanged();
                }
                {
                    stage = L"retained raster public-DP overlay reentrancy";
                    auto target = Access::RasterTarget(view);
                    auto property = winrt::Windows::UI::Xaml::UIElement::RenderTransformProperty();
                    auto reentered = std::make_shared<bool>(false);
                    auto callback = target.RegisterPropertyChangedCallback(property,
                        [weak = winrt::make_weak(view), reentered](auto const &, auto const &)
                    {
                        if (auto owner = weak.get(); owner && !std::exchange(*reentered, true))
                            owner.ShowInactiveCaretMarker(true);
                    });
                    struct Unregister
                    {
                        winrt::Windows::UI::Xaml::Controls::Border target;
                        winrt::Windows::UI::Xaml::DependencyProperty property;
                        int64_t token{};
                        ~Unregister() { try { target.UnregisterPropertyChangedCallback(property, token); } catch (...) {} }
                    } unregister{target, property, callback};
                    moveRaster(15);
                    co_await WaitUi(dispatcher, context, deadline, [&]
                    {
                        auto value = Access::ObserveRasterBand(view);
                        return value.current && value.cropMatches && std::abs(value.viewportY - 15) < 0.75 / value.scale;
                    });
                    auto const repainted = Access::ObserveRasterBand(view);
                    Require(*reentered && view.ShowInactiveCaretMarker() && repainted.surface != rasterSeed.surface &&
                            repainted.pixelHeight == repainted.viewportPixelHeight);
                    view.ShowInactiveCaretMarker(false);
                    unchanged();
                }
                moveRaster(0);
                co_await WaitUi(dispatcher, context, deadline, [&]
                {
                    auto value = Access::ObserveRasterBand(view);
                    return value.current && value.cropMatches && value.viewportY == 0;
                });
#endif
                stage = L"same-position progress retires pending wheel input";
                Require(Access::ReplacesPendingWheelAtTop(view));
                unchanged();
                stage = L"far viewport retains Ready without provider Open or read";
                Require(view.TrySetVerticalScrollProgress(1));
                co_await WaitUi(dispatcher, context, deadline, [&]
                    { return view.VerticalScrollProgress() == 1 && Access::PumpImageAtCurrentViewport(view, true); });
                unchanged();
                stage = L"return viewport reuses the same Ready publication";
                Require(view.TrySetVerticalScrollProgress(0));
                co_await WaitUi(dispatcher, context, deadline, [&]
                    { return view.VerticalScrollProgress() == 0 && Access::PumpImageAtCurrentViewport(view, false); });
                unchanged();
            }
            catch (...)
            {
                auto status = winrt::to_hresult();
                auto reason = L"Offscreen image retention " + stage + L": " + std::wstring(winrt::to_message());
                if (diagnostics) diagnostics->Set(reason + DescribeDiagnostics(*counts, L"offscreen"));
                failure = std::make_exception_ptr(hresult_error{status, reason});
            }
            if (view) { FontAccess::DetachAndStopWatch(view); view.Cancel(); }
            if (inserted)
            {
                uint32_t index{}; if (parent.Children().IndexOf(host, index)) parent.Children().RemoveAt(index);
            }
            host.Children().Clear(); view = nullptr; host = nullptr; parent = nullptr;
            if (failure) std::rethrow_exception(failure);
            co_await WaitUi(dispatcher, context, deadline, [&]
                { auto p = PhysicalCapacity(); return pixels.expired() && !p.encoded_bytes && !p.decoded_bytes; });
        }
        IAsyncAction GlobalPressureAsync(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                         std::shared_ptr<CoreValidation::Context> context,
                                         std::shared_ptr<FailureDiagnostics> diagnostics, bool hiddenPeer = false)
        {
            using namespace winrt::Windows::UI::Xaml::Controls;
            constexpr uint64_t peerBytes = 7300854, fifthBytes = 6291510;
            constexpr uint64_t paddingBlockBytes = 16ull * 1024 * 1024;
            constexpr uint64_t paddingBytes = 6 * paddingBlockBytes;
            static_assert(paddingBytes + 32ull * 1024 * 1024 == ImageProcessMaximumEncodedBytes);
            static_assert(paddingBytes + 4 * peerBytes + fifthBytes > ImageProcessMaximumEncodedBytes);
            static_assert(peerBytes + 8192 < ImageOffscreenWarmTargetBytes);
            auto deadline = GetTickCount64() + 60000;
            auto parent = winrt::Windows::UI::Xaml::Window::Current().Content().as<Panel>();
            Grid host;
            std::array<View, 5> views{nullptr, nullptr, nullptr, nullptr, nullptr};
            std::array<std::shared_ptr<Counts>, 5> counts;
            std::array<ImageReuseControlObservation, 4> before;
            std::array<std::shared_ptr<ImagePixels const>, 4> pixels;
            std::array<NativeEncodedImageHandle, 4> held;
            std::array<std::shared_ptr<NativeImagePolicy>, 3> paddingPolicies;
            std::array<NativeEncodedImageHandle, 6> padding;
            std::exception_ptr failure;
            std::wstring stage = L"setup";
            bool inserted{};
            try
            {
                // A bounded predicate waits for actual prior last-owner free,
                // rather than treating a previous test's logical cleanup as it.
                co_await WaitUi(dispatcher, context, deadline, []
                    { auto p = PhysicalCapacity(); return !p.encoded_bytes && !p.decoded_bytes; });
                host.Width(800); host.Height(240); Grid::SetRow(host, 2);
                for (size_t index = 0; index < views.size(); ++index)
                {
                    ColumnDefinition column; column.Width({160, winrt::Windows::UI::Xaml::GridUnitType::Pixel});
                    host.ColumnDefinitions().Append(column);
                    auto &view = views[index];
                    view = winrt::make<winrt::Notepads::Markdown::implementation::MarkdownView>();
                    view.Width(160); view.Height(240); view.CacheDocumentOnUnload(hiddenPeer && index == 0);
                    Grid::SetColumn(view, static_cast<int>(index)); host.Children().Append(view);
                    counts[index] = std::make_shared<Counts>();
                    GUID document{}; check_hresult(CoCreateGuid(&document));
                    std::memcpy(&counts[index]->document, &document, sizeof(document));
                    counts[index]->width = index < 4 ? 1560 : 2048;
                    counts[index]->height = index < 4 ? 1560 : 1024;
                    counts[index]->view = winrt::make_weak(view);
                    view.SetImageProvider(winrt::make<Provider>(counts[index]), 1);
                }
                parent.Children().Append(host); inserted = true;
                co_await WaitUi(dispatcher, context, deadline, [&]
                {
                    return std::all_of(views.begin(), views.end(), [](auto const &view)
                        { return Access::Observe(view).loaded && view.ActualWidth() > 50; });
                });
                stage = L"initial viewports";
                for (auto const &view : views) Access::PrepareInitialViewport(view);
                host.UpdateLayout();
                co_await WaitUi(dispatcher, context, deadline, [&]
                {
                    host.UpdateLayout();
                    return std::all_of(views.begin(), views.end(), [](auto const &view)
                        { return Access::InitialViewportReady(view); });
                });
                constexpr std::string_view source = "![same](reuse-control:bmp)";
                auto submit = [&](size_t index)
                {
                    return views[index].SetDocumentAsync(counts[index]->document, 1,
                        {reinterpret_cast<uint8_t const *>(source.data()), static_cast<uint32_t>(source.size())});
                };
                for (size_t index = 0; index < 4; ++index)
                {
                    stage = L"fill peer " + std::to_wstring(index);
                    auto update = submit(index); co_await update; update = nullptr;
                    co_await WaitUi(dispatcher, context, deadline, [&]
                    {
                        auto state = Access::Observe(views[index]);
                        return Ready(views[index], 1, 1) && state.cacheCount == 1 &&
                            !state.usage.active_requests && counts[index]->closes == 1;
                    });
                    before[index] = Access::Observe(views[index]);
                    pixels[index] = Access::Pixels(views[index]);
                    held[index] = Access::HoldEncoded(views[index]);
                    Require(held[index] && held[index]->Capacity() == peerBytes &&
                        before[index].cacheEncoded == peerBytes && counts[index]->opens == 1);
                    if (index == 0)
                    {
                        // Real immutable-capacity owners fill the rest of the
                        // process budget without additional heavy UI controls.
                        auto memory = FontAccess::SourceMemory(views[0]).lock();
                        Require(static_cast<bool>(memory));
                        for (size_t slot = 0; slot < padding.size(); ++slot)
                        {
                            auto &policy = paddingPolicies[slot / 2];
                            if (!policy)
                            {
                                policy = NativeImagePolicy::Create();
                                policy->Begin(before[0].epoch);
                            }
                            auto object = static_cast<uint32_t>(slot % 2);
                            policy->Desire(object, {1, 1, 512, 1});
                            auto request = policy->Start(object, NOTEPADS_MARKDOWN_IMAGE_AUTO);
                            padding[slot] = NativeEncodedImage::Create(memory,
                                request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, paddingBlockBytes), paddingBlockBytes);
                            constexpr std::array<uint8_t, 4096> page{};
                            for (uint64_t filled = 0; filled < paddingBlockBytes; filled += page.size())
                                padding[slot]->Append(page);
                            padding[slot]->Freeze();
                            Require(padding[slot]->Bytes().size() == paddingBlockBytes &&
                                ReadingMemory::InspectAllocation(padding[slot].Get()).requestedTotal >= paddingBlockBytes);
                        }
                    }
                }
                if (hiddenPeer)
                {
                    views[0].Visibility(winrt::Windows::UI::Xaml::Visibility::Collapsed);
                    Require(!views[0].IsCachedPresentationSuspended() && counts[0]->opens == 1);
                }
                auto charged = PhysicalCapacity();
                Require(charged.encoded_bytes == paddingBytes + 4 * peerBytes);
                stage = L"denial broadcasts optional reclaim while physical owners are held";
                auto update = submit(4); co_await update; update = nullptr;
                co_await WaitUi(dispatcher, context, deadline, [&]
                {
                    auto fifth = Access::Observe(views[4]);
                    if (!fifth.found || fifth.row.state != NOTEPADS_MARKDOWN_IMAGE_LIMITED ||
                        !(fifth.row.status_flags & NOTEPADS_MARKDOWN_IMAGE_CAPACITY_WAIT) ||
                        fifth.usage.active_requests || counts[4]->closes != 1) return false;
                    for (size_t index = 0; index < 4; ++index)
                    {
                        auto peer = Access::Observe(views[index]);
                        if (peer.cacheScope == before[index].cacheScope || peer.cacheCount) return false;
                    }
                    if (hiddenPeer && (!views[0].IsCachedPresentationSuspended() || !views[0].HasCachedDocument()))
                        return false;
                    return true;
                });
                auto cleared = PhysicalCapacity();
                Require(cleared.encoded_bytes == charged.encoded_bytes && cleared.generation == charged.generation);
                Require(!Access::Pixels(views[4]) && counts[4]->opens == 1 && counts[4]->reads == 0);
                auto unchangedPeers = [&]
                {
                    if (hiddenPeer)
                        Require(views[0].IsCachedPresentationSuspended() && views[0].HasCachedDocument() &&
                            !Access::Pixels(views[0]) && counts[0]->opens == 1 && counts[0]->closes == 1 &&
                            !counts[0]->wrongRequest && !counts[0]->badClose);
                    for (size_t index = hiddenPeer ? 1 : 0; index < 4; ++index)
                    {
                        auto peer = Access::Observe(views[index]);
                        auto current = Access::Pixels(views[index]);
                        Require(Ready(views[index], 1, 1) && current == pixels[index] &&
                            !current.owner_before(pixels[index]) && !pixels[index].owner_before(current));
                        Require(peer.text == before[index].text &&
                            peer.epoch.source_generation == before[index].epoch.source_generation &&
                            peer.row.object == before[index].row.object &&
                            peer.row.published_request == before[index].row.published_request && !peer.row.pending_request &&
                            peer.cacheScope != before[index].cacheScope && !peer.cacheCount && !peer.rowEncoded);
                        Require(counts[index]->opens == 1 && counts[index]->closes == 1 &&
                            !counts[index]->wrongRequest && !counts[index]->badClose);
                    }
                };
                unchangedPeers();
                stage = L"actual last encoded owner returns capacity and fifth retries fresh";
                held[0] = {};
                auto returned = PhysicalCapacity();
                Require(returned.encoded_bytes == paddingBytes + 3 * peerBytes && returned.generation > cleared.generation);
                co_await WaitUi(dispatcher, context, deadline, [&]
                {
                    return Ready(views[4], 1, 1) && !Access::Observe(views[4]).usage.active_requests &&
                        counts[4]->closes == 2;
                });
                Require(PhysicalCapacity().encoded_bytes == paddingBytes + 3 * peerBytes + fifthBytes);
                Require(counts[4]->opens == 2 && counts[4]->returned == 2 && counts[4]->reads >= 2 &&
                    !counts[4]->badClose && !counts[4]->wrongRequest && Access::Observe(views[4]).cacheCount == 1);
                unchangedPeers();
                if (hiddenPeer)
                {
                    stage = L"hidden cached model resumes after actual pressure capacity return";
                    padding[0].Reset();
                    Require(PhysicalCapacity().encoded_bytes == paddingBytes - paddingBlockBytes + 3 * peerBytes + fifthBytes);
                    views[0].Visibility(winrt::Windows::UI::Xaml::Visibility::Visible);
                    update = views[0].ResumeCachedDocumentAsync(counts[0]->document, 1);
                    co_await update; update = nullptr;
                    co_await WaitUi(dispatcher, context, deadline, [&]
                    {
                        return Ready(views[0], 1, 1) && !Access::Observe(views[0]).usage.active_requests && counts[0]->closes == 2;
                    });
                    Require(!views[0].IsCachedPresentationSuspended() && counts[0]->opens == 2 && counts[0]->returned == 2 &&
                        !counts[0]->badClose && !counts[0]->wrongRequest);
                    Require(PhysicalCapacity().encoded_bytes == paddingBytes - paddingBlockBytes + 4 * peerBytes + fifthBytes);
                }
            }
            catch (...)
            {
                auto status = winrt::to_hresult();
                auto reason = L"Global encoded pressure " + stage + L": " + std::wstring(winrt::to_message());
                std::wstring details = reason;
                for (size_t index = 0; index < counts.size(); ++index)
                    if (counts[index]) details += DescribeDiagnostics(*counts[index], (L"control" + std::to_wstring(index)).c_str());
                if (diagnostics) diagnostics->Set(std::move(details));
                failure = std::make_exception_ptr(hresult_error{status, reason});
            }
            for (auto const &view : views)
                if (view) { FontAccess::DetachAndStopWatch(view); view.Cancel(); }
            if (inserted)
            {
                uint32_t index{}; if (parent.Children().IndexOf(host, index)) parent.Children().RemoveAt(index);
            }
            host.Children().Clear();
            for (auto &view : views) view = nullptr;
            for (auto &owner : held) owner = {};
            for (auto &value : pixels) value.reset();
            for (auto &owner : padding) owner.Reset();
            for (auto &policy : paddingPolicies) policy.reset();
            host = nullptr; parent = nullptr;
            if (failure) std::rethrow_exception(failure);
            co_await WaitUi(dispatcher, context, deadline, []
                { auto p = PhysicalCapacity(); return !p.encoded_bytes && !p.decoded_bytes; });
        }
    }
    winrt::Windows::Foundation::IAsyncAction RunAsync(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                                     std::shared_ptr<CoreValidation::Context> context,
                                                     std::shared_ptr<FailureDiagnostics> diagnostics)
    {
        co_await winrt::resume_foreground(dispatcher);
        auto deadline = GetTickCount64() + 45000;
        winrt::Windows::UI::Xaml::Controls::Panel parent{nullptr};
        winrt::Windows::UI::Xaml::Controls::Grid host{nullptr};
        View view{nullptr};
        auto counts = std::make_shared<Counts>();
        auto replacement = std::make_shared<Counts>();
        auto hooks = std::make_shared<ImageFontControlTestHooks>();
        std::shared_ptr<ImagePixels const> first, changed, upgraded, animated;
        std::exception_ptr failure;
        std::wstring stage = L"setup";
        bool inserted{};
        try
        {
            GUID document{}; winrt::check_hresult(CoCreateGuid(&document));
            std::memcpy(&counts->document, &document, sizeof(document));
            replacement->document = counts->document; replacement->generation = 2; replacement->blue = 0x40;
            parent = winrt::Windows::UI::Xaml::Window::Current().Content().try_as<winrt::Windows::UI::Xaml::Controls::Panel>();
            Require(parent && counts->entered && counts->proceed);
            host = winrt::Windows::UI::Xaml::Controls::Grid{};
            host.Width(160); host.Height(240); winrt::Windows::UI::Xaml::Controls::Grid::SetRow(host, 2);
            view = winrt::make<winrt::Notepads::Markdown::implementation::MarkdownView>();
            view.Width(160); view.Height(240); view.CacheDocumentOnUnload(false);
            host.Children().Append(view); parent.Children().Append(host); inserted = true;
            counts->view = winrt::make_weak(view); replacement->view = winrt::make_weak(view);
            co_await WaitUi(dispatcher, context, deadline, [&] { return Access::Observe(view).loaded && view.ActualWidth() > 50; });
            FontAccess::Attach(view, hooks);
            stage = L"initial viewport";
            Access::PrepareInitialViewport(view);
            host.UpdateLayout();
            co_await WaitUi(dispatcher, context, deadline, [&]
            {
                host.UpdateLayout();
                return Access::InitialViewportReady(view);
            });
            view.SetImageProvider(winrt::make<Provider>(counts), 1);
            auto submit = [&](uint64_t revision, std::string_view source)
            {
                return view.SetDocumentAsync(counts->document, revision,
                    {reinterpret_cast<uint8_t const *>(source.data()), static_cast<uint32_t>(source.size())});
            };
            stage = L"hover over a pending image";
            counts->gateNext = true;
            auto update = submit(1, "![same](reuse-control:bmp \"Stationary image title\")");
            co_await WaitUi(dispatcher, context, deadline, [&]
            {
                return WaitForSingleObjectEx(counts->entered.get(), 0, FALSE) == WAIT_OBJECT_0 &&
                       Access::HoverPendingImage(view);
            });
            co_await WaitUi(dispatcher, context, deadline, [&]
                { return std::wstring_view{Access::HoverText(view)}.find(L"reuse-control:bmp") != std::wstring_view::npos; });

            stage = L"loaded image title without another pointer move";
            winrt::check_bool(SetEvent(counts->proceed.get()));
            co_await update; update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&] { return Ready(view, 1, 1) && counts->closes == counts->opens; });
            co_await WaitUi(dispatcher, context, GetTickCount64() + 3000, [&]
                { return Access::HoverText(view) == L"Stationary image title"; });
            stage = L"loaded image accessibility name and status details";
            Require(Access::ReadyImageAutomation(view));
            winrt::check_bool(ResetEvent(counts->entered.get()));
            winrt::check_bool(ResetEvent(counts->proceed.get()));
            first = Access::Pixels(view);
            auto before = Access::Observe(view);
            auto oldMemory = FontAccess::SourceMemory(view);
            Require(first->IsFrozen() && counts->opens == 1 && before.cacheCount == 1);
            stage = L"shifted prose delta retains current resource without IO";
            constexpr std::string_view prefix = "A preceding prose [link](https://example.invalid).\n\n";
            winrt::Notepads::Markdown::MarkdownSourceEdit edit{};
            edit.ReplacementLength = prefix.size();
            update = view.ApplyEditsAsync(counts->document, 1, 2, {&edit, 1},
                {reinterpret_cast<uint8_t const *>(prefix.data()), static_cast<uint32_t>(prefix.size())});
            co_await update; update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&] { return Ready(view, 2, 1) && oldMemory.expired() && counts->closes == 1; });
            auto second = Access::Pixels(view);
            auto after = Access::Observe(view);
            Require(second == first && !second.owner_before(first) && !first.owner_before(second));
            Require(counts->opens == 1 && counts->returned == 1 && counts->reads >= 2 && !counts->badClose);
            Require(after.row.published_request != before.row.published_request && after.epoch.source_generation != before.epoch.source_generation);
            Require(after.text != before.text && (after.textStart != before.textStart || after.row.object != before.row.object));
            second.reset();
            stage = L"shifted full synchronization retains current resource without IO";
            update = submit(3, "More prose.\n\n![same](reuse-control:bmp \"Stationary image title\")");
            co_await update; update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&] { return Ready(view, 3, 1); });
            Require(Access::Pixels(view) == first && counts->opens == 1 && counts->closes == 1);
            stage = L"explicit refresh reloads same URI changed bytes";
            counts->blue = 0x90;
            view.Refresh();
            co_await WaitUi(dispatcher, context, deadline, [&] { return Ready(view, 3, 1) && counts->closes == 2; });
            changed = Access::Pixels(view);
            Require(changed != first && changed->pixels[0] == 0x90 && counts->opens == 2);
            stage = L"larger physical bucket miss";
            auto oldWidth = changed->metadata.decoded_width;
            host.Width(960); view.Width(960);
            co_await WaitUi(dispatcher, context, deadline, [&]
            {
                auto pixels = Access::Pixels(view);
                return Ready(view, 3, 1) && pixels && pixels->metadata.decoded_width > oldWidth && counts->closes == counts->opens;
            });
            upgraded = Access::Pixels(view);
            Require(upgraded != changed && counts->opens == 3);
            stage = L"pressure revokes pre-Open scope";
            counts->gateNext = true;
            update = submit(4, "New current prose.\n\n![pressure](reuse-control:bmp)");
            co_await WaitUi(dispatcher, context, deadline, [&]
                { return WaitForSingleObjectEx(counts->entered.get(), 0, FALSE) == WAIT_OBJECT_0; });
            auto oldScope = Access::Observe(view).cacheScope;
            Access::PressureClear(view);
            Require(Access::Observe(view).cacheScope != oldScope && !Access::Observe(view).cacheCount);
            winrt::check_bool(SetEvent(counts->proceed.get()));
            co_await update; update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&] { return Ready(view, 4, 1) && counts->closes == 4; });
            Require(Access::Pixels(view) != upgraded && !Access::Observe(view).cacheCount);
            stage = L"synchronous provider replacement";
            auto weak = winrt::make_weak(view);
            counts->onOpen = [weak, replacement]
            {
                if (auto current = weak.get()) current.SetImageProvider(winrt::make<Provider>(replacement), 2);
            };
            update = submit(5, "Provider reentry prose.\n\n![provider](reuse-control:bmp)");
            co_await update; update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&]
                { return Ready(view, 5, 2) && counts->closes == counts->opens && replacement->closes == replacement->opens; });
            Require(Access::Pixels(view)->pixels[0] == 0x40 && Access::Observe(view).cacheCount == 1);
            Require(!counts->wrongRequest && !replacement->wrongRequest && !replacement->badClose);
            stage = L"animated resource retains owner and clock on source updates";
            replacement->animated = true;
            replacement->generation = 3;
            view.SetImageProvider(winrt::make<Provider>(replacement), 3);
            co_await WaitUi(dispatcher, context, deadline, [&]
                { return Ready(view, 5, 3) && replacement->closes == replacement->opens; });
            animated = Access::Pixels(view);
            auto animation = Access::Observe(view);
            auto gifOpens = replacement->opens.load();
            Require(animated && animated->IsAnimated() && animation.animationStartTicks);
            update = submit(6, "Shifted animation prose.\n\n![provider](reuse-control:bmp)");
            co_await update; update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&] { return Ready(view, 6, 3); });
            Require(Access::Pixels(view) == animated && replacement->opens == gifOpens &&
                Access::Observe(view).animationStartTicks == animation.animationStartTicks);
            edit = {};
            edit.ReplacementLength = prefix.size();
            update = view.ApplyEditsAsync(counts->document, 6, 7, {&edit, 1},
                {reinterpret_cast<uint8_t const *>(prefix.data()), static_cast<uint32_t>(prefix.size())});
            co_await update; update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&] { return Ready(view, 7, 3); });
            Require(Access::Pixels(view) == animated && replacement->opens == gifOpens &&
                Access::Observe(view).animationStartTicks == animation.animationStartTicks);
            stage = L"superseded bound source retains the original resource identity";
            hooks->readingPublication = std::make_shared<ReadingPublicationControlGate>();
            hooks->gateReading = true;
            auto superseded = submit(8, "First queued prose.\n\n![provider](reuse-control:bmp)");
            co_await WaitUi(dispatcher, context, deadline, [&]
                { return hooks->readingPublication->Reached(0); });
            Require(!FontAccess::Observe(view).readingDescriptors);
            hooks->gateReading = false;
            update = submit(9, "Newest queued prose.\n\n![provider](reuse-control:bmp)");
            hooks->readingPublication->ReleaseAll();
            co_await superseded; superseded = nullptr;
            co_await update; update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&] { return Ready(view, 9, 3); });
            Require(!hooks->readingPublication->timedOut && Access::Pixels(view) == animated &&
                replacement->opens == gifOpens && Access::Observe(view).animationStartTicks == animation.animationStartTicks);
            stage = L"removed resource is not resurrected by a later occurrence";
            update = submit(10, "No image in this current source.");
            co_await update; update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&]
                { auto state = Access::Observe(view); return state.current && !state.found; });
            update = submit(11, "![provider](reuse-control:bmp)");
            co_await update; update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&]
                { return Ready(view, 11, 3) && replacement->closes == replacement->opens; });
            Require(Access::Pixels(view) != animated && replacement->opens == gifOpens + 1);
            stage = L"scrolled lower duplicate retains its occurrence identity";
            std::string duplicates = "![provider](reuse-control:bmp)\n\n";
            for (uint32_t paragraph = 0; paragraph < 96; ++paragraph)
                duplicates += "A paragraph separating equal images.\n\n";
            duplicates += "![provider](reuse-control:bmp)";
            view.IsResourceRenderingDeferred(true);
            replacement->generation = 4;
            view.SetImageProvider(winrt::make<Provider>(replacement), 4);
            update = submit(12, duplicates);
            co_await update; update = nullptr;
            stage = L"scrolled duplicate waits for measured bottom viewport";
            co_await WaitUi(dispatcher, context, deadline, [&]
            {
                // A published Frame can precede ScrollViewer measurement of
                // its new extent. A progress request against the former short
                // document succeeds at offset zero; reconcile after measure.
                host.UpdateLayout();
                if (view.VerticalScrollProgress() != 1)
                    (void)view.TrySetVerticalScrollProgress(1);
                return Access::Observe(view).current && view.VerticalScrollProgress() == 1;
            });
            stage = L"scrolled duplicate loads only lower current occurrence";
            view.IsResourceRenderingDeferred(false);
            co_await WaitUi(dispatcher, context, deadline, [&]
            {
                auto lower = Access::Observe(view, 1);
                return lower.current && lower.found && lower.row.state == NOTEPADS_MARKDOWN_IMAGE_READY &&
                    !lower.usage.active_requests && replacement->closes == replacement->opens;
            });
            auto lower = Access::Pixels(view, 1);
            auto duplicateOpens = replacement->opens.load();
            Require(lower && !Access::Pixels(view));
            duplicates.insert(0, "Unrelated preceding prose.\n\n");
            update = submit(13, duplicates);
            co_await update; update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&]
            {
                auto state = Access::Observe(view, 1);
                return state.current && state.found && state.epoch.source.revision == 13 &&
                    state.row.state == NOTEPADS_MARKDOWN_IMAGE_READY;
            });
            Require(Access::Pixels(view, 1) == lower && !Access::Pixels(view) && replacement->opens == duplicateOpens);
            lower.reset();
            stage = L"optional reuse freeze failure still publishes the edited source";
            hooks->readingPublication = std::make_shared<ReadingPublicationControlGate>();
            hooks->gateReading = true;
            duplicates.insert(0, "Another preceding paragraph.\n\n");
            update = submit(14, duplicates);
            co_await WaitUi(dispatcher, context, deadline, [&]
                { return hooks->readingPublication->Reached(0); });
            hooks->failNextFreeze = true;
            hooks->gateReading = false;
            hooks->readingPublication->ReleaseAll();
            co_await WaitUi(dispatcher, context, deadline, [&]
                { return update.Status() != winrt::Windows::Foundation::AsyncStatus::Started; });
            Require(update.GetResults().Status() == winrt::Notepads::Markdown::DocumentUpdateStatus::Ready &&
                !hooks->failNextFreeze && !hooks->readingPublication->timedOut);
            update = nullptr;
            co_await WaitUi(dispatcher, context, deadline, [&]
            {
                auto state = Access::Observe(view, 1);
                return state.current && state.found && state.epoch.source.revision == 14 &&
                    state.row.state == NOTEPADS_MARKDOWN_IMAGE_READY;
            });
            stage = L"optional reclaim discards pixels held for an unpresented edit";
            lower = Access::Pixels(view, 1);
            auto opensBeforeReclaim = replacement->opens.load();
            hooks->readingPublication = std::make_shared<ReadingPublicationControlGate>();
            hooks->gateReading = true;
            duplicates.insert(0, "A further preceding paragraph.\n\n");
            update = submit(15, duplicates);
            co_await WaitUi(dispatcher, context, deadline, [&]
                { return hooks->readingPublication->Reached(0); });
            auto scopeBeforeReclaim = Access::Observe(view).cacheScope;
            RequestOptionalImageReclaim();
            co_await WaitUi(dispatcher, context, deadline, [&]
                { return Access::Observe(view).cacheScope != scopeBeforeReclaim; });
            hooks->gateReading = false;
            hooks->readingPublication->ReleaseAll();
            co_await WaitUi(dispatcher, context, deadline, [&]
            {
                auto state = Access::Observe(view, 1);
                return state.current && state.found && state.epoch.source.revision == 15 &&
                    state.row.state == NOTEPADS_MARKDOWN_IMAGE_READY && replacement->closes == replacement->opens;
            });
            Require(update.GetResults().Status() == winrt::Notepads::Markdown::DocumentUpdateStatus::Ready &&
                !hooks->readingPublication->timedOut && Access::Pixels(view, 1) != lower &&
                replacement->opens == opensBeforeReclaim + 1);
            update = nullptr;
            lower.reset();
            stage = L"hidden model last owner and old-root metadata estimate";
            auto source = FontAccess::SourceMemory(view);
            auto lifetime = FontAccess::Lifetime(view);
            view.SuspendCachedDocument(); view.DiscardCachedDocument();
            co_await WaitUi(dispatcher, context, deadline, [&]
                { return source.expired() && lifetime.Snapshot().modelPins == 0 && !Access::Observe(view).usage.active_requests; });
            auto hidden = Access::Observe(view);
            Require(!hidden.loaded && !hidden.cacheCount && hidden.cacheMetadata && hidden.usage.decoded_bytes);
            Require(view.GetRetainedByteEstimate() >= 8192 && !lifetime.Snapshot().cleanupStatus && !lifetime.Snapshot().cleanupHresult);
        }
        catch (...)
        {
            auto status = winrt::to_hresult();
            auto reason = stage + L": " + std::wstring(winrt::to_message());
            auto details = reason +
                L"; old/new opens/closes=" + std::to_wstring(counts->opens.load()) + L"/" + std::to_wstring(counts->closes.load()) +
                L"/" + std::to_wstring(replacement->opens.load()) + L"/" + std::to_wstring(replacement->closes.load()) +
                DescribeDiagnostics(*counts, L"old") + DescribeDiagnostics(*replacement, L"replacement");
            if (diagnostics) diagnostics->Set(std::move(details));
            failure = std::make_exception_ptr(winrt::hresult_error{status, reason});
        }
        SetEvent(counts->proceed.get());
        if (hooks->readingPublication) hooks->readingPublication->ReleaseAll();
        if (view) { FontAccess::DetachAndStopWatch(view); view.Cancel(); }
        if (inserted)
        {
            uint32_t index{}; if (parent.Children().IndexOf(host, index)) parent.Children().RemoveAt(index);
        }
        if (host) host.Children().Clear();
        view = nullptr; host = nullptr; parent = nullptr;
        first.reset(); changed.reset(); upgraded.reset(); animated.reset();
        if (failure) std::rethrow_exception(failure);
        co_await OffscreenReadyAsync(dispatcher, context, diagnostics);
        co_await GlobalPressureAsync(dispatcher, context, diagnostics);
        co_await GlobalPressureAsync(dispatcher, context, diagnostics, true);
    }
}
