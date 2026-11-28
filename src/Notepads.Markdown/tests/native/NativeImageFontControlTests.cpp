// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information (MIT).
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeImageFontControlTests.h"
#include "Control/MarkdownView.h"
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.Storage.h>
#include <winrt/Windows.Storage.Streams.h>
#include <algorithm>
#include <chrono>
#include <exception>
#include <functional>
#include <sstream>
#include <string_view>

namespace Notepads::Markdown::Native
{
    ReadingPublicationControlGate::ReadingPublicationControlGate()
    {
        for (uint32_t stage = 0; stage < reached.size(); ++stage)
        {
            reached[stage].attach(CreateEventW(nullptr, TRUE, FALSE, nullptr));
            proceed[stage].attach(CreateEventW(nullptr, TRUE, FALSE, nullptr));
            if (!reached[stage] || !proceed[stage])
                winrt::throw_last_error();
        }
    }

    ReadingPublicationControlGate::~ReadingPublicationControlGate()
    {
        ReleaseAll();
    }

    void ReadingPublicationControlGate::Pause(uint32_t stage)
    {
        SetEvent(reached[stage].get());
        if (WaitForSingleObjectEx(proceed[stage].get(), 10000, FALSE) != WAIT_OBJECT_0)
        {
            timedOut.store(true, std::memory_order_release);
            throw winrt::hresult_error{HRESULT_FROM_WIN32(ERROR_TIMEOUT)};
        }
    }

    bool ReadingPublicationControlGate::Reached(uint32_t stage) const noexcept
    {
        return WaitForSingleObjectEx(reached[stage].get(), 0, FALSE) == WAIT_OBJECT_0;
    }

    void ReadingPublicationControlGate::Release(uint32_t stage) noexcept
    {
        SetEvent(proceed[stage].get());
    }

    void ReadingPublicationControlGate::ReleaseAll() noexcept
    {
        for (uint32_t stage = 0; stage < proceed.size(); ++stage)
            Release(stage);
    }

    ImageFontControlTestHooks::~ImageFontControlTestHooks()
    {
        ReleaseGate();
    }

    void *ImageFontControlTestHooks::FailFreeze(void *context, ReadingMemory const &, size_t)
    {
        auto &tests = *static_cast<ImageFontControlTestHooks *>(context);
        ++tests.freezeFaultHits;
        tests.failedFreezeCall = tests.freezeCalls;
        throw ReadingFailure{NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY};
    }

    ReadingAllocationFactory ImageFontControlTestHooks::BeforeFreeze() noexcept
    {
        ++freezeCalls;
        return std::exchange(failNextFreeze, false) ? ReadingAllocationFactory{this, FailFreeze}
                                                   : ReadingAllocationFactory{};
    }

