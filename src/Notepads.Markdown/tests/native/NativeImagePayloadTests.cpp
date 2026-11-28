// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeImageTestSupport.h"
#include "NativeImagePayloadTests.h"
#include "Resources/Images/NativeImageResources.h"
#include "Resources/Images/NativeImageCoordinator.h"
#include "Resources/Images/NativeImagePresentation.h"
#include <d3d11.h>
#include "Resources/Svg/NativeSvgJob.h"
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
#include "Control/MarkdownView.h"
#include "NativeImageFontControlTests.h"
#include "NativeImageReuseControlTests.h"
#include <winrt/Windows.Foundation.Collections.h>
#include <winrt/Windows.UI.Xaml.Controls.h>
#endif
#include <algorithm>
#include <array>
#include <chrono>
#include <cstring>
#include <exception>
#include <functional>
#include <source_location>
#include <thread>

namespace Notepads::Markdown::Native::ImagePayloadTests
{
    namespace
    {
        template <typename T> T Pod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(T)};
            return value;
        }

        void Check(int32_t status)
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw NativeImageFailure{status};
        }

        void Require(bool value, std::source_location location = std::source_location::current())
        {
            if (!value)
                throw winrt::hresult_error{E_FAIL, L"Image payload assertion at line " + std::to_wstring(location.line())};
        }

        template <typename Action> void Refuses(Action action, int32_t expected = NOTEPADS_MARKDOWN_CORE_INVALID_STATE)
        {
            bool refused{};
            try { action(); }
            catch (NativeFailure const &failure) { refused = failure.status == expected; }
            Require(refused);
        }

        template <typename Predicate> bool Until(Predicate predicate)
        {
            auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
            while (!predicate())
            {
                if (std::chrono::steady_clock::now() >= deadline)
                    return false;
                std::this_thread::sleep_for(std::chrono::milliseconds(1));
            }
            return true;
        }

        class HookScope final
        {
          public:
            explicit HookScope(ReadingMemoryTestHooks &hooks) noexcept : _old(ReadingMemory::SetTestHooks(&hooks)) {}
            ~HookScope() { ReadingMemory::SetTestHooks(_old); }
          private:
            ReadingMemoryTestHooks *_old;
        };

        notepads_markdown_image_epoch Epoch(ReadingMemory const &memory)
        {
            notepads_markdown_image_epoch epoch{};
            auto const &identity = memory.Identity();
            std::copy(std::begin(identity.document), std::end(identity.document), std::begin(epoch.source.document));
            epoch.source.revision = identity.revision;
            epoch.source_generation = epoch.provider_generation = 1;
            return epoch;
        }

        std::shared_ptr<ImagePixels> Pixels(std::shared_ptr<ReadingMemory> const &memory,
                                            std::shared_ptr<NativeImageRequest> const &request)
        {
            auto charge = request->Reserve(NOTEPADS_MARKDOWN_IMAGE_DECODED, 16);
            std::shared_ptr<ImagePixels> pixels;
            {
                ImagePayloadConstruction construction{*memory};
                pixels = std::allocate_shared<ImagePixels>(ImagePayloadAllocator<ImagePixels>{construction.Nonce()}, memory, charge);
            }
            pixels->AllocatePixels(*memory, 16);
            pixels->BindRasterAllocation();
            charge->Admit(pixels->pixels.capacity());
            pixels->metadata = MakeRasterImageMetadata({2, 2, 2, 2, 1, 8});
            auto writable = pixels->MutablePixels();
            std::fill(writable.begin(), writable.end(), uint8_t{0x39});
            return pixels;
        }

        struct AllocationObserver
        {
            void *first{};
            static void Observe(void *context, void *memory, ReadingMemoryTestEvent event) noexcept
            {
                auto &observer = *static_cast<AllocationObserver *>(context);
                if (event == ReadingMemoryTestEvent::Allocated && !observer.first)
                    observer.first = memory;
            }
        };

        struct Throws final
        {
            Throws() { throw winrt::hresult_error{E_ABORT}; }
        };

        struct FactoryObserver
        {
            uint32_t calls{};
            size_t bytes{};
            static void *Allocate(void *context, ReadingMemory const &memory, size_t bytes)
            {
                auto &observer = *static_cast<FactoryObserver *>(context);
                ++observer.calls;
                observer.bytes = bytes;
                return memory.Allocate(bytes);
            }
        };

        constexpr std::array<uint8_t, 70> Bmp{
            'B', 'M', 70, 0, 0, 0, 0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0, 2, 0, 0, 0, 2, 0,
            0, 0, 1, 0, 24, 0, 0, 0, 0, 0, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 255, 0, 0, 255, 255, 255, 0, 0, 0, 0, 255, 0, 255, 0, 0, 0};
        // Original independent top-down PBGRA oracle for the 2x2 bottom-up BMP.
        constexpr std::array<uint8_t, 16> Expected{
            0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255};
    }

    Results Run(std::shared_ptr<ReadingMemory> const &memory)
    {
        Require(static_cast<bool>(memory));
        Results result;
        ImagePayloadAllocator<uint32_t> saved;
        {
            ImagePayloadConstruction outer{*memory};
            saved = ImagePayloadAllocator<uint32_t>{outer.Nonce()};
            ImagePayloadAllocator<uint8_t> rebound{saved};
            Require(saved == rebound);
            std::atomic<bool> otherThreadRejected{};
            std::thread other{[saved, &otherThreadRejected]() mutable
            {
                try { (void)saved.allocate(1); }
                catch (NativeFailure const &failure)
                {
                    otherThreadRejected = failure.status == NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
                }
            }};
            other.join();
            Require(otherThreadRejected);
            {
                ImagePayloadConstruction inner{*memory};
                Refuses([&] { (void)saved.allocate(1); });
                ImagePayloadAllocator<uint8_t> nested{inner.Nonce()};
                auto bytes = nested.allocate(7);
                Require(ReadingMemory::InspectAllocation(bytes).requestedTotal == ReadingMemory::AllocationRequestBytes(7));
                nested.deallocate(bytes, 7);
                Refuses([&] { (void)nested.allocate(1); });
            }
            auto words = saved.allocate(3);
            Require(ReadingMemory::InspectAllocation(words).requestedTotal == ReadingMemory::AllocationRequestBytes(12));
            saved.deallocate(words, 3);
            Refuses([&] { (void)saved.allocate(1); });
        }
        Refuses([&] { (void)saved.allocate(1); });
        Refuses([&] { (void)saved.allocate(SIZE_MAX); }, NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW);
        result.scopedAllocation = true;
        result.checks += 8;

        FactoryObserver factory;
        ImagePayloadAllocator<uint32_t> closedFactory;
        std::shared_ptr<uint32_t> scalar;
        {
            ImagePayloadConstruction construction{*memory, {&factory, FactoryObserver::Allocate}};
            closedFactory = ImagePayloadAllocator<uint32_t>{construction.Nonce()};
            scalar = std::allocate_shared<uint32_t>(closedFactory, uint32_t{123});
        }
        Refuses([&] { (void)closedFactory.allocate(1); });
        Require(factory.calls == 1 && factory.bytes >= sizeof(uint32_t) && *scalar == 123);
        scalar.reset();
        Require(factory.calls == 1);
        result.checks += 3;

        ReadingFreeReceipt cleanup;
        bool threw{};
        {
            ReadingFreeScope free{cleanup};
            ImagePayloadConstruction construction{*memory};
            try { (void)std::allocate_shared<Throws>(ImagePayloadAllocator<Throws>{construction.Nonce()}); }
            catch (winrt::hresult_error const &failure) { threw = failure.code() == E_ABORT; }
        }
        Require(threw && cleanup.Succeeded() && cleanup.physicalFreed == 1 && cleanup.rootReleased == 1);
        result.constructorFailure = true;
        ++result.checks;

        auto policy = NativeImagePolicy::Create();
        policy->Begin(Epoch(*memory));
        auto request = StartImageTestRequest(policy, 1);
        auto pixels = Pixels(memory, request);
        Refuses([&] { pixels->AllocatePixels(*memory, 16); });
        pixels->Freeze();
        Require(pixels->IsFrozen() && pixels->pixels.size() == 16 && pixels->pixels.capacity() == 16);
        Refuses([&] { (void)pixels->MutablePixels(); });
        Refuses([&] { pixels->AllocatePixels(*memory, 32); });
        Require(std::all_of(pixels->pixels.begin(), pixels->pixels.end(), [](uint8_t value) { return value == 0x39; }));
        pixels.reset();
        Require(policy->Snapshot().header.decoded_bytes == 0);
        result.immutablePixels = true;
        result.checks += 6;

        auto encoded = NativeEncodedImage::Create(memory, request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, 16), 16);
        encoded->Append(Expected);
        encoded->Freeze();
        {
            auto another = encoded;
            Require(encoded->IsFrozen() && !encoded.IsUnique());
            Refuses([&] { encoded->Append({}); });
        }
        Require(encoded.IsUnique());
        Refuses([&] { encoded->Append({}); });
        Require(std::equal(Expected.begin(), Expected.end(), encoded->Bytes().begin()));
        encoded.Reset();
        request.reset();
        Require(policy->Snapshot().header.reservations == 0);
        result.immutableEncoded = true;
        result.checks += 6;
        return result;
    }

    void RunDrawCacheRegression(std::shared_ptr<ReadingMemory> const &memory)
    {
        winrt::com_ptr<ID2D1Factory1> factory;
        winrt::check_hresult(D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, factory.put()));
        auto createDevice = [&]
        {
            winrt::com_ptr<ID3D11Device> d3d;
            winrt::check_hresult(D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_WARP, nullptr,
                D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0, D3D11_SDK_VERSION, d3d.put(), nullptr, nullptr));
            winrt::com_ptr<ID2D1Device> device;
            winrt::check_hresult(factory->CreateDevice(d3d.as<IDXGIDevice>().get(), device.put()));
            return device;
        };
        auto createContext = [](ID2D1Device *device)
        {
            winrt::com_ptr<ID2D1DeviceContext> context;
            winrt::check_hresult(device->CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE, context.put()));
            return context;
        };
        auto device = createDevice();
        auto context = createContext(device.get());
        auto policy = NativeImagePolicy::Create();
        policy->Begin(Epoch(*memory));
        auto request = StartImageTestRequest(policy, 1);
        auto pixels = Pixels(memory, request);
        pixels->Freeze();
        ImageDrawCache cache;
        cache.Begin(context.get());
        winrt::com_ptr<ID2D1Bitmap1> first;
        first.copy_from(cache.Bitmap(context.get(), pixels));
        cache.End();
        // One entirely image-free frame, followed by another surface context
        // on the same device, must reuse the original upload.
        cache.Begin(context.get());
        cache.End();
        auto nextContext = createContext(device.get());
        cache.Begin(nextContext.get());
        Require(cache.Bitmap(nextContext.get(), pixels) == first.get());
        cache.End();
        // Keep old test-owned COM references alive so pointer identity cannot
        // be recycled and hide an incorrect cross-device/cache-clear reuse.
        auto nextDevice = createDevice();
        auto otherContext = createContext(nextDevice.get());
        cache.Begin(otherContext.get());
        winrt::com_ptr<ID2D1Bitmap1> second;
        second.copy_from(cache.Bitmap(otherContext.get(), pixels));
        Require(second.get() != first.get());
        cache.End();
        cache.Clear();
        cache.Begin(otherContext.get());
        Require(cache.Bitmap(otherContext.get(), pixels) != second.get());
        cache.End();
        std::weak_ptr<ImagePixels> weak = pixels;
        pixels.reset();
        Require(weak.expired() && policy->Snapshot().header.decoded_bytes == 0);
        cache.Begin(otherContext.get());
        cache.End();
        cache.Clear();
        request.reset();
        Require(policy->Snapshot().header.reservations == 0);
    }
    void RunDirectSvgFreezeRegression(std::shared_ptr<ReadingMemory> const &memory,
                                     std::shared_ptr<ImageLoadTask> *retiredTask)
    {
        auto policy = NativeImagePolicy::Create();
        policy->Begin(Epoch(*memory));
        constexpr std::string_view svg = "<svg xmlns='http://www.w3.org/2000/svg' width='64' height='64'><rect width='64' height='64' fill='red'/></svg>";
        struct Payload
        {
            std::shared_ptr<ImagePixels> pixels;
            NativeEncodedImageHandle encoded;
        };
        auto render = [&](uint64_t generation, double scale, NativeEncodedImageHandle encoded)
        {
            std::shared_ptr<NativeImageRequest> request;
            try
            {
                policy->Desire(1, {1, generation, 64, scale});
                request = policy->Start(1, NOTEPADS_MARKDOWN_IMAGE_SVG);
            }
            catch (NativeFailure const &error)
            {
                throw winrt::hresult_error{error.code(), L"Direct SVG admission generation=" + std::to_wstring(generation) +
                    L" status=" + std::to_wstring(error.status)};
            }
            auto task = ImageLoadTask::Create(memory);
            task->request = std::move(request);
            auto result = std::shared_ptr<NativeImageDecodeResult>{task, &task->decodeResult};
            try
            {
                if (!encoded)
                {
                    encoded = NativeEncodedImage::Create(memory, task->request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, svg.size()), svg.size());
                    encoded->Append({reinterpret_cast<uint8_t const *>(svg.data()), svg.size()});
                }
                // This is the control's direct retarget entry, with no outer
                // DecodeImageAsync call to freeze a successful publication.
                RenderSvgImageOnBackground(task, memory, std::move(encoded), {64, scale, generation, true, 1}, result);
                Require(result->Ready() && result->svg && result->pixels->IsFrozen());
                Refuses([&] { (void)result->pixels->MutablePixels(); });
                Require(task->svgRetired && !task->svgQuarantined && !task->svgKeepAlive && !task->svgResources &&
                            !task->svgAccount && !task->svgCoreCancel && task->svgReceipt.threadSignaled &&
                            task->svgReceipt.cancelLeasesZero && task->svgReceipt.nodeFreed &&
                            task->svgReceipt.resources.cleanup_succeeded == 1);
                try
                {
                    task->request->CompleteReady(result->pixels->metadata, result->pixels->charge);
                }
                catch (NativeFailure const &error)
                {
                    throw winrt::hresult_error{error.code(), L"Direct SVG completion generation=" + std::to_wstring(generation) +
                        L" status=" + std::to_wstring(error.status)};
                }
                Payload payload{std::move(result->pixels), std::move(result->encoded)};
                task->Finish();
                Require(!task->finished.load(std::memory_order_acquire) && task->svgFinishPending && task->request);
                ImageCoordinator retired;
                retired.tasks[0] = task;
                retired.Retire();
                Require(retired.tasks[0] == task && policy->Snapshot().header.active_requests == 1);
                // The actual SVG worker/borrows have retired, but its outer
                // decoder has not yet published the IO-tail boundary. Finish
                // must wait; Notify performs the late Finish without a Pump.
                task->NotifyDecodeComplete();
                Require(task->finished.load(std::memory_order_acquire) && !task->request &&
                            policy->Snapshot().header.active_requests == 0);
                if (retiredTask)
                    *retiredTask = task;
                return payload;
            }
            catch (...)
            {
                task->NotifyDecodeComplete();
                task->Finish();
                throw;
            }
        };
        auto first = render(1, 1, {});
        auto second = render(2, 2, first.encoded);
        Require(first.pixels->IsFrozen() && second.pixels->IsFrozen() && first.encoded->IsFrozen() && second.encoded->IsFrozen());
        Require(first.pixels->metadata.decoded_width == 64 && second.pixels->metadata.decoded_width == 128);
        Refuses([&] { (void)first.pixels->MutablePixels(); });
        first = {};
        Require(policy->Snapshot().header.decoded_bytes == 128 * 128 * 4);
        second = {};
        Require(policy->Snapshot().header.active_requests == 0 && policy->Snapshot().header.reservations == 0);
    }

    std::shared_ptr<ImageCoordinator> RunTaskRetirementRegression(std::shared_ptr<ReadingMemory> const &memory)
    {
        auto coordinator = std::make_shared<ImageCoordinator>();
        coordinator->policy = NativeImagePolicy::Create();
        coordinator->policy->Begin(Epoch(*memory));
        auto task = ImageLoadTask::Create(memory);
        task->request = StartImageTestRequest(coordinator->policy, 1);
        coordinator->tasks[0] = task;
        task->Finish();
        std::weak_ptr<ImageLoadTask> finished = task;
        task.reset();
        coordinator->Retire();
        Require(!coordinator->tasks[0] && finished.expired() && coordinator->policy->Snapshot().header.active_requests == 0);
        finished.reset();

        // A deterministic state-gate negative, not a synthetic SVG worker
        // failure: quarantine must never be treated as finished/permit-free.
        task = ImageLoadTask::Create(memory);
        task->request = StartImageTestRequest(coordinator->policy, 2);
        coordinator->tasks[1] = task;
        task->svgQuarantined = true;
        task->Finish();
        coordinator->Retire();
        coordinator->ClearFinishedTasks();
        Require(coordinator->tasks[1] == task && !task->finished.load(std::memory_order_acquire) && task->request &&
                    coordinator->policy->Snapshot().header.active_requests == 1);
        task->svgQuarantined = false; // Restore only this test-owned gate; no worker/resource existed.
        task->Finish();
        Require(task->finished.load(std::memory_order_acquire) && coordinator->policy->Snapshot().header.active_requests == 0);
        task.reset();
        RunDirectSvgFreezeRegression(memory, &coordinator->tasks[0]);
        Require(coordinator->tasks[0] && coordinator->tasks[0]->finished.load(std::memory_order_acquire) &&
                    coordinator->tasks[0]->metadataRetained &&
                    ReadingMemory::InspectAllocation(coordinator->tasks[0]->allocationBlock).hasAction);
        return coordinator;
    }

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    namespace
    {
        using View = winrt::Notepads::Markdown::MarkdownView;
        using Access = winrt::Notepads::Markdown::implementation::ImageFontControlTestAccess;
        using namespace winrt::Windows::Foundation;
        using namespace winrt::Windows::Storage::Streams;

        struct PendingProviderState
        {
            winrt::handle entered{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
            winrt::handle proceed{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
            std::atomic<uint32_t> opens{}, returned{};
            winrt::guid document;
            bool svg{};
        };

        struct PendingProvider : winrt::implements<PendingProvider, winrt::Notepads::Markdown::IMarkdownImageProvider>
        {
            explicit PendingProvider(std::shared_ptr<PendingProviderState> state) : _state(std::move(state)) {}
            IAsyncOperation<IRandomAccessStream> OpenAsync(winrt::Notepads::Markdown::MarkdownImageRequest request)
            {
                auto self = get_strong();
                Require(request.DocumentId() == _state->document && request.Revision() == 1 && request.ProviderGeneration() == 1);
                ++_state->opens;
                InMemoryRandomAccessStream stream;
                DataWriter writer{stream};
                constexpr std::string_view svg = "<svg xmlns='http://www.w3.org/2000/svg' width='8' height='8'><rect width='8' height='8' fill='red'/></svg>";
                if (_state->svg)
                    writer.WriteBytes({reinterpret_cast<uint8_t const *>(svg.data()), static_cast<uint32_t>(svg.size())});
                else
                    writer.WriteBytes(Bmp);
                co_await writer.StoreAsync();
                writer.DetachStream();
                writer = nullptr;
                stream.Seek(0);
                winrt::check_bool(SetEvent(_state->entered.get()));
                // Keep this gate as the last await: a legitimate cancellation
                // must not instead throw at a later StoreAsync checkpoint and
                // bypass the physical-return observation being tested.
                if (!_state->svg)
                    co_await winrt::resume_on_signal(_state->proceed.get());
                ++_state->returned;
                co_return stream;
            }
          private:
            std::shared_ptr<PendingProviderState> _state;
        };

        IAsyncAction WaitUi(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                            std::shared_ptr<CoreValidation::Context> context, std::function<bool()> predicate)
        {
            auto deadline = GetTickCount64() + 10000;
            while (!predicate())
            {
                context->Arbitrate();
                if (GetTickCount64() >= deadline)
                    throw winrt::hresult_error{HRESULT_FROM_WIN32(ERROR_TIMEOUT), L"Hidden image task retirement timed out"};
                co_await winrt::resume_after(std::chrono::milliseconds(10));
                co_await winrt::resume_foreground(dispatcher);
            }
        }

        IAsyncAction HiddenLateFinish(winrt::Windows::UI::Core::CoreDispatcher dispatcher,
                                      std::shared_ptr<CoreValidation::Context> context, bool svg, bool cacheOnUnload)
        {
            co_await winrt::resume_foreground(dispatcher);
            winrt::Windows::UI::Xaml::Controls::Panel parent{nullptr};
            winrt::Windows::UI::Xaml::Controls::Grid host{nullptr};
            View view{nullptr};
            IAsyncOperation<winrt::Notepads::Markdown::DocumentUpdateResult> update{nullptr};
            auto state = std::make_shared<PendingProviderState>();
            auto hooks = std::make_shared<ImageFontControlTestHooks>();
            std::array<std::shared_ptr<ImageLoadTask>, 2> tasks;
            CoreControllerLifetime lifetime;
            std::weak_ptr<ReadingMemory> source;
            bool inserted{}, attached{};
            std::wstring stage = svg ? L"SVG: setup" : L"BMP: setup";
            std::exception_ptr failure;
            try
            {
                Require(state->entered && state->proceed);
                GUID document{};
                winrt::check_hresult(CoCreateGuid(&document));
                static_assert(sizeof(document) == sizeof(state->document));
                std::memcpy(&state->document, &document, sizeof(document));
                state->svg = svg;
                parent = winrt::Windows::UI::Xaml::Window::Current().Content().try_as<winrt::Windows::UI::Xaml::Controls::Panel>();
                Require(parent != nullptr);
                host = winrt::Windows::UI::Xaml::Controls::Grid{};
                host.Width(480);
                host.Height(240);
                winrt::Windows::UI::Xaml::Controls::Grid::SetRow(host, 2);
                view = winrt::make<winrt::Notepads::Markdown::implementation::MarkdownView>();
                view.CacheDocumentOnUnload(cacheOnUnload);
                view.Width(480);
                view.Height(240);
                host.Children().Append(view);
                parent.Children().Append(host);
                inserted = true;
                stage = svg ? L"SVG: Loaded/viewport" : L"BMP: Loaded/viewport";
                co_await WaitUi(dispatcher, context, [&] { return Access::Observe(view).loaded && view.ActualWidth() > 100; });
                if (svg)
                    Require(Access::Observe(view).enabled);
                hooks->gateNextTask = svg;
                hooks->gateColdAutoTask = svg;
                Access::Attach(view, hooks);
                attached = true;
                view.SetImageProvider(winrt::make<PendingProvider>(state), 1);
                constexpr std::string_view markdown = "![pending](task-retirement:asset)";
                update = view.SetDocumentAsync(state->document, 1,
                    {reinterpret_cast<uint8_t const *>(markdown.data()), static_cast<uint32_t>(markdown.size())});
                stage = svg ? L"SVG: admitted task reached after-commit gate" : L"BMP: admitted provider entered";
                co_await WaitUi(dispatcher, context, [&]
                {
                    if (!svg)
                        return WaitForSingleObjectEx(state->entered.get(), 0, FALSE) == WAIT_OBJECT_0;
                    auto task = hooks->HeldTask();
                    return task && task->testAfterCommitReached &&
                           WaitForSingleObjectEx(task->testAfterCommitReached.get(), 0, FALSE) == WAIT_OBJECT_0;
                });
                stage = svg ? L"SVG: document Ready" : L"BMP: document Ready";
                co_await WaitUi(dispatcher, context, [&] { return update.Status() != AsyncStatus::Started; });
                Require(update.GetResults().Status() == winrt::Notepads::Markdown::DocumentUpdateStatus::Ready);
                update = nullptr;
                source = Access::SourceMemory(view);
                lifetime = Access::Lifetime(view);
                tasks = Access::Tasks(view);
                Require(!source.expired() && std::any_of(tasks.begin(), tasks.end(), [](auto const &task) { return bool(task); }));
                view.SuspendCachedDocument();
                view.DiscardCachedDocument();
                Require(!view.HasCachedDocument() && !Access::Observe(view).loaded);
                Require(std::any_of(tasks.begin(), tasks.end(), [](auto const &task)
                {
                    if (!task)
                        return false;
                    std::lock_guard lock(task->mutex);
                    return !task->finished.load(std::memory_order_acquire) && bool(task->request);
                }));
                Require(!source.expired()); // The real unfinished provider/worker still owns the model.
                winrt::check_bool(SetEvent(state->proceed.get()));
                hooks->ReleaseGate();
                stage = svg ? L"SVG: hidden late task/model retirement" : L"BMP: hidden late task/model retirement";
                co_await WaitUi(dispatcher, context, [&]
                {
                    return std::all_of(tasks.begin(), tasks.end(), [](auto const &task)
                        { return !task || task->finished.load(std::memory_order_acquire); }) &&
                           source.expired() && lifetime.Snapshot().modelPins == 0;
                });
                Require(!view.HasCachedDocument() && !Access::Observe(view).loaded && lifetime.Snapshot().cleanupStatus == 0 &&
                            lifetime.Snapshot().cleanupHresult == 0);
                // Keep the actual finished task values/control blocks strong
                // during this proof. Neither their paid metadata nor the still
                // hidden control may pin the accepted model; no Resume occurs.
                Require(state->opens == 1 && state->returned == 1);
                if (!svg)
                {
                    auto slots = Access::Tasks(view);
                    Require(std::all_of(slots.begin(), slots.end(), [](auto const &task) { return !task; }));
                }
            }
            catch (...)
            {
                auto status = winrt::to_hresult();
                auto message = stage + L";cacheOnUnload=" + std::to_wstring(cacheOnUnload) + L"; " + std::wstring(winrt::to_message()) +
                    L";opens/returned=" + std::to_wstring(state->opens.load()) + L"/" + std::to_wstring(state->returned.load());
                try
                {
                    if (view)
                    {
                        auto observed = Access::Observe(view);
                        message += L";isLoaded/hasSource=" + std::to_wstring(observed.loaded) + L"/" +
                            std::to_wstring(observed.source) + L";view=" + observed.Describe();
                    }
                    message += L";sourceExpired/modelPins=" + std::to_wstring(source.expired()) + L"/" +
                        std::to_wstring(lifetime.Snapshot().modelPins);
                    for (size_t index = 0; index < tasks.size(); ++index)
                    {
                        auto const &task = tasks[index];
                        if (!task)
                            continue;
                        std::lock_guard lock(task->mutex);
                        message += L";task" + std::to_wstring(index) + L" finished/decodeStarted/svgStarted/tail/quarantine/request=" +
                            std::to_wstring(task->finished.load(std::memory_order_acquire)) + L"/" +
                            std::to_wstring(task->decodeStarted) + L"/" + std::to_wstring(task->svgStarted) + L"/" +
                            std::to_wstring(task->svgDecodeComplete) + L"/" + std::to_wstring(task->svgQuarantined) + L"/" +
                            std::to_wstring(bool(task->request));
                    }
                    if (update)
                        message += L";updateStatus=" + std::to_wstring(static_cast<int>(update.Status()));
                }
                catch (...) {} // Preserve the original failure if observation itself fails.
                failure = std::make_exception_ptr(winrt::hresult_error{status, message});
            }
            SetEvent(state->proceed.get());
            hooks->ReleaseGate();
            if (view)
            {
                if (attached)
                    Access::DetachAndStopWatch(view);
                view.Cancel();
            }
            if (inserted)
            {
                uint32_t index{};
                if (parent.Children().IndexOf(host, index))
                    parent.Children().RemoveAt(index);
            }
            if (host)
                host.Children().Clear();
            view = nullptr;
            update = nullptr;
            host = nullptr;
            parent = nullptr;
            co_await winrt::resume_background();
            auto deadline = GetTickCount64() + 10000;
            while (GetTickCount64() < deadline && std::any_of(tasks.begin(), tasks.end(), [](auto const &task)
                { return task && !task->finished.load(std::memory_order_acquire); }))
                co_await winrt::resume_after(std::chrono::milliseconds(10));
            auto retired = std::all_of(tasks.begin(), tasks.end(), [](auto const &task)
                { return !task || task->finished.load(std::memory_order_acquire); });
            hooks->ForgetTask();
            tasks = {};
            if (failure)
                std::rethrow_exception(failure);
            Require(retired);
        }
    }
#endif

    winrt::Windows::Foundation::IAsyncAction RunAsync(
        winrt::Windows::UI::Core::CoreDispatcher dispatcher,
        std::shared_ptr<CoreControllerTests::Results> results,
        std::shared_ptr<CoreValidation::Context> context)
    {
        auto controller = CoreController::CreatePreview(dispatcher);
        auto lifetime = controller->Lifetime();
        CoreCompletionTicket ticket;
        CoreModelPin pin;
        notepads_markdown_core_cancel *sourceFlag{};
        bool registered{}, sourceRegistered{};
        std::shared_ptr<ReadingMemory> memory;
        std::weak_ptr<ReadingMemory> oldMemory;
        std::shared_ptr<NativeImagePolicy> policy;
        std::shared_ptr<ImagePixels> pixels;
        std::weak_ptr<ImagePixels> weakMetadata;
        NativeEncodedImageHandle encoded;
        ImagePayloadAllocator<uint8_t> closedAllocator;
        AllocationObserver metadata;
        std::shared_ptr<ImageCoordinator> retiredTasks;
        std::exception_ptr failure;
        CoreDrainResult drain;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        auto reuseDiagnostics = std::make_shared<ImageReuseControlTests::FailureDiagnostics>();
#endif
        try
        {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            co_await HiddenLateFinish(dispatcher, context, false, false);
            co_await HiddenLateFinish(dispatcher, context, false, true);
            co_await HiddenLateFinish(dispatcher, context, true, false);
            co_await HiddenLateFinish(dispatcher, context, true, true);
            co_await ImageReuseControlTests::RunAsync(dispatcher, context, reuseDiagnostics);
            co_await winrt::resume_foreground(dispatcher);
#endif
            context->PublishController(controller.get());
            registered = true;
            auto descriptor = Pod<notepads_markdown_core_snapshot>();
            descriptor.source.document[0] = 0x42;
            descriptor.source.document[15] = 0x33;
            descriptor.source.revision = 1;
            descriptor.profile = NOTEPADS_MARKDOWN_CORE_G3;
            constexpr std::string_view source = "![payload](payload.bmp)";
            Check(controller->SubmitUtf8({reinterpret_cast<uint8_t const *>(source.data()), source.size()}, descriptor, &ticket).status);
            co_await winrt::resume_background();
            Require(Until([&] { context->Arbitrate(); return ticket.Done(); }));
            Check(ticket.Status());
            co_await winrt::resume_foreground(dispatcher);
            pin = controller->PinInstalled();
            co_await winrt::resume_background();
            auto summary = Pod<notepads_markdown_core_model_summary_result>();
            auto error = Pod<notepads_markdown_core_error>();
            Check(notepads_markdown_core_model_summary(pin.Get(), &summary, &error));
            Check(notepads_markdown_core_model_cancel_create(pin.Get(), &sourceFlag, &error));
            context->Publish(sourceFlag);
            sourceRegistered = true;
            memory = ReadingMemory::Create(std::move(pin), sourceFlag, summary.identity);
            auto focused = Run(memory);
            Require(focused.scopedAllocation && focused.constructorFailure && focused.immutablePixels && focused.immutableEncoded);
            RunDrawCacheRegression(memory);
            retiredTasks = RunTaskRetirementRegression(memory);
            policy = NativeImagePolicy::Create();
            policy->Begin(Epoch(*memory));
            auto request = StartImageTestRequest(policy, 1);
            auto decoded = std::make_shared<NativeImageDecodeResult>();
            winrt::Windows::Storage::Streams::InMemoryRandomAccessStream stream;
            winrt::Windows::Storage::Streams::DataWriter writer{stream};
            writer.WriteBytes(Bmp);
            co_await writer.StoreAsync();
            writer.DetachStream();
            writer = nullptr;
            stream.Seek(0);
            co_await DecodeImageAsync(request, memory, stream, 2048, decoded);
            stream = nullptr;
            Require(decoded->Ready() && decoded->pixels->IsFrozen());
            pixels = std::move(decoded->pixels);
            request->CompleteReady(pixels->metadata, pixels->charge);
            request.reset();
            decoded.reset();
            Require(std::equal(Expected.begin(), Expected.end(), pixels->pixels.begin()));
            auto raw = StartImageTestRequest(policy, 2);
            encoded = NativeEncodedImage::Create(memory, raw->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, 16), 16);
            encoded->Append(Expected);
            encoded->Freeze();
            raw.reset();
            auto weakRequest = StartImageTestRequest(policy, 3);
            {
                ReadingMemoryTestHooks hooks;
                hooks.context = &metadata;
                hooks.observe = AllocationObserver::Observe;
                HookScope hook{hooks};
                auto temporary = Pixels(memory, weakRequest);
                temporary->Freeze();
                weakMetadata = temporary;
            }
            weakRequest.reset();
            Require(weakMetadata.expired() && metadata.first);
            {
                ImagePayloadConstruction construction{*memory};
                closedAllocator = ImagePayloadAllocator<uint8_t>{construction.Nonce()};
            }
        }
        catch (...)
        {
            failure = std::current_exception();
        }

        if (sourceRegistered)
            context->Withdraw(sourceFlag);
        if (registered)
            context->WithdrawController(controller.get());
        controller->Stop();
        co_await winrt::resume_background();
        if (sourceFlag)
            context->RecordCleanup(notepads_markdown_core_cancel_destroy(sourceFlag), L"image_payload_cleanup");
        pin.Reset();
        oldMemory = memory;
        memory.reset();
        drain = controller->Drain();
        controller.reset();
        ticket = {};
        if (!failure)
        {
            try
            {
                Require(oldMemory.expired() && lifetime.Snapshot().modelPins == 0);
                Require(retiredTasks && retiredTasks->tasks[0] && retiredTasks->tasks[1]);
                std::array<std::weak_ptr<ImageLoadTask>, 2> weakTasks;
                std::array<void *, 2> taskBlocks;
                for (size_t index = 0; index < weakTasks.size(); ++index)
                {
                    weakTasks[index] = retiredTasks->tasks[index];
                    taskBlocks[index] = retiredTasks->tasks[index]->allocationBlock;
                }
                retiredTasks->ClearFinishedTasks();
                Require(!retiredTasks->tasks[0] && !retiredTasks->tasks[1]);
                for (size_t index = 0; index < weakTasks.size(); ++index)
                {
                    Require(weakTasks[index].expired());
                    auto originalTask = ReadingMemory::InspectAllocation(taskBlocks[index]);
                    ReadingMemoryTestHooks taskHooks;
                    taskHooks.target = taskBlocks[index];
                    taskHooks.failNextFree = true;
                    {
                        HookScope hook{taskHooks};
                        weakTasks[index].reset();
                    }
                    auto failedTask = ReadingMemory::InspectAllocation(taskBlocks[index]);
                    Require(failedTask.lease == originalTask.lease && failedTask.requestedTotal == originalTask.requestedTotal &&
                                failedTask.action.context == originalTask.action.context && oldMemory.expired() &&
                                lifetime.Snapshot().modelPins == 0);
                    ReadingFreeReceipt taskReceipt;
                    {
                        ReadingFreeScope free{taskReceipt};
                        Require(ReadingMemory::TryFree(taskBlocks[index]));
                    }
                    Require(taskReceipt.Succeeded());
                }
                retiredTasks.reset();
                Require(pixels && pixels->IsFrozen() && encoded && encoded->IsFrozen());
                Require(std::equal(Expected.begin(), Expected.end(), pixels->pixels.begin()));
                Require(std::equal(Expected.begin(), Expected.end(), encoded->Bytes().begin()));
                auto retained = policy->Snapshot();
                Require(retained.header.active_requests == 0 && retained.header.decoded_bytes == 16 && retained.header.encoded_bytes == 16);
                Refuses([&] { (void)closedAllocator.allocate(1); });

                auto physical = const_cast<uint8_t *>(pixels->pixels.data());
                auto original = ReadingMemory::InspectAllocation(physical);
                ReadingMemoryTestHooks hooks;
                hooks.target = physical;
                hooks.failNextFree = true;
                {
                    HookScope hook{hooks};
                    pixels.reset();
                }
                auto failed = ReadingMemory::InspectAllocation(physical);
                Require(failed.lease == original.lease && failed.action.context == original.action.context &&
                            policy->Snapshot().header.decoded_bytes == 16 && oldMemory.expired());
                ReadingFreeReceipt retired;
                {
                    ReadingFreeScope free{retired};
                    Require(ReadingMemory::TryFree(physical));
                }
                Require(retired.Succeeded() && policy->Snapshot().header.decoded_bytes == 0);

                auto encodedPhysical = encoded.Get();
                original = ReadingMemory::InspectAllocation(encodedPhysical);
                hooks.target = encodedPhysical;
                hooks.failNextFree = true;
                {
                    HookScope hook{hooks};
                    encoded.Reset();
                }
                failed = ReadingMemory::InspectAllocation(encodedPhysical);
                Require(failed.lease == original.lease && failed.action.context == original.action.context &&
                            policy->Snapshot().header.encoded_bytes == 16 && oldMemory.expired());
                Require(ReadingMemory::TryFree(encodedPhysical));
                Require(policy->Snapshot().header.encoded_bytes == 0);

                // The weak ImagePixels metadata is the sole owner of this
                // implementation block; its value/model have already died.
                original = ReadingMemory::InspectAllocation(metadata.first);
                std::thread weakTail{[weak = std::move(weakMetadata), target = metadata.first]() mutable
                {
                    ReadingMemoryTestHooks weakHooks;
                    weakHooks.target = target;
                    weakHooks.failNextFree = true;
                    HookScope hook{weakHooks};
                    weak.reset();
                }};
                weakTail.join();
                failed = ReadingMemory::InspectAllocation(metadata.first);
                Require(failed.lease == original.lease && failed.requestedTotal == original.requestedTotal);
                Require(ReadingMemory::TryFree(metadata.first));
                Require(policy->Snapshot().header.reservations == 0 && oldMemory.expired());
            }
            catch (...)
            {
                failure = std::current_exception();
            }
        }
        pixels.reset();
        encoded.Reset();
        weakMetadata.reset();
        retiredTasks.reset();
        policy.reset();
        auto released = Until([&] { return lifetime.Snapshot().fullyReleased; });
        results->lastDrain = drain;
        results->lastLifetime = lifetime.Snapshot();
        CoreValidation::CheckResult check{
            L"image_payload_independent_last_owners", !failure && released && drain.coreDrained && drain.driverExited,
            NOTEPADS_MARKDOWN_CORE_OK,
            L"Actual Preview controller/WIC payload; model retires before raster/encoded and weak metadata owners. "
            L"Original failed-free prefixes retain charges, then retire once without a live model/allocation context."};
        if (failure)
        {
            try { std::rethrow_exception(failure); }
            catch (winrt::hresult_error const &error)
            {
                check.status = error.code().value;
                check.detail += L" " + std::wstring{error.message()};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                auto values = reuseDiagnostics->Snapshot();
                if (!values.empty()) check.detail += L"\nFull reuse diagnostics: " + values;
#endif
            }
            catch (...) { check.status = E_FAIL; }
        }
        check.passed = check.passed && drain.primaryStatus == NOTEPADS_MARKDOWN_CORE_OK && drain.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                       drain.primaryHresult == 0 && drain.cleanupHresult == 0 && results->lastLifetime.cleanupStatus == NOTEPADS_MARKDOWN_CORE_OK;
        results->checks.push_back(std::move(check));
    }
}