    void ImageFontControlTestHooks::InitializeTask(std::shared_ptr<ImageLoadTask> const &task)
    {
        if (!gateNextTask || !task->request)
            return;
        auto kind = task->request->Kind();
        if (kind != NOTEPADS_MARKDOWN_IMAGE_SVG && !(gateColdAutoTask && kind == NOTEPADS_MARKDOWN_IMAGE_AUTO))
            return;
        winrt::handle reached{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
        if (!reached)
            winrt::throw_last_error();
        winrt::handle proceed{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
        if (!proceed)
            winrt::throw_last_error();
        std::lock_guard lock(_taskGate);
        if (_task || task->testAfterCommitReached || task->testAfterCommitProceed)
            throw winrt::hresult_illegal_method_call();
        task->testAfterCommitReached = std::move(reached);
        task->testAfterCommitProceed = std::move(proceed);
        _task = task; // Task owns both handles through every actual worker use.
        gateNextTask = false;
    }

    std::shared_ptr<ImageLoadTask> ImageFontControlTestHooks::HeldTask() const
    {
        std::lock_guard lock(_taskGate);
        return _task;
    }

    void ImageFontControlTestHooks::ReleaseGate() noexcept
    {
        if (readingPublication)
            readingPublication->ReleaseAll();
        auto task = HeldTask();
        if (task && task->testAfterCommitProceed)
            SetEvent(task->testAfterCommitProceed.get());
    }

    void ImageFontControlTestHooks::ForgetTask() noexcept
    {
        std::lock_guard lock(_taskGate);
        _task.reset();
    }

    std::wstring ImageFontControlObservation::Describe() const
    {
        winrt::guid document{};
        static_assert(sizeof(document) == sizeof(epoch.source.document));
        std::memcpy(&document, epoch.source.document, sizeof(document));
        std::wostringstream text;
        text << L"doc=" << winrt::to_hstring(document).c_str() << L"/" << epoch.source.revision
             << L";text=" << textGeneration << L";epoch=" << epoch.source_generation << L"/"
             << epoch.provider_generation << L";font=" << fontGeneration << L";loaded=" << loadedGeneration
             << L";reading/main/descriptors/math/diagram=" << readingMain << L"/" << readingDescriptors << L"/"
             << mathBound << L"/" << diagramBound
             << L";watch=" << watchVersion << L";idle/baseline=" << watchIdle << L"/" << watchBaseline
             << L";policy/table/frameGen=" << policyGeneration << L"/" << tableGeneration << L"/" << frameGeneration
             << L";table/frame/frameTable/published=" << table << L"/" << frame << L"/" << frameTable << L"/"
             << publishedTable << L";layout/barrier=" << layoutGeneration << L"/" << barrier
             << L";svg=" << svgFound << L":" << svg.object << L":" << svg.state << L";request="
             << svg.pending_request << L"/" << svg.published_request << L";desired="
             << svg.desired_target.font_generation << L"/" << svg.desired_target.target_generation << L"/"
             << svg.desired_target.available_width_dip << L"/" << svg.desired_target.effective_physical_scale
             << L";published=" << svg.published_target.font_generation << L"/" << svg.published_target.target_generation
             << L";frameSvg=" << frameSvgFound << L":" << frameSvg.state << L":" << frameSvg.published_request
             << L":" << frameSvg.published_target.font_generation << L";encoded/raster=" << encoded << L"/"
             << rasterPixels << L";current/action/surface=" << currentImage << L"/" << currentAction << L"/" << surface
             << L";width/viewport/measured=" << actualWidth << L"/" << viewportWidth << L"/" << measuredWidth
             << L";error=" << error.c_str();
        return text.str();
    }
}

namespace Notepads::Markdown::Native::ImageFontControlTests
{
    namespace
    {
        using namespace winrt;
        using namespace winrt::Windows::Foundation;
        using namespace winrt::Windows::Storage::Streams;
        using namespace winrt::Windows::UI::Xaml;
        using namespace winrt::Windows::UI::Xaml::Controls;
        using Access = winrt::Notepads::Markdown::implementation::ImageFontControlTestAccess;
        using View = winrt::Notepads::Markdown::MarkdownView;
        constexpr wchar_t CheckName[] = L"image_font_live_view_refresh_recovery";

        void Require(bool condition, std::wstring const &detail)
        {
            if (!condition)
                throw hresult_error{NativeStatusHresult(NOTEPADS_MARKDOWN_CORE_INVALID_STATE), hstring{detail}};
        }

        bool SameEpoch(notepads_markdown_image_epoch const &left, notepads_markdown_image_epoch const &right) noexcept
        {
            return left.source.revision == right.source.revision &&
                   std::memcmp(left.source.document, right.source.document, sizeof(left.source.document)) == 0 &&
                   left.source_generation == right.source_generation &&
                   left.provider_generation == right.provider_generation;
        }

        bool Ready(ImageFontControlObservation const &value) noexcept
        {
            return value.enabled && value.loaded && value.source && value.watchBaseline && value.surface &&
                   !value.refreshScheduled && value.currentAction && value.currentImage && value.error.empty() &&
                   value.table && value.frame && value.table == value.frameTable && value.frameTable == value.publishedTable &&
                   value.tableTextGeneration == value.textGeneration && SameEpoch(value.epoch, value.tableEpoch) &&
                   value.svgFound && value.rasterFound && value.frameSvgFound && value.frameRasterFound &&
                   value.svg.state == NOTEPADS_MARKDOWN_IMAGE_READY && value.svg.metadata.kind == NOTEPADS_MARKDOWN_IMAGE_SVG &&
                   value.svg.published_target.font_generation == value.fontGeneration &&
                   value.frameSvg.state == NOTEPADS_MARKDOWN_IMAGE_READY &&
                   value.frameSvg.published_target.font_generation == value.fontGeneration &&
                   value.frameSvg.published_request == value.svg.published_request &&
                   value.raster.state == NOTEPADS_MARKDOWN_IMAGE_READY && value.raster.metadata.kind == NOTEPADS_MARKDOWN_IMAGE_RASTER &&
                   value.frameRaster.state == NOTEPADS_MARKDOWN_IMAGE_READY && value.encoded && value.rasterPixels;
        }

        void SameSource(ImageFontControlObservation const &value, ImageFontControlObservation const &baseline)
        {
            Require(value.textGeneration == baseline.textGeneration && SameEpoch(value.epoch, baseline.epoch),
                    L"Font refresh changed document/revision/text/source/provider identity: " + value.Describe());
            Require(value.encoded == baseline.encoded && value.rasterPixels == baseline.rasterPixels &&
                        value.raster.published_request == baseline.raster.published_request,
                    L"Font refresh replaced retained encoded/raster identity: " + value.Describe());
        }

        struct StreamCount
        {
            std::atomic<uint32_t> closes{};
            uint64_t image{}, request{}, text{};
        };

        struct ProviderCounts
        {
            std::atomic<uint32_t> calls{}, returned{}, wrongRequests{};
            mutable std::mutex gate;
            std::vector<std::shared_ptr<StreamCount>> streams;

            uint32_t Closes() const noexcept
            {
                std::lock_guard lock(gate);
                uint32_t count{};
                for (auto const &stream : streams)
                    count += stream->closes.load(std::memory_order_acquire);
                return count;
            }
            bool ClosedOnce() const noexcept
            {
                std::lock_guard lock(gate);
                return streams.size() == returned && std::all_of(streams.begin(), streams.end(),
                    [](auto const &stream) { return stream->closes.load(std::memory_order_acquire) == 1; });
            }
            std::wstring Describe() const
            {
                std::lock_guard lock(gate);
                uint32_t closes{};
                bool once = streams.size() == returned.load(std::memory_order_acquire);
                for (auto const &stream : streams)
                {
                    auto count = stream->closes.load(std::memory_order_acquire);
                    closes += count;
                    once = once && count == 1;
                }
                std::wstring text = L"Open/returned/Close=" + std::to_wstring(calls.load(std::memory_order_acquire)) + L"/" +
                                    std::to_wstring(returned.load(std::memory_order_acquire)) + L"/" +
                                    std::to_wstring(closes) + L";eachClose1=" + std::to_wstring(once) +
                                    L";wrongRequests=" + std::to_wstring(wrongRequests.load(std::memory_order_acquire));
                for (auto const &stream : streams)
                    text += L";stream=" + std::to_wstring(stream->image) + L"/" + std::to_wstring(stream->request) +
                            L"/" + std::to_wstring(stream->text) + L":Close" +
                            std::to_wstring(stream->closes.load(std::memory_order_acquire));
                return text;
            }
        };

        struct CountingStream : implements<CountingStream, IRandomAccessStream, IInputStream, IOutputStream, IClosable>
        {
            CountingStream(IRandomAccessStream stream, std::shared_ptr<StreamCount> count)
                : _stream(std::move(stream)), _count(std::move(count))
            {
            }
            bool CanRead() const
            {
                return _stream.CanRead();
            }
            bool CanWrite() const
            {
                return _stream.CanWrite();
            }
            uint64_t Position() const
            {
                return _stream.Position();
            }
            uint64_t Size() const
            {
                return _stream.Size();
            }
            void Size(uint64_t value) const
            {
                _stream.Size(value);
            }
            void Seek(uint64_t value) const
            {
                _stream.Seek(value);
            }
            IInputStream GetInputStreamAt(uint64_t value) const
            {
                return _stream.GetInputStreamAt(value);
            }
            IOutputStream GetOutputStreamAt(uint64_t value) const
            {
                return _stream.GetOutputStreamAt(value);
            }
            IRandomAccessStream CloneStream() const
            {
                return _stream.CloneStream();
            }
            IAsyncOperationWithProgress<IBuffer, uint32_t> ReadAsync(IBuffer const &buffer, uint32_t count,
                                                                  InputStreamOptions options) const
            {
                return _stream.ReadAsync(buffer, count, options);
            }
            IAsyncOperationWithProgress<uint32_t, uint32_t> WriteAsync(IBuffer const &buffer) const
            {
                return _stream.WriteAsync(buffer);
            }
            IAsyncOperation<bool> FlushAsync() const
            {
                return _stream.FlushAsync();
            }
            void Close()
            {
                // Count every external Close; only physical inner close is idempotent.
                if (_count->closes.fetch_add(1, std::memory_order_acq_rel) == 0)
                    _stream.Close();
            }

          private:
            IRandomAccessStream _stream{nullptr};
            std::shared_ptr<StreamCount> _count;
        };

        std::vector<uint8_t> FixedBmp()
        {
            // The original independent 2x2 BI_RGB fixture from image tests.
            return {'B', 'M', 70, 0, 0,  0, 0,   0, 0, 0,   54,  0,   0, 0, 40, 0, 0,   0, 2,   0, 0, 0, 2, 0,
                    0,   0,   1,  0, 24, 0, 0,   0, 0, 0,   16,  0,   0, 0, 0,  0, 0,   0, 0,   0, 0, 0, 0, 0,
                    0,   0,   0,  0, 0,  0, 255, 0, 0, 255, 255, 255, 0, 0, 0,  0, 255, 0, 255, 0, 0, 0};
        }

        std::vector<uint8_t> FixedSvg()
        {
            // Original app-source-v4/Fixtures/text.svg, not renderer output.
            constexpr std::u8string_view text =
                u8"<!-- Copyright (c) 2026, Jiaqi (0x7c13) Liu. See LICENSE in the project root. -->\r\n"
                u8"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"300\" height=\"58\" viewBox=\"0 0 300 58\">"
                u8"<rect width=\"300\" height=\"58\" fill=\"#eef3fa\"/><text x=\"12\" y=\"36\" font-family=\"Segoe UI\" "
                u8"font-size=\"24\" fill=\"#263b55\">\u4e2d\u6587 English</text></svg>\r\n";
            auto first = reinterpret_cast<uint8_t const *>(text.data());
            return {first, first + text.size()};
        }

        struct FixtureProvider : implements<FixtureProvider, winrt::Notepads::Markdown::IMarkdownImageProvider>
        {
            FixtureProvider(guid document, std::shared_ptr<ProviderCounts> counts)
                : _document(document), _counts(std::move(counts)), _thread(GetCurrentThreadId())
            {
            }
            IAsyncOperation<IRandomAccessStream> OpenAsync(winrt::Notepads::Markdown::MarkdownImageRequest request)
            {
                auto lifetime = get_strong();
                ++_counts->calls;
                if (GetCurrentThreadId() != _thread || request.DocumentId() != _document || request.Revision() != 1 ||
                    request.ProviderGeneration() != 1 || !request.TextGeneration() || !request.RequestId())
                    ++_counts->wrongRequests;
                auto destination = request.Destination();
                Require(destination == L"font-control:text" || destination == L"font-control:bmp",
                        L"Unexpected live font fixture destination.");
                auto bytes = destination == L"font-control:text" ? FixedSvg() : FixedBmp();
                auto count = std::make_shared<StreamCount>();
                count->image = request.ImageId();
                count->request = request.RequestId();
                count->text = request.TextGeneration();
                InMemoryRandomAccessStream stream;
                DataWriter writer{stream};
                writer.WriteBytes(bytes);
                co_await writer.StoreAsync();
                (void)writer.DetachStream();
                writer.Close();
                stream.Seek(0);
                {
                    std::lock_guard lock(_counts->gate);
                    _counts->streams.push_back(count);
                    ++_counts->returned;
                }
                co_return make<CountingStream>(stream, std::move(count));
            }

          private:
            guid _document;
            std::shared_ptr<ProviderCounts> _counts;
            uint32_t _thread;
        };

        IAsyncAction WaitUntil(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                              std::shared_ptr<CoreValidation::Context> context, uint64_t overallDeadline,
                              uint32_t maximumMilliseconds, std::function<bool()> predicate, std::wstring operation)
        {
            auto deadline = std::min(overallDeadline, GetTickCount64() + maximumMilliseconds);
            for (;;)
            {
                context->Arbitrate();
                if (predicate())
                    co_return;
                if (GetTickCount64() >= deadline)
                    throw hresult_error{HRESULT_FROM_WIN32(ERROR_TIMEOUT), hstring{L"Timed out: " + operation}};
                co_await winrt::resume_after(std::chrono::milliseconds(10));
                co_await winrt::resume_foreground(dispatcher);
            }
        }

        std::wstring ExceptionText(std::exception_ptr const &failure, int32_t &status)
        {
            if (!failure)
                return {};
            try
            {
                std::rethrow_exception(failure);
            }
            catch (NativeFailure const &error)
            {
                status = error.status;
                return L"Native status=" + std::to_wstring(error.status);
            }
            catch (hresult_error const &error)
            {
                status = error.code().value;
                return std::wstring{error.message()};
            }
            catch (std::exception const &error)
            {
                status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                return std::wstring{to_hstring(error.what())};
            }
            catch (...)
            {
                status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                return L"Unexpected live font diagnostic exception.";
            }
        }

        struct GateRescue
        {
            std::shared_ptr<ImageFontControlTestHooks> hooks;
            ~GateRescue()
            {
                hooks->ReleaseGate();
            }
        };
    }

    winrt::Windows::Foundation::IAsyncAction RunAsync(
        winrt::Windows::UI::Core::CoreDispatcher dispatcher, std::shared_ptr<CoreControllerTests::Results> results,
        std::shared_ptr<CoreValidation::Context> context)
    {
        ModuleLifetime module;
        if (!dispatcher || !dispatcher.HasThreadAccess() || !results || !context)
            throw winrt::hresult_invalid_argument();
        auto started = GetTickCount64();
        auto deadline = started + 38000; // Leave seven seconds for actual retirement.
        auto hooks = std::make_shared<ImageFontControlTestHooks>();
        GateRescue rescue{hooks};
        auto counts = std::make_shared<ProviderCounts>();
        CoreControllerLifetime lifetime;
        std::weak_ptr<ReadingMemory> sourceMemory;
        Panel parent{nullptr};
        Grid host{nullptr};
        View view{nullptr};
        IAsyncOperation<winrt::Notepads::Markdown::DocumentUpdateResult> update{nullptr};
        winrt::Windows::Storage::StorageFile journal{nullptr};
        std::wstring journalName, journalHeader;
        winrt::event_token publishedToken{};
        bool subscribed{}, attached{}, inserted{}, haveLifetime{};
        std::exception_ptr failure, cleanupFailure;
        std::wstring transcript, failureObservation;
        auto note = [&](std::wstring const &name)
        {
            auto observation = Access::Observe(view);
            transcript += L"\n" + name + L": " + observation.Describe() + L";" + counts->Describe();
            return observation;
        };
        auto noIo = [&]
        {
            Require(counts->calls == 2 && counts->returned == 2 && counts->Closes() == 2 && counts->ClosedOnce() &&
                        counts->wrongRequests == 0,
                    L"Live font refresh must preserve the exact two initial provider streams: " + counts->Describe());
        };
        try
        {
            FILETIME processCreated{}, processExited{}, kernelTime{}, userTime{}, now{};
            if (!GetProcessTimes(GetCurrentProcess(), &processCreated, &processExited, &kernelTime, &userTime))
                winrt::throw_last_error();
            GetSystemTimePreciseAsFileTime(&now);
            auto processStart = (static_cast<uint64_t>(processCreated.dwHighDateTime) << 32) | processCreated.dwLowDateTime;
            auto timestamp = (static_cast<uint64_t>(now.dwHighDateTime) << 32) | now.dwLowDateTime;
            journalName = L"font-control-live-" + std::to_wstring(GetCurrentProcessId()) + L"-" +
                          std::to_wstring(processStart) + L"-" + std::to_wstring(timestamp) + L".txt";
            journalHeader = L"pid=" + std::to_wstring(GetCurrentProcessId()) + L"\nprocessStartFileTime=" +
                            std::to_wstring(processStart) + L"\ncreatedFileTime=" + std::to_wstring(timestamp) + L"\n";
            auto folder = winrt::Windows::Storage::ApplicationData::Current().LocalFolder();
            journal = co_await folder.CreateFileAsync(journalName, winrt::Windows::Storage::CreationCollisionOption::FailIfExists);
            co_await winrt::Windows::Storage::FileIO::WriteTextAsync(journal, journalHeader + L"stage=starting\n",
                                                                   UnicodeEncoding::Utf8);
            hooks->phase = L"baseline";
            auto window = Window::Current();
            Require(window != nullptr, L"Live font test requires its actual application's current Window.");
            parent = window.Content().try_as<Panel>();
            Require(parent != nullptr, L"Live font test requires the existing own-window Content Panel.");
            host = Grid{};
            host.Width(540);
            host.Height(260);
            host.HorizontalAlignment(HorizontalAlignment::Left);
            host.VerticalAlignment(VerticalAlignment::Top);
            Grid::SetRow(host, 2);
            view = make<winrt::Notepads::Markdown::implementation::MarkdownView>();
            view.Width(480);
            view.Height(240);
            view.HorizontalAlignment(HorizontalAlignment::Left);
            view.VerticalAlignment(VerticalAlignment::Top);
            publishedToken = view.ViewportChanged(
                [hooks](View const &sender, winrt::Notepads::Markdown::MarkdownViewportAnchor const &)
                {
                    try
                    {
                        auto value = Access::Observe(sender);
                        ++hooks->viewportEvents;
                        if (value.frameSvgFound && value.frameSvg.state == NOTEPADS_MARKDOWN_IMAGE_READY)
                        {
                            hooks->wrongFontPublished = hooks->wrongFontPublished ||
                                value.frameSvg.published_target.font_generation != value.fontGeneration ||
                                !value.currentAction || !value.currentImage || value.frameTable != value.publishedTable;
                            hooks->forbiddenPublished = hooks->forbiddenPublished ||
                                (hooks->forbiddenRequest && value.frameSvg.published_request == hooks->forbiddenRequest);
                        }
                        if (hooks->publications.size() < 96)
                            hooks->publications.push_back(hooks->phase + L": " + value.Describe());
                        else
                            hooks->publicationError = true;
                    }
                    catch (...)
                    {
                        hooks->publicationError = true;
                    }
                });
            subscribed = true;
            host.Children().Append(view);
            parent.Children().Append(host);
            inserted = true;
            co_await WaitUntil(dispatcher, context, deadline, 5000,
                [&]
                {
                    if (!view.Dispatcher())
                        return false;
                    auto value = Access::Observe(view);
                    return value.loaded && value.actualWidth > 40 && value.viewportWidth > 40;
                }, L"real child Loaded/template/viewport");
            Require(Access::Observe(view).enabled,
                    L"Experimental SVG capability is disabled; the new live font check cannot pass or skip.");
            Access::Attach(view, hooks);
            attached = true;
            GUID createdDocument{};
            winrt::check_hresult(CoCreateGuid(&createdDocument));
            winrt::guid document{};
            static_assert(sizeof(document) == sizeof(createdDocument));
            std::memcpy(&document, &createdDocument, sizeof(document));
            constexpr std::string_view markdown = "![font](font-control:text)\n\n![raster](font-control:bmp)";
            auto provider = make<FixtureProvider>(document, counts);
            view.SetImageProvider(provider, 1);
            hooks->phase = L"reading-publication-before-prepare";
            hooks->readingPublication = std::make_shared<ReadingPublicationControlGate>();
            hooks->gateReading = true;
            view.IsResourceRenderingDeferred(true);
            // Both derived guards execute even without a derived occurrence;
            // the two-image source preserves this test's provider IO oracle.
            update = view.SetDocumentWithExtensionsAsync(document, 1,
                {reinterpret_cast<uint8_t const *>(markdown.data()), static_cast<uint32_t>(markdown.size())},
                static_cast<winrt::Notepads::Markdown::MarkdownExtensions>(
                    NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MATH | NOTEPADS_MARKDOWN_CORE_EXTENSIONS_MERMAID));
            co_await WaitUntil(dispatcher, context, deadline, 5000,
                [&] { return hooks->readingPublication->Reached(0); }, L"real first renderer before preparation");
            auto beforePrepare = note(L"first worker held before preparation");
            Require(beforePrepare.source && !beforePrepare.readingMain && !beforePrepare.readingDescriptors &&
                        !beforePrepare.frame && !beforePrepare.mathBound && !beforePrepare.diagramBound,
                    L"First render must be held before Main/semantics exist, without an old published Frame.");
            Access::EnsureFontWatch(view); // Actual pre-frame font consumer.
            auto callbacks = hooks->resourceCallbacks;
            view.IsResourceRenderingDeferred(false); // Public API -> actual low-priority callback.
            co_await WaitUntil(dispatcher, context, deadline, 3000,
                [&] { return hooks->resourceCallbacks > callbacks; }, L"deferred resources while preparation is held");
            auto pendingReading = note(L"UI consumers returned with unpublished reading");
            Require(!pendingReading.readingMain && !pendingReading.readingDescriptors && !pendingReading.frame &&
                        !pendingReading.mathBound && !pendingReading.diagramBound &&
                        !hooks->watchesCreated.load(std::memory_order_acquire) && !counts->calls,
                    L"Unpublished reading must leave resource admission pending and keep UI responsive.");
            view.IsResourceRenderingDeferred(true);
            hooks->readingPublication->Release(0);
            co_await WaitUntil(dispatcher, context, deadline, 5000,
                [&]
                {
                    // Keep real UI consumers active while the worker builds
                    // and publishes the snapshots, before either frame gate.
                    Access::EnsureFontWatch(view);
                    view.IsResourceRenderingDeferred(true);
                    view.IsResourceRenderingDeferred(false);
                    return hooks->readingPublication->Reached(1);
                }, L"real first renderer before Frame creation");
            hooks->phase = L"reading-publication-before-frame";
            auto beforeFrame = note(L"Main/semantics published while first Frame is held");
            Require(beforeFrame.textGeneration == beforePrepare.textGeneration &&
                        beforeFrame.readingMain && beforeFrame.readingDescriptors && !beforeFrame.frame &&
                        update.Status() == AsyncStatus::Started,
                    L"Actual worker must publish complete reading independently of its first Frame.");
            view.IsResourceRenderingDeferred(true);
            Access::EnsureFontWatch(view);
            callbacks = hooks->resourceCallbacks;
            view.IsResourceRenderingDeferred(false);
            co_await WaitUntil(dispatcher, context, deadline, 3000,
                [&]
                {
                    auto value = Access::Observe(view);
                    return hooks->resourceCallbacks > callbacks && value.mathBound && value.diagramBound && value.watchBaseline;
                }, L"actual resource binding before first Frame");
            auto boundReading = note(L"resources bound without a completed first Frame");
            Require(!boundReading.frame && !counts->calls &&
                        !hooks->readingPublication->timedOut.load(std::memory_order_acquire),
                    L"Resource admission must progress while first Frame work remains held; image IO still needs geometry.");
            Require(Access::CoalescePendingResourceLayout(view),
                    L"LayoutUpdated must retain the active first renderer while a real image refresh timer owns the newer table.");
            transcript += L"\nC10 resource-layout coalescing assertion passed before first-frame gate release.";
            hooks->readingPublication->Release(1);
            hooks->gateReading = false;
            hooks->phase = L"baseline";
            co_await WaitUntil(dispatcher, context, deadline, 7000,
                               [&] { return update.Status() != AsyncStatus::Started; }, L"live document publication");
            auto receipt = update.GetResults();
            Require(receipt.Status() == winrt::Notepads::Markdown::DocumentUpdateStatus::Ready && receipt.Published(),
                    L"Live document must publish through its actual MarkdownView pipeline.");
            update = nullptr;
            co_await WaitUntil(dispatcher, context, deadline, 6000,
                [&] { return Ready(Access::Observe(view)) && counts->ClosedOnce() && counts->returned == 2; },
                L"two initial drawn Ready images and real observer baseline");
            lifetime = Access::Lifetime(view);
            sourceMemory = Access::SourceMemory(view);
            haveLifetime = true;
            auto baseline = note(L"baseline");
            noIo();
            Require(Access::CheckRasterScaleTargets(view),
                    L"Actual SVG target scale and Mermaid DPI must share root/display scale precedence and finite fallbacks.");
            transcript += L"\nActual SVG/Mermaid captures passed root/display DPI divergence and invalid-input fallback checks.";
            Require(baseline.watchVersion == 0 && hooks->watchesCreated.load(std::memory_order_acquire) == 1,
                    L"Observer attachment must be a baseline, without a fabricated font notification.");

            hooks->phase = L"refresh";
            auto beforeEvents = hooks->viewportEvents;
            Require(Access::RequestRefresh(view), L"Real observer must be idle before the first explicit command.");
            co_await WaitUntil(dispatcher, context, deadline, 6000,
                [&]
                {
                    auto value = Access::Observe(view);
                    return value.fontGeneration > baseline.fontGeneration && Ready(value) &&
                           hooks->viewportEvents > beforeEvents;
                }, L"actual font/UI/Desire/Frame refresh");
            auto refreshed = note(L"refresh-ready");
            SameSource(refreshed, baseline);
            noIo();
            Require(refreshed.layoutGeneration > baseline.layoutGeneration &&
                        (refreshed.table != baseline.table || refreshed.tableGeneration != baseline.tableGeneration) &&
                        refreshed.svg.published_request != baseline.svg.published_request &&
                        refreshed.svg.published_target.target_generation > baseline.svg.published_target.target_generation,
                    L"Refresh must publish a genuinely new table/layout/target/request.");

            hooks->phase = L"late-candidate";
            hooks->gateNextTask = true;
            Require(Access::RequestRefresh(view), L"Real observer must be idle before the late-candidate command.");
            co_await WaitUntil(dispatcher, context, deadline, 5000,
                [&]
                {
                    auto task = hooks->HeldTask();
                    return task && task->testAfterCommitReached &&
                           WaitForSingleObjectEx(task->testAfterCommitReached.get(), 0, FALSE) == WAIT_OBJECT_0;
                }, L"actual next SVG task after-commit gate");
            auto oldTask = hooks->HeldTask();
            std::shared_ptr<NativeImageRequest> oldRequest;
            {
                std::lock_guard lock(oldTask->mutex);
                Require(oldTask->svgOwnerHandoffCommitted && static_cast<bool>(oldTask->request),
                        L"The held candidate must be the actual committed owner handoff.");
                oldRequest = oldTask->request;
            }
            hooks->forbiddenRequest = oldRequest->Key().request;
            auto held = note(L"after-commit-gate-reached");
            transcript += L";gate=actual-after-commit;heldRequest=" + std::to_wstring(hooks->forbiddenRequest);
            co_await WaitUntil(dispatcher, context, deadline, 2000,
                               [&] { return Access::RequestRefresh(view); }, L"second real observer command");
            co_await WaitUntil(dispatcher, context, deadline, 2000,
                [&]
                {
                    auto value = Access::Observe(view);
                    return value.fontGeneration > held.fontGeneration && !oldRequest->IsCurrent();
                }, L"new Desire invalidates the held actual candidate");
            note(L"held-candidate-invalidated");
            hooks->ReleaseGate();
            co_await WaitUntil(dispatcher, context, deadline, 3000,
                               [&] { return oldTask->finished.load(std::memory_order_acquire); },
                               L"old actual task returns through UI rejection and Finish");
            Require(!oldRequest->IsCurrent() && !hooks->forbiddenPublished,
                    L"The late real request must never publish after its font was superseded.");
            oldRequest.reset();
            oldTask.reset();
            hooks->ForgetTask();
            co_await WaitUntil(dispatcher, context, deadline, 5000,
                [&]
                {
                    auto value = Access::Observe(view);
                    return value.fontGeneration > held.fontGeneration && Ready(value);
                }, L"new-font Ready after actual stale candidate rejection");
            auto late = note(L"late-candidate-rejected-new-ready");
            SameSource(late, baseline);
            noIo();

            hooks->phase = L"freeze-failure";
            auto stable = note(L"pre-fault-ready");
            Require(Ready(stable), L"Freeze failure requires an actual stable old Ready table.");
            hooks->armedFreezeCall = hooks->freezeCalls;
            hooks->failNextFreeze = true;
            Require(Access::RequestRefresh(view), L"Real observer must be idle before the exact Freeze failure.");
            co_await WaitUntil(dispatcher, context, deadline, 4000,
                               [&] { return hooks->freezeFaultHits != 0; }, L"first common Freeze allocation failure");
            auto failed = note(L"freeze-fault-observed");
            transcript += L";freezeCall/armed/failed/hits=" + std::to_wstring(hooks->freezeCalls) + L"/" +
                          std::to_wstring(hooks->armedFreezeCall) + L"/" + std::to_wstring(hooks->failedFreezeCall) + L"/" +
                          std::to_wstring(hooks->freezeFaultHits);
            Require(hooks->freezeFaultHits == 1 && hooks->failedFreezeCall == hooks->armedFreezeCall + 1,
                    L"Fault must hit the first actual common Freeze allocation exactly once.");
            Require(failed.fontGeneration > stable.fontGeneration && failed.table == stable.table &&
                        failed.tableGeneration == stable.tableGeneration && failed.tableGeneration < failed.policyGeneration &&
                        failed.barrier && !failed.currentImage && failed.frameSvg.state == NOTEPADS_MARKDOWN_IMAGE_READY &&
                        failed.frameSvg.published_target.font_generation == stable.fontGeneration && !failed.error.empty(),
                    L"Fault must preserve the old Ready table behind the advanced policy/font barrier: " + failed.Describe());
            SameSource(failed, baseline);
            noIo();
            Require(!hooks->failNextFreeze, L"The exact Freeze fault must already be consumed.");
            hooks->phase = L"width-recovery";
            beforeEvents = hooks->viewportEvents;
            view.Width(520); // Only this real XAML change is allowed to drive recovery.
            co_await WaitUntil(dispatcher, context, deadline, 6000,
                [&]
                {
                    auto value = Access::Observe(view);
                    return Ready(value) && value.fontGeneration == failed.fontGeneration &&
                           value.viewportWidth > stable.viewportWidth + 1 && value.layoutGeneration > failed.barrier &&
                           hooks->viewportEvents > beforeEvents;
                }, L"same-font Freeze recovery driven only by actual Width/SizeChanged/LayoutUpdated");
            auto recovered = note(L"width-recovered-ready");
            SameSource(recovered, baseline);
            noIo();
            Require(recovered.fontGeneration == failed.fontGeneration &&
                        recovered.tableGeneration > stable.tableGeneration && hooks->freezeFaultHits == 1 &&
                        !hooks->wrongFontPublished && !hooks->forbiddenPublished && !hooks->publicationError,
                    L"Recovery must retain exact font/source/encoded identities and every real publication gate.");
        }
        catch (...)
        {
            failure = std::current_exception();
        }

        // Preserve failure state before Cancel/Unloaded destroys the table.
        if (view && dispatcher.HasThreadAccess())
        {
            try
            {
                failureObservation = hooks->phase + L": " + Access::Observe(view).Describe() + L";" + counts->Describe();
            }
            catch (...)
            {
                failureObservation = L"Final live observation unavailable; original failure retained.";
            }
        }
        hooks->ReleaseGate(); // Also safe when UI delivery has disappeared.
        std::array<std::shared_ptr<ImageLoadTask>, 2> tasks;
        bool uiDetached{};
        try
        {
            co_await winrt::resume_foreground(dispatcher);
            if (view)
            {
                if (!haveLifetime && Access::Observe(view).source)
                {
                    lifetime = Access::Lifetime(view);
                    sourceMemory = Access::SourceMemory(view);
                    haveLifetime = true;
                }
                tasks = Access::Tasks(view);
                if (subscribed)
                    view.ViewportChanged(publishedToken);
                if (attached)
                    Access::DetachAndStopWatch(view);
                if (update && update.Status() == AsyncStatus::Started)
                    update.Cancel();
                update = nullptr;
                view.Cancel();
            }
            if (inserted)
            {
                uint32_t index{};
                Require(parent.Children().IndexOf(host, index), L"The test must remove only its own actual child.");
                parent.Children().RemoveAt(index);
            }
            if (host)
                host.Children().Clear();
            view = nullptr;
            host = nullptr;
            parent = nullptr;
            uiDetached = true;
        }
        catch (...)
        {
            cleanupFailure = std::current_exception();
        }
        // No rescue closes a handle. Actual tasks retain their handles and
        // native owners even if a dispatcher or bounded retirement is lost.
        auto cleanupDeadline = std::min(started + 45000, GetTickCount64() + 7000);
        auto retired = [&]
        {
            auto isRetired = [](std::shared_ptr<ImageLoadTask> const &task)
            {
                if (!task)
                    return true;
                std::lock_guard lock(task->mutex);
                return task->finished.load(std::memory_order_acquire) && IsSvgImageTaskRetiredLocked(*task) &&
                       !task->svgQuarantined;
            };
            return std::all_of(tasks.begin(), tasks.end(), isRetired) && isRetired(hooks->HeldTask());
        };
        co_await winrt::resume_background();
        while (!retired() && GetTickCount64() < cleanupDeadline)
            co_await winrt::resume_after(std::chrono::milliseconds(10));
        auto tasksRetired = retired();
        hooks->ForgetTask();
        tasks = {};
        while (GetTickCount64() < cleanupDeadline && !sourceMemory.expired())
            co_await winrt::resume_after(std::chrono::milliseconds(10));
        auto memoryReleased = sourceMemory.expired();
        sourceMemory.reset();
        while (GetTickCount64() < cleanupDeadline &&
               ((haveLifetime && !lifetime.Snapshot().fullyReleased) ||
                hooks->watchesFreed.load(std::memory_order_acquire) != hooks->watchesCreated.load(std::memory_order_acquire)))
            co_await winrt::resume_after(std::chrono::milliseconds(10));
        auto final = lifetime.Snapshot();
        auto watchesCreated = hooks->watchesCreated.load(std::memory_order_acquire);
        auto watchesFreed = hooks->watchesFreed.load(std::memory_order_acquire);
        auto watchFailures = hooks->watchFreeFailures.load(std::memory_order_acquire);
        bool finalProviderValid = counts->calls == 2 && counts->returned == 2 && counts->Closes() == 2 &&
                                  counts->ClosedOnce() && counts->wrongRequests == 0;
        bool cleanupPassed = (!hooks->readingPublication || !hooks->readingPublication->timedOut.load(std::memory_order_acquire)) &&
                             finalProviderValid && uiDetached && tasksRetired && memoryReleased && watchesCreated == watchesFreed && !watchFailures &&
                             (!haveLifetime || (final.fullyReleased && final.modelPins == 0 &&
                              final.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK && final.cleanupHresult == 0));
        CoreValidation::CheckResult result{CheckName, !failure && !cleanupFailure && cleanupPassed, NOTEPADS_MARKDOWN_CORE_OK, {}};
        auto primaryText = ExceptionText(failure, result.status);
        int32_t cleanupStatus = NOTEPADS_MARKDOWN_CORE_OK;
        auto cleanupText = ExceptionText(cleanupFailure, cleanupStatus);
        if (!result.passed && result.status == NOTEPADS_MARKDOWN_CORE_OK)
            result.status = cleanupStatus != NOTEPADS_MARKDOWN_CORE_OK ? cleanupStatus : NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
        result.detail = L"Phase=" + hooks->phase + L";failure=" + primaryText + L"\nlast=" + failureObservation +
                        L"\ncleanup: UI-detached=" + std::to_wstring(uiDetached) + L";tasks-retired=" +
                        std::to_wstring(tasksRetired) + L";source-expired=" + std::to_wstring(memoryReleased) +
                        L";watch-created/freed/failures=" + std::to_wstring(watchesCreated) + L"/" +
                        std::to_wstring(watchesFreed) + L"/" + std::to_wstring(watchFailures) + L";fullyReleased=" +
                        std::to_wstring(final.fullyReleased) + L";modelPins=" + std::to_wstring(final.modelPins) +
                        L";cleanup-status/hresult=" + std::to_wstring(final.cleanupStatus) + L"/" +
                        std::to_wstring(final.cleanupHresult) + L";error=" + cleanupText + L"\n" + counts->Describe() +
                        L"\nfreeze-call/armed/failed/hits=" + std::to_wstring(hooks->freezeCalls) + L"/" +
                        std::to_wstring(hooks->armedFreezeCall) + L"/" + std::to_wstring(hooks->failedFreezeCall) + L"/" +
                        std::to_wstring(hooks->freezeFaultHits) + L";viewport-events=" + std::to_wstring(hooks->viewportEvents) +
                        L";forbidden-request/published=" + std::to_wstring(hooks->forbiddenRequest) + L"/" +
                        std::to_wstring(hooks->forbiddenPublished) + L";wrong-font/publication-error=" +
                        std::to_wstring(hooks->wrongFontPublished) + L"/" + std::to_wstring(hooks->publicationError) + transcript;
        for (auto const &publication : hooks->publications)
            result.detail += L"\nViewportChanged " + publication;
        result.detail += L"\nActual observer command/UI delivery only; no system font installation/event or full budget/G0 claim.";
        if (hooks->readingPublication && hooks->readingPublication->firstCompleted.load(std::memory_order_acquire))
        {
            auto const &gate = *hooks->readingPublication;
            result.detail = L"reading-gate completed/first-status/token/latest/layout=" +
                            std::to_wstring(gate.completedRequests.load()) + L"/" +
                            std::to_wstring(gate.firstStatus.load()) + L"/" + std::to_wstring(gate.firstToken.load()) +
                            L"/" + std::to_wstring(gate.firstLatestToken.load()) + L"/" +
                            std::to_wstring(gate.firstLayout.load()) + L"\n" + result.detail;
        }
        std::exception_ptr journalFailure;
        bool journalWritten{};
        try
        {
            if (journal)
            {
                co_await winrt::Windows::Storage::FileIO::WriteTextAsync(journal,
                    journalHeader + L"stage=complete\npassed=" + std::to_wstring(result.passed) + L"\nstatus=" +
                    std::to_wstring(result.status) + L"\n" + result.detail, UnicodeEncoding::Utf8);
                journalWritten = true;
            }
        }
        catch (...)
        {
            journalFailure = std::current_exception();
        }
        int32_t journalStatus = NOTEPADS_MARKDOWN_CORE_OK;
        auto journalText = ExceptionText(journalFailure, journalStatus);
        if (!journalWritten)
        {
            result.passed = false;
            if (result.status == NOTEPADS_MARKDOWN_CORE_OK)
                result.status = journalStatus != NOTEPADS_MARKDOWN_CORE_OK ? journalStatus : NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
        }
        // Keep the concrete independent original's name inside the unchanged
        // probe's bounded exception detail, including the deliberate old bug.
        result.detail = L"journal=LocalState/" + journalName + L";complete=" + std::to_wstring(journalWritten) +
                        L";phase=" + hooks->phase + L";failure=" + primaryText + L";journal-error=" + journalText +
                        L"\n" + result.detail;
        if (!result.passed)
            context->RecordPrimary(CoreValidation::FaultKind::Assertion, result.status, CheckName);
        results->checks.push_back(std::move(result));
        if (!cleanupPassed || cleanupFailure)
            context->RecordCleanupInvariant(CheckName);
    }
}
