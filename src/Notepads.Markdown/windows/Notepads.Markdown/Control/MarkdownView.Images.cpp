// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Control/MarkdownView.h"
#include "Core/NativeThreadpool.h"
#include "Control/MarkdownImageRequest.h"
#include "Resources/Images/NativeImageCoordinator.h"
#include "Resources/Images/NativeImageReuseCache.h"
#include "Resources/Images/NativeImageViewport.h"
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
#include "Diagnostics/NativeStartupTrace.h"
#include "NativeImageFontControlTests.h"
#include "NativeImageReuseControlTests.h"
#endif
#include <dwrite_3.h>
#include <winrt/Windows.UI.Xaml.Controls.Primitives.h>
#include <cmath>
#include <exception>
#include <new>
#include <optional>

namespace winrt::Notepads::Markdown::implementation
{
    using namespace Windows::Foundation;
    using namespace Windows::Storage::Streams;
    namespace Native = ::Notepads::Markdown::Native;

    namespace
    {
        bool SameImageEpoch(notepads_markdown_image_epoch const &left, notepads_markdown_image_epoch const &right) noexcept
        {
            return left.source.revision == right.source.revision && left.source_generation == right.source_generation &&
                   left.provider_generation == right.provider_generation &&
                   std::memcmp(left.source.document, right.source.document, sizeof(left.source.document)) == 0;
        }
        bool SameImageTarget(notepads_markdown_image_target const &left, notepads_markdown_image_target const &right) noexcept
        {
            return left.font_generation == right.font_generation && left.target_generation == right.target_generation &&
                   left.available_width_dip == right.available_width_dip &&
                   left.effective_physical_scale == right.effective_physical_scale;
        }
        notepads_markdown_image_target ImageTarget(Native::NativeSvgImageTarget const &value) noexcept
        {
            return {value.fontGeneration, value.generation, value.maximumWidthDip, value.effectivePhysicalScale};
        }
        struct ImageBackgroundApartment
        {
            ImageBackgroundApartment()
            {
                check_hresult(CoInitializeEx(nullptr, COINIT_MULTITHREADED));
            }
            ~ImageBackgroundApartment()
            {
                CoUninitialize();
            }
        };

        struct ImageAdmission
        {
            Native::ReadingObjectDescriptor const *descriptor{};
            double distance{};
            bool visible{};
        };

        size_t ImageAdmissions(Native::PreviewFrame const &frame, Native::SemanticDescriptors const &descriptors,
                               std::array<ImageAdmission, NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES> &selected)
        {
            if (!frame.navigation)
                return 0;
            auto viewport = Native::ImageViewportWindow::FromCanvas(frame.viewportY, frame.viewportHeight, frame.theme.padding);
            if (!viewport)
                return 0;
            size_t count{};
            auto before = [](ImageAdmission const &left, ImageAdmission const &right)
            {
                if (left.visible != right.visible)
                    return left.visible;
                if (left.distance != right.distance)
                    return left.distance < right.distance;
                return left.descriptor->text.start < right.descriptor->text.start;
            };
            for (auto const &object : descriptors.objects)
            {
                if (object.kind != Native::ReadingObjectKind::Image)
                    continue;
                auto const &line = frame.navigation->Line(frame.navigation->LineAt(object.text.start));
                auto objectTop = line.y;
                auto objectBottom = line.y + line.height;
                if (!viewport->Near(objectTop, objectBottom))
                    continue;
                ImageAdmission value{&object, viewport->Distance(objectTop, objectBottom),
                                     viewport->Visible(objectTop, objectBottom)};
                auto end = selected.begin() + count;
                auto position = std::lower_bound(selected.begin(), end, value, before);
                if (position == selected.end())
                    continue;
                auto index = static_cast<size_t>(position - selected.begin());
                auto nextCount = std::min(count + 1, selected.size());
                for (auto cursor = nextCount - 1; cursor > index; --cursor)
                    selected[cursor] = selected[cursor - 1];
                selected[index] = value;
                count = nextCount;
            }
            return count;
        }
    }

    // One paid block; neither its intrusive references nor its TP context keep
    // ReadingMemory/CoreModelPin alive. The prefix independently owns its lease.
    struct ImageFontWatch
    {
        enum class Command
        {
            Load,
            Stop,
            Refresh,
            Dirty,
            Notify,
            Destroy
        };
        struct Notification
        {
            ImageFontWatch *state{};
            uint64_t identity{};
            Notification() noexcept = default;
            Notification(ImageFontWatch *value, uint64_t id) noexcept : state(value), identity(id)
            {
            }
            Notification(Notification const &other) noexcept;
            Notification(Notification &&other) noexcept
                : state(std::exchange(other.state, nullptr)), identity(other.identity)
            {
            }
            ~Notification();
            void Invoke() const noexcept;
        };
        struct Diagnostics
        {
            winrt::handle event{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
            winrt::handle idleReached{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
            winrt::handle idleProceed{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
            winrt::handle waitReached{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
            winrt::handle waitProceed{CreateEventW(nullptr, TRUE, FALSE, nullptr)};
            std::atomic<bool> pauseIdle{}, pauseWait{};
            std::atomic<uint32_t> waitsCreated{}, waitsClosed{}, workClosed{}, notifications{}, acknowledgements{};
            std::atomic<uint32_t> frees{}, failures{}, activeBodies{}, maximumBodies{}, activeWaits{}, maximumWaits{};
            std::atomic<uint64_t> refreshedGeneration{};
            std::mutex notificationGate;
            std::optional<Notification> heldNotification;
            bool holdNotifications{};
        };

        std::atomic<uint32_t> references{1}; // C; Request always starts with C/W/R/N held.
        std::mutex gate;
        weak_ref<MarkdownView> owner;
        Windows::UI::Core::CoreDispatcher dispatcher{nullptr};
        PTP_WORK work{};
        PTP_WAIT wait{};
        winrt::com_ptr<IDWriteFontCollection3> collection;
        Diagnostics *diagnostics{}; // Test-call lifetime extends through actual free.
        std::shared_ptr<Native::ImageFontControlTestHooks> controlTests;
        bool busy{}, dirty{}, closed{}, destroying{}, desiredLoaded{}, reset{}, refresh{};
        bool automaticDisabled{}, deliveryBlocked{}, pending{}, notificationCompleted{};
        uint32_t notificationReferences{};
        uint64_t desiredGeneration{}, activeGeneration{}, version{}, deliveredVersion{}, deliveredGeneration{};
        uint64_t notificationIdentity{}, notificationGeneration{};
        HRESULT firstFault{S_OK};

        ImageFontWatch(weak_ref<MarkdownView> weak, Windows::UI::Core::CoreDispatcher const &ui,
                       Diagnostics *tests, std::shared_ptr<Native::ImageFontControlTestHooks> controls) noexcept
            : owner(std::move(weak)), dispatcher(ui), diagnostics(tests), controlTests(std::move(controls))
        {
        }

        static void CountMaximum(std::atomic<uint32_t> &maximum, uint32_t value) noexcept
        {
            auto old = maximum.load(std::memory_order_relaxed);
            while (old < value && !maximum.compare_exchange_weak(old, value, std::memory_order_relaxed))
            {
            }
        }
        bool PinCallback(PTP_CALLBACK_INSTANCE instance) noexcept
        {
            if (Native::PinModuleUntilCallbackReturns(instance))
                return true;
            auto error = HRESULT_FROM_WIN32(GetLastError());
            {
                std::lock_guard lock(gate);
                if (SUCCEEDED(firstFault))
                    firstFault = FAILED(error) ? error : E_UNEXPECTED;
            }
            if (diagnostics)
                diagnostics->failures.fetch_add(1, std::memory_order_relaxed);
            // R/W remains the original retained context owner. A failed work
            // entry also keeps busy/W; never fake completion or free its fees.
            return false;
        }
        static ImageFontWatch *Create(std::shared_ptr<Native::ReadingMemory> const &memory, weak_ref<MarkdownView> weak,
                                      Windows::UI::Core::CoreDispatcher const &dispatcher, Diagnostics *tests = nullptr,
                                      std::shared_ptr<Native::ImageFontControlTestHooks> controls = {})
        {
            auto allocation = memory->Allocate(sizeof(ImageFontWatch));
            auto state = new (allocation) ImageFontWatch(std::move(weak), dispatcher, tests, std::move(controls));
            try
            {
                state->work = CreateThreadpoolWork(WorkCallback, state, nullptr);
                if (!state->work)
                    throw hresult_error(HRESULT_FROM_WIN32(GetLastError()));
            }
            catch (...)
            {
                Native::ModuleLifetime module;
                state->~ImageFontWatch();
                Native::ReadingMemory::Free(allocation);
                throw;
            }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            if (state->controlTests)
                state->controlTests->watchesCreated.fetch_add(1, std::memory_order_relaxed);
#endif
            return state;
        }
        void AddRef() noexcept
        {
            references.fetch_add(1, std::memory_order_relaxed);
        }
        void Release() noexcept
        {
            Native::ModuleLifetime module;
            if (references.fetch_sub(1, std::memory_order_acq_rel) != 1)
                return;
            auto tests = diagnostics;
            auto controls = controlTests; // Actual free may be later than the live test's UI teardown.
            this->~ImageFontWatch();
            Native::ReadingFreeReceipt receipt;
            Native::ReadingFreeScope scope(receipt);
            auto freed = Native::ReadingMemory::TryFree(this);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            if (controls)
            {
                if (!freed || !receipt.Succeeded())
                    controls->watchFreeFailures.fetch_add(1, std::memory_order_relaxed);
                controls->watchesFreed.fetch_add(freed ? 1u : 0u, std::memory_order_release);
            }
#endif
            if (tests)
            {
                if (!freed || !receipt.Succeeded())
                    tests->failures.fetch_add(1, std::memory_order_relaxed);
                tests->frees.fetch_add(freed ? 1u : 0u, std::memory_order_release);
            }
        }
        void Request(Command command, uint64_t generation = 0) noexcept
        {
            std::lock_guard lock(gate);
            if (closed)
                return;
            switch (command)
            {
            case Command::Load:
                desiredLoaded = true;
                desiredGeneration = generation;
                reset = refresh = true;
                deliveryBlocked = false;
                break;
            case Command::Stop:
                desiredLoaded = false;
                break;
            case Command::Refresh:
                reset = refresh = true;
                deliveryBlocked = false;
                break;
            case Command::Dirty:
                refresh = true;
                break;
            case Command::Notify:
                break;
            case Command::Destroy:
                destroying = true;
                desiredLoaded = false;
                break;
            }
            dirty = true;
            if (!busy)
            {
                busy = true;
                AddRef(); // W is owned before the no-return-value Submit publishes context.
                SubmitThreadpoolWork(work);
            }
        }
        static void CALLBACK WaitCallback(PTP_CALLBACK_INSTANCE instance, void *context, PTP_WAIT, TP_WAIT_RESULT) noexcept
        {
            auto state = static_cast<ImageFontWatch *>(context); // R spans all real/cancelled callbacks.
            if (!state->PinCallback(instance))
                return;
            auto tests = state->diagnostics;
            if (tests && tests->pauseWait.exchange(false, std::memory_order_acq_rel))
            {
                SetEvent(tests->waitReached.get());
                WaitForSingleObjectEx(tests->waitProceed.get(), INFINITE, FALSE);
            }
            state->Request(Command::Dirty);
        }
        void CloseWait() noexcept
        {
            if (!wait)
            {
                collection = nullptr;
                return;
            }
            SetThreadpoolWait(wait, nullptr, nullptr);
            WaitForThreadpoolWaitCallbacks(wait, TRUE); // Only the separate work body drains.
            CloseThreadpoolWait(std::exchange(wait, nullptr));
            collection = nullptr;
            if (diagnostics)
            {
                diagnostics->waitsClosed.fetch_add(1, std::memory_order_relaxed);
                diagnostics->activeWaits.fetch_sub(1, std::memory_order_relaxed);
            }
            Release(); // R, after actual wait callback return, even if callback was cancelled.
        }
        void CreateWait()
        {
            wait = CreateThreadpoolWait(WaitCallback, this, nullptr);
            auto error = wait ? S_OK : HRESULT_FROM_WIN32(GetLastError());
            if (!wait)
            {
                throw hresult_error(error);
            }
            AddRef(); // R before any arm; callback entry acquires its own loader reference.
            if (diagnostics)
            {
                diagnostics->waitsCreated.fetch_add(1, std::memory_order_relaxed);
                CountMaximum(diagnostics->maximumWaits,
                             diagnostics->activeWaits.fetch_add(1, std::memory_order_relaxed) + 1);
            }
        }
        void RefreshCollection(bool allowAutomatic)
        {
            if (wait)
            {
                SetThreadpoolWait(wait, nullptr, nullptr);
                WaitForThreadpoolWaitCallbacks(wait, TRUE);
            }
            collection = nullptr;
            HANDLE event{};
            if (diagnostics)
            {
                ResetEvent(diagnostics->event.get()); // Only the private fixture event, never a DWrite event.
                event = diagnostics->event.get();
            }
            else
            {
                ImageBackgroundApartment apartment;
                winrt::com_ptr<IDWriteFactory6> factory;
                check_hresult(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory6),
                                                  reinterpret_cast<::IUnknown **>(factory.put())));
                winrt::com_ptr<IDWriteFontCollection> refreshed;
                check_hresult(factory.as<IDWriteFactory>()->GetSystemFontCollection(refreshed.put(), TRUE));
                collection = refreshed.try_as<IDWriteFontCollection3>();
                if (!collection)
                    if (auto recent = factory.try_as<IDWriteFactory7>())
                        recent->GetSystemFontCollection(FALSE, DWRITE_FONT_FAMILY_MODEL_WEIGHT_STRETCH_STYLE,
                                                        collection.put());
                if (collection)
                    event = collection->GetExpirationEvent();
            }
            {
                std::lock_guard lock(gate);
                if (version == UINT64_MAX)
                    throw hresult_error(E_BOUNDS);
                ++version;
                if (version == 1)
                {
                    // First collection is a baseline, not a change event.
                    // Merely attaching the observer must not cancel/open images again.
                    deliveredVersion = version;
                    deliveredGeneration = activeGeneration;
                }
            }
            if (!allowAutomatic || !event || WaitForSingleObjectEx(event, 0, FALSE) != WAIT_TIMEOUT)
            {
                automaticDisabled = true;
                CloseWait();
                return;
            }
            if (!wait)
                CreateWait();
            SetThreadpoolWait(wait, event, nullptr);
        }
        void AddNotificationReference(uint64_t id) noexcept
        {
            std::lock_guard lock(gate);
            if (pending && notificationIdentity == id)
            {
                ++notificationReferences;
                AddRef();
            }
        }
        void DropNotificationReference(uint64_t id) noexcept
        {
            bool wake{};
            {
                std::lock_guard lock(gate);
                if (pending && notificationIdentity == id && --notificationReferences == 0)
                {
                    if (!notificationCompleted)
                    {
                        // A discarded old-loaded delegate must not block a
                        // newer Loaded request that already cleared deliveryBlocked.
                        if (desiredLoaded && notificationGeneration == desiredGeneration)
                            deliveryBlocked = true;
                        if (diagnostics)
                            diagnostics->acknowledgements.fetch_add(1, std::memory_order_relaxed);
                    }
                    pending = false;
                    wake = !closed && !destroying && desiredLoaded && !deliveryBlocked &&
                           (deliveredGeneration != desiredGeneration || deliveredVersion < version);
                }
            }
            if (wake)
                Request(Command::Notify); // N remains held until Request has taken W if needed.
            Release();
        }
        void InvokeNotification(uint64_t id) noexcept
        {
            uint64_t generation{}, observed{};
            {
                std::lock_guard lock(gate);
                if (!pending || notificationIdentity != id || notificationCompleted)
                    return;
                generation = notificationGeneration;
                observed = version;
            }
            bool accepted{};
            if (diagnostics)
            {
                std::lock_guard lock(gate);
                accepted = !destroying && desiredLoaded && desiredGeneration == generation;
                diagnostics->notifications.fetch_add(1, std::memory_order_relaxed);
            }
            else
            {
                try
                {
                    if (auto control = owner.get())
                        accepted = control->AcceptImageFontChange(this, generation, observed);
                }
                catch (...)
                {
                }
            }
            std::lock_guard lock(gate);
            if (pending && notificationIdentity == id && !notificationCompleted)
            {
                notificationCompleted = true; // Exactly-once Ack, later delegate destruction cannot Ack again.
                if (accepted)
                {
                    deliveredGeneration = generation;
                    deliveredVersion = observed;
                }
                else if (desiredLoaded && desiredGeneration == generation && !destroying)
                    deliveryBlocked = true;
                if (diagnostics)
                    diagnostics->acknowledgements.fetch_add(1, std::memory_order_relaxed);
            }
        }
        void PostNotification() noexcept
        {
            uint64_t id{};
            {
                std::lock_guard lock(gate);
                if (closed || destroying || !desiredLoaded || activeGeneration != desiredGeneration || !version ||
                    pending || deliveryBlocked ||
                    (deliveredGeneration == desiredGeneration && deliveredVersion == version))
                    return;
                if (notificationIdentity == UINT64_MAX)
                {
                    deliveryBlocked = true;
                    if (SUCCEEDED(firstFault))
                        firstFault = E_BOUNDS;
                    return;
                }
                id = ++notificationIdentity;
                notificationGeneration = desiredGeneration;
                notificationCompleted = false;
                notificationReferences = 1;
                pending = true;
                AddRef(); // N; fixed slot lives inside the originally paid state.
            }
            Notification notification{this, id};
            try
            {
                if (diagnostics)
                {
                    if (diagnostics->holdNotifications)
                    {
                        std::lock_guard lock(diagnostics->notificationGate);
                        diagnostics->heldNotification.emplace(std::move(notification));
                    }
                    else
                        notification.Invoke();
                    return;
                }
                // The delegate's RAII token also releases N if TryRunAsync
                // rejects/cancels it without invoking the handler.
                dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal,
                                       [notification = std::move(notification)] { notification.Invoke(); });
            }
            catch (...)
            {
                // Token destruction clears pending exactly once and blocks
                // retry until a new explicit loaded/refresh entry.
            }
        }
        static void CALLBACK WorkCallback(PTP_CALLBACK_INSTANCE instance, void *context, PTP_WORK) noexcept
        {
            auto state = static_cast<ImageFontWatch *>(context); // W held until this callback's local tail.
            if (!state->PinCallback(instance))
                return;
            auto tests = state->diagnostics;
            if (tests)
                CountMaximum(tests->maximumBodies, tests->activeBodies.fetch_add(1, std::memory_order_relaxed) + 1);
            PTP_WORK closeWork{}; // Only this callback can claim this local close right.
            for (;;)
            {
                bool destroy{}, loaded{}, reset{}, refresh{};
                uint64_t generation{};
                {
                    std::lock_guard lock(state->gate);
                    state->dirty = false;
                    destroy = state->destroying;
                    loaded = state->desiredLoaded;
                    generation = state->desiredGeneration;
                    reset = std::exchange(state->reset, false);
                    refresh = std::exchange(state->refresh, false);
                }
                try
                {
                    if (destroy || !loaded || state->activeGeneration != generation)
                    {
                        state->CloseWait();
                        state->activeGeneration = loaded && !destroy ? generation : 0;
                        state->automaticDisabled = false;
                    }
                    if (loaded && !destroy)
                    {
                        if (reset)
                            state->automaticDisabled = false;
                        if (refresh)
                            state->RefreshCollection(!state->automaticDisabled);
                        if (tests)
                            tests->refreshedGeneration.store(generation, std::memory_order_release);
                        state->PostNotification();
                    }
                }
                catch (...)
                {
                    auto error = to_hresult();
                    state->automaticDisabled = true;
                    state->CloseWait();
                    std::lock_guard lock(state->gate);
                    if (SUCCEEDED(state->firstFault))
                        state->firstFault = error;
                    if (tests)
                        tests->failures.fetch_add(1, std::memory_order_relaxed);
                }
                {
                    std::lock_guard lock(state->gate);
                    if (state->dirty)
                        continue;
                    state->busy = false; // Request's same G makes this final transfer lossless.
                    if (state->destroying)
                    {
                        state->closed = true;
                        closeWork = std::exchange(state->work, nullptr);
                    }
                    if (tests)
                        tests->activeBodies.fetch_sub(1, std::memory_order_relaxed);
                    break;
                }
            }
            // No state/wait/body reads after relinquishing busy. A successor
            // may already own the body; only this local closeWork may be closed.
            if (tests && tests->pauseIdle.exchange(false, std::memory_order_acq_rel))
            {
                SetEvent(tests->idleReached.get());
                WaitForSingleObjectEx(tests->idleProceed.get(), INFINITE, FALSE);
            }
            if (closeWork)
            {
                CloseThreadpoolWork(closeWork); // OS finishes object destruction after all callback returns.
                if (tests)
                    tests->workClosed.fetch_add(1, std::memory_order_relaxed);
            }
            Native::ModuleLifetime module;
            state->Release(); // W; this callback's loader reference protects its actual machine-code return.
        }
    };

    ImageFontWatch::Notification::Notification(Notification const &other) noexcept
        : state(other.state), identity(other.identity)
    {
        if (state)
            state->AddNotificationReference(identity);
    }
    ImageFontWatch::Notification::~Notification()
    {
        if (state)
            state->DropNotificationReference(identity);
    }
    void ImageFontWatch::Notification::Invoke() const noexcept
    {
        if (state)
            state->InvokeNotification(identity);
    }

    void MarkdownView::LoadImageFonts() noexcept
    {
        if (_imageFontLoadedGeneration == UINT64_MAX)
        {
            _imageFontWatchStartFailed = true;
            return;
        }
        ++_imageFontLoadedGeneration;
        RequestImageFontRefresh();
    }
    void MarkdownView::RequestImageFontRefresh() noexcept
    {
        _imageFontRefreshPending = true;
        _imageFontWatchStartFailed = false;
        EnsureImageFontWatch();
    }
    void MarkdownView::EnsureImageFontWatch() noexcept
    {
        auto descriptors = _readingSource ? _readingSource->Descriptors() : nullptr;
        if (!SvgImagesEnabled() || !_loaded || !descriptors || !_imageProvider ||
            !_imageProviderGeneration || !_imageFontLoadedGeneration || _imageFontWatchStartFailed)
            return;
        if (std::none_of(descriptors->objects.begin(), descriptors->objects.end(),
                         [](auto const &object) { return object.kind == Native::ReadingObjectKind::Image; }))
            return;
        try
        {
            if (!_imageFontWatch)
                _imageFontWatch = ImageFontWatch::Create(_readingSource->memory, get_weak(), _owningDispatcher,
                                                        nullptr, _imageFontControlTests);
            if (_imageFontRefreshPending)
            {
                _imageFontRefreshPending = false;
                _imageFontWatch->Request(ImageFontWatch::Command::Load, _imageFontLoadedGeneration);
            }
        }
        catch (...)
        {
            _imageFontWatchStartFailed = true;
            try
            {
                _renderingError = L"System font observation unavailable.";
            }
            catch (...)
            {
            }
        }
    }
    void MarkdownView::StopImageFonts(bool destroy) noexcept
    {
        if (_imageFontWatch)
        {
            _imageFontWatch->Request(destroy ? ImageFontWatch::Command::Destroy : ImageFontWatch::Command::Stop);
            if (destroy)
                std::exchange(_imageFontWatch, nullptr)->Release(); // C, after guaranteed Stop submission.
        }
    }
    void MarkdownView::ReconcileImageFonts()
    {
        if (!_imageFontReconcilePending || !_images || !_images->bound)
            return;
        auto snapshot = _images->policy->Snapshot();
        uint64_t count{};
        for (uint32_t i = 0; i < snapshot.header.count; ++i)
        {
            auto const &row = snapshot.entries[i];
            if (row.desired_target.target_generation && row.desired_target.font_generation != _imageFontGeneration)
                ++count;
        }
        if (UINT64_MAX - _imageTargetGeneration < count)
            throw hresult_error(E_BOUNDS);
        if (count)
            CaptureImageAnchorBeforeChange();
        for (uint32_t i = 0; i < snapshot.header.count; ++i)
        {
            auto const &row = snapshot.entries[i];
            if (!row.desired_target.target_generation || row.desired_target.font_generation == _imageFontGeneration)
                continue; // Raster completion removes desired; Core retains its own permanent-failure rules.
            auto desired = row.desired_target;
            desired.font_generation = _imageFontGeneration;
            desired.target_generation = ++_imageTargetGeneration;
            _images->policy->Desire(row.object, desired); // No admission/provider callback.
        }
        _imageFontReconcilePending = false;
    }
    void MarkdownView::ReconcilePictureSources()
    {
        const bool dark = Native::PreviewTheme::DiffPanelIsDark(_background, _background);
        if (dark == _pictureDark)
            return;
        const bool previous = std::exchange(_pictureDark, dark);
        auto coordinator = _images;
        auto descriptors = _readingSource ? _readingSource->Descriptors() : nullptr;
        if (!coordinator || !coordinator->bound || !descriptors)
            return;
        bool changed{};
        std::array<std::shared_ptr<Native::ImageLoadTask>, 2> obsoleteTasks;
        for (auto const &object : descriptors->objects)
        {
            if (object.kind != Native::ReadingObjectKind::Image ||
                object.ImageDestination(object.PictureSource(dark)) == object.ImageDestination(object.PictureSource(previous)))
                continue;
            changed = true;
            coordinator->policy->ReplaceSource(object.object);
            coordinator->ClearIntrinsic(object.object);
            if (auto resident = coordinator->FindPixels(object.object))
                *resident = {};
            for (size_t i = 0; i < coordinator->tasks.size(); ++i)
            {
                auto const &task = coordinator->tasks[i];
                if (!task)
                    continue;
                std::lock_guard lock(task->mutex);
                if (task->request && task->request->BelongsTo(coordinator->policy.get()) &&
                    task->request->Key().object == object.object)
                {
                    task->pictureSourceSuperseded = true;
                    obsoleteTasks[i] = task;
                }
            }
        }
        std::exception_ptr failure;
        if (changed)
        {
            try
            {
                QueueImageRefresh();
                FreezeImagePresentation();
            }
            catch (...)
            {
                failure = std::current_exception();
            }
        }
        // Provider cancellation can reenter. Publish the complete change
        // first and leave current source access to the normal callback guards.
        for (auto const &task : obsoleteTasks)
            if (task)
                task->Cancel();
        if (failure)
            std::rethrow_exception(failure);
    }

    bool MarkdownView::AcceptImageFontChange(ImageFontWatch *watch, uint64_t generation, uint64_t version) noexcept
    {
        if (!_loaded || _imageFontWatch != watch || _imageFontLoadedGeneration != generation)
            return false;
        if (version <= _imageFontWatchVersion)
            return true;
        try
        {
            if (_imageFontGeneration == UINT64_MAX || _previewLayoutGeneration == UINT64_MAX)
                throw hresult_error(E_BOUNDS);
            if (_images && _images->bound)
            {
                auto snapshot = _images->policy->Snapshot();
                uint64_t count{};
                for (uint32_t i = 0; i < snapshot.header.count; ++i)
                    count += snapshot.entries[i].desired_target.target_generation ? 1u : 0u;
                if (UINT64_MAX - _imageTargetGeneration < count)
                    throw hresult_error(E_BOUNDS);
            }
            CaptureImageAnchorBeforeChange();
            ++_imageFontGeneration;
            _imageFontWatchVersion = version;
            _imageFontReconcilePending = true;
            _imageFontLayoutBarrier = _previewLayoutGeneration;
            ReconcileImageFonts();
            QueueImageRefresh();
            return true;
        }
        catch (...)
        {
            try
            {
                _renderingError = L"System font image refresh failed: " + hresult_error(to_hresult()).message();
            }
            catch (...)
            {
            }
            return true; // The new font epoch stays withdrawn; existing layout entry replays pending Desire.
        }
    }

    void MarkdownView::SetImageProvider(Markdown::IMarkdownImageProvider const &provider, uint64_t generation)
    {
        CheckUiThread();
        if (!generation || generation <= _imageProviderGeneration)
            throw hresult_invalid_argument();
        CaptureImageAnchorBeforeChange();
        auto anchor = _imageAnchor;
        auto anchorNavigation = _imageAnchorNavigation;
        RetireImages();
        // Invalidate synchronously, before returning to the dispatcher. No old
        // provider completion/frame can publish during the coalescing interval.
        _previewToken->fetch_add(1, std::memory_order_acq_rel);
        if (_previewCancel)
            _previewCancel->Cancel();
        _previewRenderer.reset();
        _presented.frame.reset();
        _surfaceReady = false;
        _imageAnchor = anchor;
        _imageAnchorNavigation = std::move(anchorNavigation);
        _imageProvider = provider;
        _imageProviderGeneration = generation;
        ClearReadingHover();
        if (_readingSource)
        {
            BindImages();
            QueueImageRefresh();
        }
    }
    void MarkdownView::RetireImages(bool keepReuse) noexcept
    {
        try
        {
        if (_owningDispatcher && _owningDispatcher.HasThreadAccess())
        {
            if (_imageRefreshTimer)
                _imageRefreshTimer.Stop();
            if (_imageAnimationTimer)
                _imageAnimationTimer.Stop();
        }
        }
        catch (...) { /* A closed UI dispatcher cannot stop its timers. */ }
        if (_images)
        {
            if (keepReuse && !_images->transitionDescriptors && _images->bound && _readingSource &&
                _images->textGeneration == _readingSource->textGeneration)
            {
                _images->transitionDescriptors = _readingSource->Descriptors();
                _images->transitionDark = _pictureDark;
            }
            _images->Retire(keepReuse, keepReuse && static_cast<bool>(_images->transitionDescriptors));
        }
        if (keepReuse && _images && _images->reuse)
            for (;;)
            {
                // The former source has no current admission rows now. Bound
                // its optional independent cache before a replacement layout
                // exists; any old frame still keeps its real physical charge.
                auto cached = _images->reuse->Inspect();
                uint64_t warm = cached.metadataRequestedBytes;
                size_t oldest = cached.count;
                for (size_t index = 0; index < cached.count; ++index)
                {
                    warm += cached.entries[index].encodedCapacity + cached.entries[index].pixelCapacity;
                    if (oldest == cached.count || cached.entries[index].lastUse < cached.entries[oldest].lastUse)
                        oldest = index;
                }
                if (warm <= Native::ImageOffscreenWarmTargetBytes || oldest == cached.count) break;
                auto const &entry = cached.entries[oldest];
                if (!_images->reuse->Remove(cached.scope, entry.slot, entry.entry)) break;
            }
        _imagePresentation.reset();
        _publishedImagePresentation.reset();
        _imageAnchor.reset();
        _imageAnchorNavigation.reset();
        if (!keepReuse)
            _imageDrawCache.Clear();
        if (_images && _owningDispatcher)
            for (auto const &task : _images->tasks)
                if (task && !task->finished.load(std::memory_order_acquire) &&
                    !task->cancelScheduled.exchange(true, std::memory_order_acq_rel))
                {
                    try { ObserveImageCancellation(_owningDispatcher, task); }
                    catch (...) { task->cancelScheduled.store(false, std::memory_order_release); }
                }
    }
    fire_and_forget MarkdownView::ObserveImageCancellation(Windows::UI::Core::CoreDispatcher dispatcher,
                                                           std::shared_ptr<Native::ImageLoadTask> task)
    {
        // RetireImages has already retired all native state. External host code
        // runs later on the owning UI thread, with no old transaction to resume.
        // Observe rejection instead of permanently suppressing a future attempt.
        try
        {
            auto dispatched = co_await resume_agile(
                dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Low, [task] { task->Cancel(); }));
            if (!dispatched)
                task->cancelScheduled.store(false, std::memory_order_release);
        }
        catch (...)
        {
            task->cancelScheduled.store(false, std::memory_order_release);
        }
    }
    void MarkdownView::BindImages()
    {
        EnsureImageFontWatch();
        if (!_readingSource || !_imageProviderGeneration)
            return;
        if (!_images)
            _images = std::make_shared<Native::ImageCoordinator>();
        _images->SetCapacitySink(make_weak(get_strong().as<IInspectable>()), Dispatcher(),
            [](IInspectable const &target, Native::ImageCoordinator const *expected, uint32_t notices) noexcept
            {
                try
                {
                    auto control = get_self<MarkdownView>(target.as<Markdown::MarkdownView>());
                    if (control->_images.get() != expected)
                        return; // An old coordinator cannot act on its replacement.
                    if (notices & Native::ImageCoordinator::OptionalReclaim)
                    {
                        control->_images->ClearReuse();
                        control->_images->DiscardTransition();
                        if (control->_cacheDocumentOnUnload && !control->_cachedPresentationSuspended &&
                            control->Visibility() == Windows::UI::Xaml::Visibility::Collapsed)
                        {
                            // Normal tab hiding keeps all Ready owners warm. Only a genuine
                            // process-capacity denial retires a hidden presentation. Its host
                            // validates the editor stamp before explicitly resuming it.
                            control->SuspendCachedDocument();
                            return;
                        }
                    }
                    // Only actual physical capacity-return notices drive retry.
                    // Reclaim alone preserves source, current rows and pixels.
                    if ((notices & Native::ImageCoordinator::CapacityReturned) && control->_loaded &&
                        control->_documentAccepted && control->_readingSource && control->_images->bound)
                        control->QueueResourceRendering();
                }
                catch (...) {}
            });
        if (_images->bound && _images->textGeneration == _readingSource->textGeneration &&
            _images->providerGeneration == _imageProviderGeneration)
        {
            ReconcileImageFonts();
            if (_imageFontLayoutBarrier)
            {
                auto snapshot = _images->policy->Snapshot();
                auto sameEpoch = _imagePresentation &&
                                 _imagePresentation->textGeneration == _readingSource->textGeneration &&
                                 SameImageEpoch(_imagePresentation->policy.header.epoch, snapshot.header.epoch);
                // A failed Freeze keeps the old table. The next real layout
                // must retry that table before scheduling a stale-font Frame.
                if (!sameEpoch || _imagePresentation->policy.header.state_generation != snapshot.header.state_generation)
                    FreezeImagePresentation();
            }
            return;
        }
        const bool sameProvider = _images->providerGeneration == _imageProviderGeneration;
        const bool keepCompleted = sameProvider && _images->transitionDescriptors &&
            _images->transitionEpoch.provider_generation == _imageProviderGeneration &&
            std::memcmp(_images->transitionEpoch.source.document, &_acceptedDocument, sizeof(_acceptedDocument)) == 0;
        _images->Retire(sameProvider, keepCompleted);
        if (!_images->policy)
            _images->policy = Native::NativeImagePolicy::Create();
        if (_images->epochGeneration == UINT64_MAX)
            throw hresult_error(E_BOUNDS);
        notepads_markdown_image_epoch epoch{};
        static_assert(sizeof(epoch.source.document) == sizeof(_acceptedDocument));
        std::memcpy(epoch.source.document, &_acceptedDocument, sizeof(_acceptedDocument));
        epoch.source.revision = _acceptedRevision;
        epoch.source_generation = ++_images->epochGeneration;
        epoch.provider_generation = _imageProviderGeneration;
        _images->policy->Begin(epoch);
        _images->EnsureReuse(_readingSource->memory);
        _images->bound = true;
        _images->textGeneration = _readingSource->textGeneration;
        _images->providerGeneration = _imageProviderGeneration;
        if (auto descriptors = _readingSource->Descriptors())
            _images->PrepareIntrinsics(_readingSource->memory, *descriptors);
        ReconcileImageFonts();
        FreezeImagePresentation();
    }
    bool MarkdownView::RestoreCompletedImages(Native::PreviewFrame const &frame,
        std::span<Native::PreviewImageWidth const> provenWidths)
    {
        if (!_images || !_images->bound || !_readingSource || !_images->transitionDescriptors ||
            _images->textGeneration != _readingSource->textGeneration || !frame.descriptors ||
            frame.descriptors->textGeneration != _readingSource->textGeneration)
            return false;
        auto previous = std::move(_images->transitionDescriptors);
        const auto oldDark = _images->transitionDark;
        auto retired = std::move(_images->pixels);
        for (auto &entry : _images->pixels)
            entry = {};
        struct Candidate
        {
            Native::ImageCoordinator::Pixels *pixels{};
            Native::ReadingObjectDescriptor const *before{}, *after{};
            double width{};
            size_t ordinal{}, seen{};
            bool ranked{};
        };
        std::array<Candidate, NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES> candidates;
        size_t count{};
        for (auto &entry : retired)
            if (entry.value)
                candidates[count++].pixels = &entry;
        auto end = candidates.begin() + count;
        std::sort(candidates.begin(), end,
            [](auto const &left, auto const &right) { return left.pixels->object < right.pixels->object; });
        size_t remaining{};
        for (auto const &object : previous->objects)
        {
            if (object.kind != Native::ReadingObjectKind::Image)
                continue;
            auto found = std::lower_bound(candidates.begin(), end, object.object,
                [](auto const &candidate, uint32_t id) { return candidate.pixels->object < id; });
            if (found != end && found->pixels->object == object.object)
            {
                found->before = &object;
                ++remaining;
            }
        }
        auto sameResource = [](auto const &before, bool beforeDark, auto const &after, bool afterDark)
        {
            return before.alt == after.alt && before.title == after.title && before.destination == after.destination &&
                before.lightSource == after.lightSource && before.darkSource == after.darkSource &&
                before.ImageDestination(before.PictureSource(beforeDark)) == after.ImageDestination(after.PictureSource(afterDark));
        };
        auto ranksRemaining = remaining;
        for (auto const &object : previous->objects)
        {
            if (!ranksRemaining)
                break;
            if (object.kind != Native::ReadingObjectKind::Image)
                continue;
            for (auto candidate = candidates.begin(); candidate != end; ++candidate)
            {
                if (!candidate->before || candidate->ranked)
                    continue;
                if (candidate->before == &object)
                {
                    candidate->ranked = true;
                    --ranksRemaining;
                }
                else if (sameResource(*candidate->before, oldDark, object, oldDark))
                    ++candidate->ordinal;
            }
        }
        for (auto const &object : frame.descriptors->objects)
        {
            if (!remaining)
                break;
            if (object.kind != Native::ReadingObjectKind::Image)
                continue;
            for (auto candidate = candidates.begin(); candidate != end; ++candidate)
            {
                auto before = candidate->before;
                // Source positions and parse object IDs shift on unrelated
                // edits. Compare only the bounded set of completed residents,
                // never every prior descriptor for each new image.
                // Occurrence rank includes nonresident duplicates. A scrolled
                // lower image must not be rebound to the first equal image
                // above the viewport when unrelated prose shifts both spans.
                if (before && !candidate->after && sameResource(*before, oldDark, object, _pictureDark) &&
                    candidate->seen++ == candidate->ordinal)
                {
                    candidate->after = &object;
                    --remaining;
                }
            }
        }
        std::sort(candidates.begin(), end, [](auto const &left, auto const &right)
        {
            if (!left.after || !right.after)
                return !!left.after > !!right.after;
            return left.after->object < right.after->object;
        });
        end = std::find_if(candidates.begin(), end, [](auto const &candidate) { return !candidate.after; });
        if (end == candidates.begin())
            return false;
        std::shared_ptr<Native::ImagePresentationSnapshot> next;
        try
        {
            // Allocate the replacement table before changing the policy. Each
            // matched image can add at most one row to the existing snapshot.
            next = _images->Freeze(_readingSource->memory, ImagePresentationAllocation());
            next->entries.reserve(next->entries.size() + static_cast<size_t>(end - candidates.begin()));
            _images->PrepareIntrinsics(_readingSource->memory, *frame.descriptors);
        }
        catch (...) { return false; } // The current frame can still publish without optional reuse.
        if (!provenWidths.empty())
        {
            // Edit preparation proves these widths from the new core layout
            // and the same text/table width functions used by MeasureBlock.
            for (auto const &image : provenWidths)
            {
                auto found = std::lower_bound(candidates.begin(), end, image.object,
                    [](auto const &candidate, uint32_t id) { return candidate.after->object < id; });
                if (found != end && found->after->object == image.object)
                    found->width = image.width;
            }
        }
        else
        {
            // Other containers still take their actual measured cell width.
            for (auto const &block : frame.Blocks())
                for (auto const &text : block.texts)
                    for (auto const &image : text.images)
                    {
                        auto found = std::lower_bound(candidates.begin(), end, image.object,
                            [](auto const &candidate, uint32_t id) { return candidate.after->object < id; });
                        if (found != end && found->after->object == image.object && std::isfinite(text.width) && text.width > 0)
                            found->width = text.width;
                    }
        }
        bool policyChanged{};
        for (auto candidate = candidates.begin(); candidate != end; ++candidate)
        {
            auto const &object = *candidate->after;
            auto resident = candidate->pixels;
            auto target = resident->target;
            if (candidate->width > 0 && target.available_width_dip != candidate->width)
            {
                target.available_width_dip = candidate->width;
                target.target_generation = 0;
            }
            try { target = ImageTarget(CaptureImageTarget(object.object, target)); }
            catch (...) { continue; }
            auto const &metadata = resident->value->metadata;
            auto const svg = metadata.kind == NOTEPADS_MARKDOWN_IMAGE_SVG;
            if (svg)
            {
                if (!resident->encoded || resident->target.font_generation != target.font_generation ||
                    resident->target.available_width_dip != target.available_width_dip ||
                    resident->target.effective_physical_scale != target.effective_physical_scale)
                    continue;
            }
            else
            {
                auto plan = notepads_markdown_image_raster_plan{
                    {NOTEPADS_MARKDOWN_IMAGE_ABI_VERSION, sizeof(notepads_markdown_image_raster_plan)}};
                auto error = notepads_markdown_core_error{
                    NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
                if (notepads_markdown_image_plan_raster(&metadata, &target, &plan, &error) != NOTEPADS_MARKDOWN_CORE_OK ||
                    plan.width != metadata.decoded_width || plan.height != metadata.decoded_height || plan.stride != metadata.stride)
                    continue;
            }
            std::shared_ptr<Native::NativeImageRequest> request;
            try
            {
                auto slot = _images->FindFreePixels();
                if (!slot)
                    break;
                policyChanged = true;
                _images->policy->Desire(object.object, target);
                request = _images->policy->Start(object.object, svg ? NOTEPADS_MARKDOWN_IMAGE_SVG : NOTEPADS_MARKDOWN_IMAGE_RASTER);
                request->CompleteReused(metadata, resident->value->charge);
                // Keep the exact frozen owner and animation clock. The fresh
                // request/epoch supplies all new publication authority.
                *slot = std::move(*resident);
                slot->object = object.object;
                slot->publishedRequest = request->Key().request;
                slot->target = target;
            }
            catch (...)
            {
                // Optional transfer cannot poison normal current admission.
                // No old Pending/failed state or request is carried forward.
                request.reset();
                try { _images->policy->Evict(object.object); }
                catch (...) {}
            }
        }
        if (policyChanged)
            FreezeImagePresentation(std::move(next));
        return policyChanged;
    }
    static bool SameImageGeometry(Native::ImagePresentationSnapshot const &previous,
                                  Native::ImagePresentationSnapshot const &next,
                                  Native::SemanticDescriptors const &descriptors) noexcept
    {
        if (previous.textGeneration != next.textGeneration ||
            !SameImageEpoch(previous.policy.header.epoch, next.policy.header.epoch) ||
            (previous.policy.header.count == NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES) !=
                (next.policy.header.count == NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES))
            return false;
        const auto missingState = previous.policy.header.count == NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES
            ? NOTEPADS_MARKDOWN_IMAGE_LIMITED : NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED;
        for (auto const &object : descriptors.objects)
        {
            if (object.kind != Native::ReadingObjectKind::Image)
                continue;
            const auto before = previous.Find(object.object);
            const auto after = next.Find(object.object);
            const auto beforeState = before ? before->state.state : missingState;
            const auto afterState = after ? after->state.state : missingState;
            const auto beforeMetadata = before && (before->state.status_flags & NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA);
            const auto afterMetadata = after && (after->state.status_flags & NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA);
            const auto beforePixels = before && before->pixels;
            const auto afterPixels = after && after->pixels;
            // Withdrawals/replacements must release the old snapshot and may
            // require constructing fallback visuals absent in the Ready layout.
            if (beforePixels && (!afterPixels || before->pixels != after->pixels))
                return false;
            if (!beforePixels && afterPixels &&
                (object.imageReadinessAffectsText || (!object.imageHasFixedSize && (!beforeMetadata || !afterMetadata))))
                return false;
            if (!object.imageHasFixedSize &&
                (beforeMetadata != afterMetadata || (beforeMetadata &&
                    (before->state.metadata.natural_width_dip != after->state.metadata.natural_width_dip ||
                     before->state.metadata.natural_height_dip != after->state.metadata.natural_height_dip))))
                return false;
            auto pending = [](uint32_t state)
            {
                return state == NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED || state == NOTEPADS_MARKDOWN_IMAGE_PENDING;
            };
            // Pending and Unrequested have one fallback style. State changes
            // between fallback styles still build their own caption and icon.
            if (!afterPixels && beforeState != afterState && !(pending(beforeState) && pending(afterState)))
                return false;
        }
        return true;
    }

    Native::ReadingAllocationFactory MarkdownView::ImagePresentationAllocation() noexcept
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        if (_imageFontControlTests)
            return _imageFontControlTests->BeforeFreeze();
#endif
        return {};
    }

    void MarkdownView::FreezeImagePresentation(std::shared_ptr<Native::ImagePresentationSnapshot> next)
    {
        _images->RememberMetadata(_images->policy->Snapshot());
        if (next)
            _images->PopulatePresentation(*next, *_readingSource->memory);
        else
            next = _images->Freeze(_readingSource->memory, ImagePresentationAllocation());
        if (next)
        {
            auto descriptors = _readingSource->Descriptors();
            const bool sameGeometry = _imagePresentation && descriptors &&
                !_imageFontLayoutBarrier && SameImageGeometry(*_imagePresentation, *next, *descriptors);
            next->geometryGeneration = sameGeometry ? _imagePresentation->geometryGeneration : ++_imageGeometryGeneration;
        }
        _imagePresentation = std::move(next);
    }

    std::shared_ptr<Native::PreviewFrame> MarkdownView::ProjectImageFrame(
        std::shared_ptr<Native::PreviewFrame> const &frame) const
    {
        if (!frame || !frame->images || !_imagePresentation || frame->images == _imagePresentation ||
            frame->images->geometryGeneration != _imagePresentation->geometryGeneration)
            return frame;
        return frame->WithImages(_imagePresentation);
    }

    bool MarkdownView::RefreshImageFrame()
    {
        if (!_surfaceReady || !_presented.frame)
            return false;
        auto current = _presented.frame;
        int64_t row{};
        double y{};
        if (!_previewScroll || !Native::QuantizeViewportY(_previewScroll.VerticalOffset(), _viewportGeometry.scale, row, y) ||
            y != current->viewportY)
            return false;
        auto next = ProjectImageFrame(current);
        if (current == next || !CurrentViewportFrame(*next, _presented.geometry))
            return false;
        for (auto const &block : current->Blocks())
            for (auto const &text : block.texts)
                for (auto const &image : text.images)
                    if (current->ImagePixelsFor(image.object) != next->ImagePixelsFor(image.object) ||
                        current->ImageFrameIndex(image.object) != next->ImageFrameIndex(image.object))
                        return DrawPreview(std::move(next));
        // A status-only change, or pixels outside the measured band, changes
        // no pixel on this surface. The new snapshot still retires any old band.
        _presented.frame = std::move(next);
        CompleteImageAnchor(*_presented.frame);
        RefreshReadingHover();
        NotifyImageFramePublished();
        return true;
    }
    void MarkdownView::ResetImageAnimation() noexcept
    {
        try
        {
            if (_imageAnimationTimer)
                _imageAnimationTimer.Stop();
            if (_images)
            {
                const auto now = std::chrono::steady_clock::now();
                for (auto &slot : _images->pixels)
                    if (slot.value && slot.value->IsAnimated())
                        slot.animationStart = now;
            }
            QueueImageAnimation();
        }
        catch (...) { /* The existing static frame remains presentable. */ }
    }

    bool MarkdownView::ImageAnimationVisible() const
    {
        auto window = Windows::UI::Core::CoreWindow::GetForCurrentThread();
        using Windows::UI::Xaml::Visibility;
        if (!_loaded || this->Visibility() != Visibility::Visible || (window && !window.Visible()))
            return false;
        for (auto parent = _noteParentView.get(); parent; parent = parent->_noteParentView.get())
            if (!parent->_loaded || parent->Visibility() != Visibility::Visible)
                return false;
        return true;
    }

    void MarkdownView::ImageAnimationVisibilityChanged() noexcept
    {
        try
        {
            const auto now = std::chrono::steady_clock::now();
            if (!ImageAnimationVisible())
            {
                if (!_imageAnimationHiddenAt)
                    _imageAnimationHiddenAt = now;
                if (_imageAnimationTimer)
                    _imageAnimationTimer.Stop();
            }
            else
            {
                if (auto hiddenAt = std::exchange(_imageAnimationHiddenAt, std::nullopt); hiddenAt && _images)
                    for (auto &slot : _images->pixels)
                        if (slot.value && slot.value->IsAnimated())
                            slot.animationStart += now - std::max(*hiddenAt, slot.animationStart);
                QueueImageAnimation();
            }
            // A cached descendant keeps its own Visibility when an ancestor
            // collapses. Forward the effective change for pause and resume.
            for (auto const &slot : _noteEmbeds)
                if (auto child = slot.child)
                    winrt::get_self<MarkdownView>(child)->ImageAnimationVisibilityChanged();
        }
        catch (...) { /* Visibility may change during view teardown. */ }
    }

    void MarkdownView::QueueImageAnimation() noexcept
    {
        try
        {
            if (!ImageAnimationVisible() || !_surfaceReady || !_imagePresentation || !_systemColors ||
                (_imageAnimationTimer && _imageAnimationTimer.IsEnabled()))
                return;
            const bool animate = _systemColors.AnimationsEnabled();
            if (std::none_of(_imagePresentation->entries.begin(), _imagePresentation->entries.end(),
                [&](auto const &entry) { return entry.pixels && entry.pixels->IsAnimated() && (animate || entry.frameIndex); }))
                return;
            if (!_imageAnimationTimer)
            {
                _imageAnimationTimer = Windows::UI::Xaml::DispatcherTimer{};
                _imageAnimationTickRevoker = _imageAnimationTimer.Tick(auto_revoke, [weak = get_weak()](auto const &, auto const &)
                {
                    if (auto control = weak.get())
                    {
                        control->_imageAnimationTimer.Stop();
                        control->AdvanceImageAnimation();
                    }
                });
            }
            _imageAnimationTimer.Interval(std::chrono::milliseconds(16));
            _imageAnimationTimer.Start();
        }
        catch (...) { /* Optional playback can retry at the next publication. */ }
    }

    void MarkdownView::AdvanceImageAnimation() noexcept
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiImageAnimation, UiTrace::Phase::UiImageAnimationEnd, _previewLayoutGeneration};
#endif
        try
        {
            auto const current = _imagePresentation;
            auto const frame = _presented.frame;
            if (!ImageAnimationVisible() || !_surfaceReady || !current || !frame || !frame->images || !_images || !_images->bound ||
                !_systemColors || current->geometryGeneration != frame->images->geometryGeneration ||
                !CurrentViewportFrame(*frame, _presented.geometry))
                return;
            struct Change
            {
                Native::ImageCoordinator::Pixels *slot{};
                size_t offset{};
                uint32_t index{};
            };
            std::array<Change, NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES> changes{};
            size_t count{};
            uint32_t nextDelay = UINT32_MAX;
            const bool animate = _systemColors.AnimationsEnabled();
            const auto now = std::chrono::steady_clock::now();
            const auto top = frame->viewportY - _padding;
            const auto bottom = top + _presented.geometry.surfaceHeight;
            auto advance = [&](Native::ImagePresentationEntry const &entry)
            {
                if (!entry.pixels || !entry.pixels->IsAnimated())
                    return;
                auto slot = _images->FindPixels(entry.state.object);
                if (!slot || slot->value != entry.pixels || slot->publishedRequest != entry.state.published_request)
                    return;
                const auto elapsed = std::chrono::duration_cast<std::chrono::milliseconds>(now - slot->animationStart);
                const auto position = animate
                    ? entry.pixels->FrameAt(static_cast<uint64_t>(std::max<int64_t>(0, elapsed.count()))) : Native::ImageFramePosition{};
                if (position.nextDelayMilliseconds)
                    nextDelay = std::min(nextDelay, position.nextDelayMilliseconds);
                if (position.index != entry.frameIndex && count < changes.size())
                    changes[count++] = {slot, static_cast<size_t>(&entry - current->entries.data()), position.index};
            };
            if (animate)
            {
                for (auto const &block : frame->Blocks())
                    for (auto const &text : block.texts)
                        for (auto const &image : text.images)
                        {
                            const double y = block.block.y + text.y + image.y;
                            if (y < bottom && y + image.height > top)
                                if (auto entry = current->Find(image.object))
                                    advance(*entry);
                        }
            }
            else
            {
                for (auto const &entry : current->entries)
                    if (entry.frameIndex)
                        advance(entry);
            }
            if (nextDelay != UINT32_MAX)
            {
                _imageAnimationTimer.Interval(std::chrono::milliseconds(std::max(16u, nextDelay)));
                _imageAnimationTimer.Start();
            }
            if (!count)
                return;
            auto replacement = std::allocate_shared<Native::ImagePresentationSnapshot>(
                Native::ReadingAllocator<Native::ImagePresentationSnapshot>{current->entries.get_allocator()}, *current);
            for (size_t i = 0; i < count; ++i)
            {
                auto const &change = changes[i];
                replacement->entries[change.offset].frameIndex = change.index;
                change.slot->frameIndex = change.index;
            }
            // Same accepted Ready rows and geometry. Replacing only the paint
            // snapshot invalidates the retained scroll band and leaves layout active.
            _imagePresentation = std::move(replacement);
            (void)RefreshImageFrame();
        }
        catch (...) { /* Leave accepted image pixels available on a failed frame. */ }
    }

    void MarkdownView::QueueImageRefresh()
    {
        if (!_loaded || !_readingSource)
            return;
        if (!_imageRefreshTimer)
        {
            _imageRefreshTimer = Windows::UI::Xaml::DispatcherTimer{};
            _imageRefreshTimer.Interval(std::chrono::milliseconds(16));
            _imageRefreshTickRevoker = _imageRefreshTimer.Tick(auto_revoke,
                                                               [weak = get_weak()](auto const &, auto const &)
                                                               {
                                                                   if (auto control = weak.get())
                                                                   {
                                                                       control->_imageRefreshTimer.Stop();
                                                                       control->RefreshImageLayout();
                                                                   }
                                                               });
        }
        // Coalesce completions into one immutable table and layout generation.
        if (!_imageRefreshTimer.IsEnabled())
            _imageRefreshTimer.Start();
    }
    void MarkdownView::RefreshImageLayout()
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiImageRefresh, UiTrace::Phase::UiImageRefreshEnd, _previewLayoutGeneration};
#endif
        if (!_loaded || !_documentAccepted || !_readingSource || !_previewModelReady)
            return;
        try
        {
            CaptureImageAnchorBeforeChange();
            const auto previous = _imagePresentation;
            BindImages();
            if (_images && _images->bound)
                FreezeImagePresentation();
            if (previous && _imagePresentation &&
                previous->geometryGeneration == _imagePresentation->geometryGeneration)
            {
                if (!RefreshImageFrame())
                {
                    if (_previewRenderer)
                        StartPreviewFrame();
                    else if (!HasActivePreviewLayout())
                        StartPreviewLayout();
                }
            }
            else
                StartPreviewLayout();
            PumpImages();
        }
        catch (...)
        {
            _renderingError = L"Image layout update failed: " + hresult_error(to_hresult()).message();
        }
    }
    double MarkdownView::ImageAvailableWidth(uint32_t object, double contentWidth, double previousWidth) const
    {
        // Width is reusable through color changes and viewport scrolling.
        // A new layout refines this estimate without tying SVG fonts to theme.
        if (_presented.frame && _readingSource && _presented.frame->main == _readingSource->Main() &&
            std::abs(_presented.frame->key.width - contentWidth) <= 0.25)
            for (auto const &block : _presented.frame->Blocks())
                for (auto const &text : block.texts)
                    for (auto const &image : text.images)
                        if (image.object == object && std::isfinite(text.width) && text.width > 0)
                            return text.width;
        // Prefetch reaches farther than measured blocks. Keep the last target
        // until that container is measured again instead of alternating widths.
        if (std::isfinite(previousWidth) && previousWidth > 0)
            return previousWidth;
        return contentWidth;
    }
    bool MarkdownView::SvgImagesEnabled() const noexcept
    {
        return true;
    }
    Native::NativeSvgImageTarget MarkdownView::CaptureImageTarget(uint32_t object, notepads_markdown_image_target const &previous)
    {
        auto viewportWidth =
            _previewScroll && _previewScroll.ViewportWidth() > 0 ? _previewScroll.ViewportWidth() : ActualWidth();
        auto width = ImageAvailableWidth(object, std::max(1.0, viewportWidth - 2 * _padding),
                                         previous.available_width_dip);
        auto scale = EffectiveRasterScale() *
                     (_previewScroll ? static_cast<double>(_previewScroll.ZoomFactor()) : 1.0);
        if (!std::isfinite(width) || !std::isfinite(scale) || scale <= 0)
            throw hresult_invalid_argument();
        // Author font demands and actual system-font invalidation share this
        // font epoch; Markdown theme/device generations remain unrelated.
        auto generation = previous.target_generation;
        if (!generation || previous.font_generation != _imageFontGeneration || previous.available_width_dip != width ||
            previous.effective_physical_scale != scale)
        {
            if (_imageTargetGeneration == UINT64_MAX)
                throw hresult_error(E_BOUNDS);
            generation = ++_imageTargetGeneration;
        }
        return {width, scale, generation, SvgImagesEnabled(), _imageFontGeneration};
    }

    void MarkdownView::PumpImages()
    {
        if (_images)
            _images->ClearFinishedTasks();
        auto descriptors = _readingSource ? _readingSource->Descriptors() : nullptr;
        if (_resourceRenderingDeferred || !_loaded || !_imageProvider || !_readingSource || !descriptors || !_images ||
            !_images->bound || _images->textGeneration != _readingSource->textGeneration)
            return;
        try
        {
            if (_imageFontReconcilePending)
            {
                ReconcileImageFonts();
                QueueImageRefresh();
                return;
            }
            // Retire finished owners even while replacement geometry is pending.
            // New admissions require a published current per-cell layout.
            if (!CurrentReadingAction({_presented.frame, _automationSnapshot, _presented.serial}))
                return;
            if (_imageFontLayoutBarrier)
            {
                // An old Frame with only Auto/failed rows cannot admit IO
                // before the font transition's new layout is published.
                if (_presented.frame->key.layout_generation <= _imageFontLayoutBarrier)
                    return;
                _imageFontLayoutBarrier = 0;
            }
            auto coordinator = _images;
            auto source = _readingSource;
            coordinator->PrepareIntrinsics(source->memory, *descriptors);
            auto snapshot = coordinator->policy->Snapshot();
            std::array<ImageAdmission, NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES> selected;
            auto selectedCount = ImageAdmissions(*_presented.frame, *descriptors, selected);
            auto admitted = [&](uint32_t object)
            {
                return std::find_if(selected.begin(), selected.begin() + selectedCount,
                                    [&](auto const &candidate) { return candidate.descriptor->object == object; }) != selected.begin() + selectedCount;
            };
            auto visible = [&](uint32_t object)
            {
                return std::any_of(selected.begin(), selected.begin() + selectedCount,
                                   [&](auto const &candidate) { return candidate.descriptor->object == object && candidate.visible; });
            };
            auto visibleWork = [&]
            {
                for (size_t index = 0; index < selectedCount && selected[index].visible; ++index)
                {
                    auto row = std::find_if(snapshot.entries.begin(), snapshot.entries.begin() + snapshot.header.count,
                        [&](auto const &entry) { return entry.object == selected[index].descriptor->object; });
                    if (row == snapshot.entries.begin() + snapshot.header.count || row->state == NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED ||
                        row->state == NOTEPADS_MARKDOWN_IMAGE_PENDING ||
                        (row->status_flags & (NOTEPADS_MARKDOWN_IMAGE_NEEDS_RENDER | NOTEPADS_MARKDOWN_IMAGE_CAPACITY_WAIT)))
                        return true;
                }
                return false;
            };
            auto withdraw = [&](Native::ImageCoordinator::Pixels &entry)
            {
                coordinator->RememberMetadata(snapshot);
                CaptureImageAnchorBeforeChange();
                coordinator->policy->Evict(entry.object);
                entry = {};
                snapshot = coordinator->policy->Snapshot();
                QueueImageRefresh();
            };
            // One warm target spans current cold rows and independent reuseSvgEncoded.
            // Every record identifies a real immutable owner, never a row's
            // logical binding. Erasure cannot promise physical capacity back.
            for (auto &entry : coordinator->pixels)
                if (entry.HasContent() && admitted(entry.object) && coordinator->useSequence != UINT64_MAX)
                    entry.lastUse = ++coordinator->useSequence;
            struct WarmOwner
            {
                void const *identity{};
                uint64_t bytes{}, lastUse{};
                bool encoded{};
            };
            auto protectedPixels = [&](void const *owner)
            {
                return std::any_of(coordinator->pixels.begin(), coordinator->pixels.end(), [&](auto const &entry)
                    { return entry.value.get() == owner && admitted(entry.object); });
            };
            for (;;)
            {
                std::array<WarmOwner, 2 * (NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES + Native::NativeImageReuseCache::SlotCount)> owners;
                size_t count{};
                uint64_t warm{};
                auto add = [&](void const *identity, uint64_t bytes, uint64_t lastUse, bool encoded)
                {
                    if (!identity || !bytes) return;
                    auto found = std::find_if(owners.begin(), owners.begin() + count, [&](auto const &owner)
                        { return owner.identity == identity && owner.encoded == encoded; });
                    if (found != owners.begin() + count)
                        found->lastUse = (std::max)(found->lastUse, lastUse);
                    else
                    {
                        owners[count++] = {identity, bytes, lastUse, encoded};
                        warm += bytes;
                    }
                };
                for (auto const &entry : coordinator->pixels)
                {
                    if (entry.value && !protectedPixels(entry.value.get()))
                        add(entry.value.get(), entry.value->pixels.capacity(), entry.lastUse, false);
                    if (entry.encoded && !std::any_of(coordinator->pixels.begin(), coordinator->pixels.end(),
                        [&](auto const &other) { return other.encoded.Get() == entry.encoded.Get() && admitted(other.object); }))
                        add(entry.encoded.Get(), entry.encoded->Capacity(), entry.lastUse, true);
                }
                if (coordinator->reuse)
                {
                    auto cached = coordinator->reuse->Inspect();
                    warm += cached.metadataRequestedBytes;
                    for (size_t index = 0; index < cached.count; ++index)
                    {
                        auto const &entry = cached.entries[index];
                        auto lastUse = entry.lastUse;
                        for (auto const &row : coordinator->pixels)
                            if (row.value.get() == entry.pixels) lastUse = (std::max)(lastUse, row.lastUse);
                        if (!protectedPixels(entry.pixels)) add(entry.pixels, entry.pixelCapacity, lastUse, false);
                        // Raster encoded identity is always warm, including
                        // when another current owner protects these pixels.
                        add(entry.encoded, entry.encodedCapacity, lastUse, true);
                    }
                }
                if (warm <= Native::ImageOffscreenWarmTargetBytes || !count) break;
                auto oldest = std::min_element(owners.begin(), owners.begin() + count,
                    [](auto const &left, auto const &right) { return left.lastUse < right.lastUse; });
                auto victim = *oldest;
                // Drop all optional aliases of this physical owner. Other
                // owners released incidentally are re-counted next iteration.
                if (coordinator->reuse)
                {
                    auto cached = coordinator->reuse->Inspect();
                    for (size_t index = 0; index < cached.count; ++index)
                    {
                        auto const &entry = cached.entries[index];
                        if ((victim.encoded && entry.encoded == victim.identity) ||
                            (!victim.encoded && entry.pixels == victim.identity))
                            (void)coordinator->reuse->Remove(cached.scope, entry.slot, entry.entry);
                    }
                }
                for (auto &entry : coordinator->pixels)
                    if (entry.HasContent() && !admitted(entry.object) &&
                        ((victim.encoded && entry.encoded.Get() == victim.identity) ||
                         (!victim.encoded && entry.value.get() == victim.identity))) withdraw(entry);
            }
            for (auto const &task : coordinator->tasks)
            {
                if (!task)
                    continue;
                bool cancel{};
                uint32_t cancelledObject{};
                uint64_t cancelledRequest{};
                {
                    std::lock_guard lock(task->mutex);
                    cancel = task->request && SameImageEpoch(task->request->Key().epoch, snapshot.header.epoch) &&
                             (!admitted(task->request->Key().object) || (!visible(task->request->Key().object) && visibleWork())) &&
                             !task->request->IsCancelled();
                    if (cancel)
                    {
                        cancelledObject = task->request->Key().object;
                        cancelledRequest = task->request->Key().request;
                    }
                }
                if (cancel)
                {
                    auto row = std::find_if(snapshot.entries.begin(), snapshot.entries.begin() + snapshot.header.count,
                                            [&](auto const &entry) { return entry.object == cancelledObject; });
                    if (row != snapshot.entries.begin() + snapshot.header.count && row->pending_request == cancelledRequest)
                    {
                        coordinator->RememberMetadata(snapshot);
                        coordinator->policy->Evict(cancelledObject);
                        snapshot = coordinator->policy->Snapshot();
                        QueueImageRefresh();
                    }
                    task->Cancel();
                    if (!task->cancelScheduled.exchange(true, std::memory_order_acq_rel))
                        ObserveImageCancellation(Dispatcher(), task);
                }
            }
            for (size_t candidateIndex = 0; candidateIndex < selectedCount; ++candidateIndex)
            {
                auto const &object = *selected[candidateIndex].descriptor;
                if (!selected[candidateIndex].visible && visibleWork())
                    continue;
                // A host Open callback may synchronously invalidate geometry.
                if (!CurrentReadingAction({_presented.frame, _automationSnapshot, _presented.serial}))
                    return;
                auto row = std::find_if(snapshot.entries.begin(), snapshot.entries.begin() + snapshot.header.count,
                                        [&](auto const &entry) { return entry.object == object.object; });
                if (snapshot.header.count >= NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES &&
                    row == snapshot.entries.begin() + snapshot.header.count)
                {
                    // Forget only a non-window row whose original operations
                    // and reservations are truly quiescent. Intrinsic geometry
                    // remains in the current-source funded descriptor sidecar.
                    for (uint32_t index = 0; index < snapshot.header.count; ++index)
                    {
                        auto retiredObject = snapshot.entries[index].object;
                        if (admitted(retiredObject))
                            continue;
                        if (auto resident = coordinator->FindPixels(retiredObject))
                            withdraw(*resident);
                        if (coordinator->policy->TryForget(retiredObject))
                        {
                            snapshot = coordinator->policy->Snapshot();
                            break;
                        }
                    }
                    row = std::find_if(snapshot.entries.begin(), snapshot.entries.begin() + snapshot.header.count,
                                       [&](auto const &entry) { return entry.object == object.object; });
                    if (snapshot.header.count >= NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES && row == snapshot.entries.begin() + snapshot.header.count)
                        continue;
                }
                auto imageTarget = CaptureImageTarget(
                    object.object, row == snapshot.entries.begin() + snapshot.header.count ? notepads_markdown_image_target{}
                                                                                           : row->desired_target);
                auto desired = ImageTarget(imageTarget);
                if (row == snapshot.entries.begin() + snapshot.header.count ||
                    !SameImageTarget(row->desired_target, desired))
                {
                    auto previousState = snapshot.header.state_generation;
                    CaptureImageAnchorBeforeChange();
                    coordinator->policy->Desire(object.object, desired);
                    snapshot = coordinator->policy->Snapshot();
                    row = std::find_if(snapshot.entries.begin(), snapshot.entries.begin() + snapshot.header.count,
                                       [&](auto const &entry) { return entry.object == object.object; });
                    // Core may deliberately retain an immutable terminal
                    // failure. A no-op Desire must not drive a layout loop.
                    if (snapshot.header.state_generation != previousState)
                        QueueImageRefresh();
                }
                auto published = coordinator->FindPixels(object.object);
                if (row->status_flags & NOTEPADS_MARKDOWN_IMAGE_CAPACITY_WAIT)
                {
                    // Explicit capacity pressure revokes late insertion as
                    // well as dropping cache-only encoded/decoded owners.
                    coordinator->ClearReuse();
                    for (;;)
                    {
                        Native::ImageCoordinator::Pixels *oldest{};
                        for (auto &entry : coordinator->pixels)
                            if (entry.HasContent() && (!admitted(entry.object) ||
                                (selected[candidateIndex].visible && !visible(entry.object))) &&
                                (!oldest || entry.lastUse < oldest->lastUse))
                                oldest = &entry;
                        if (!oldest)
                            break;
                        withdraw(*oldest);
                    }
                    row = std::find_if(snapshot.entries.begin(), snapshot.entries.begin() + snapshot.header.count,
                                       [&](auto const &entry) { return entry.object == object.object; });
                }
                if (published && published->value && (row->status_flags & NOTEPADS_MARKDOWN_IMAGE_CAPACITY_WAIT) &&
                    !(row->status_flags & NOTEPADS_MARKDOWN_IMAGE_TARGET_SATISFIED))
                {
                    // A full-capacity bucket upgrade may have to withdraw its
                    // own old pixels first. Keep intrinsic geometry; actual
                    // frame retirement wakes the same-target retry later.
                    withdraw(*published);
                    published = nullptr;
                    row = std::find_if(snapshot.entries.begin(), snapshot.entries.begin() + snapshot.header.count,
                                       [&](auto const &entry) { return entry.object == object.object; });
                }
                if (published && published->value && published->value->metadata.kind == NOTEPADS_MARKDOWN_IMAGE_SVG &&
                    row->state != NOTEPADS_MARKDOWN_IMAGE_READY)
                {
                    // Font invalidation withdraws pixels, but preserves the
                    // same-source immutable encoded owner for the new job.
                    auto retired = std::move(published->value);
                    published->publishedRequest = 0;
                }
                auto cancelSuperseded = [&]
                {
                    for (auto const &old : coordinator->tasks)
                    {
                        if (!old)
                            continue;
                        bool cancel{};
                        {
                            std::lock_guard lock(old->mutex);
                            if (old->request && old->request->BelongsTo(coordinator->policy.get()) &&
                                old->request->Key().object == object.object &&
                                SameImageEpoch(old->request->Key().epoch, snapshot.header.epoch) &&
                                !SameImageTarget(old->request->Target(), desired))
                            {
                                old->request->Cancel();
                                cancel = true;
                            }
                        }
                        if (cancel)
                        {
                            Native::CancelSvgImageTask(*old);
                            if (!old->cancelScheduled.exchange(true, std::memory_order_acq_rel))
                                ObserveImageCancellation(Dispatcher(), old);
                        }
                    }
                };
                auto slot = std::find(coordinator->tasks.begin(), coordinator->tasks.end(), nullptr);
                if (!(row->status_flags & NOTEPADS_MARKDOWN_IMAGE_NEEDS_RENDER) || slot == coordinator->tasks.end())
                {
                    cancelSuperseded();
                    continue; // Both actual slots busy: desired only, no queue.
                }
                auto memory = source->memory;
                auto task = Native::ImageLoadTask::Create(memory);
                task->pictureSource = object.PictureSource(_pictureDark);
                *slot = task;
                bool reuseSvgEncoded{}, waitingForSource{};
                auto provider = _imageProvider;
                // Capture the independent capability before any host Open.
                // A synchronous provider/source/pressure change revokes this
                // scope; the old task must never obtain a replacement token.
                auto rasterReuse = coordinator->reuse;
                auto reuseScope = rasterReuse ? rasterReuse->Scope() : 0;
                auto textGeneration = source->textGeneration;
                notepads_markdown_image_key key{};
                try
                {
                    CaptureImageAnchorBeforeChange();
                    if (published && published->encoded)
                    {
                        // This UI-owned source cannot disappear during the
                        // non-callback Start decision. Clone directly to task.
                        task->request = coordinator->policy->Start(object.object, NOTEPADS_MARKDOWN_IMAGE_SVG);
                        task->svgEncoded = published->encoded;
                        reuseSvgEncoded = true;
                    }
                    else
                    {
                        bool liveSource{};
                        for (auto const &old : coordinator->tasks)
                        {
                            if (!old || old == task)
                                continue;
                            std::lock_guard lock(old->mutex);
                            if (!old->request || !old->request->BelongsTo(coordinator->policy.get()) ||
                                old->request->Key().object != object.object ||
                                old->pictureSourceSuperseded || old->pictureSource != task->pictureSource ||
                                !SameImageEpoch(old->request->Key().epoch, snapshot.header.epoch))
                                continue;
                            liveSource = true;
                            if (!old->svgEncodedShareable || !old->svgEncoded || old->svgQuarantined)
                                continue;
                            // Task->Policy->Core is a short callback-free lock
                            // order. The old owner stays pinned through Start
                            // and clone; a failed Start has acquired no clone.
                            task->request = coordinator->policy->Start(object.object, NOTEPADS_MARKDOWN_IMAGE_SVG);
                            task->svgEncoded = old->svgEncoded;
                            reuseSvgEncoded = true;
                            break;
                        }
                        waitingForSource = liveSource && !reuseSvgEncoded;
                        if (!waitingForSource && !reuseSvgEncoded)
                            task->request = coordinator->policy->Start(object.object,
                                (row->state == NOTEPADS_MARKDOWN_IMAGE_READY || (row->status_flags & NOTEPADS_MARKDOWN_IMAGE_HAS_METADATA))
                                    ? row->metadata.kind : NOTEPADS_MARKDOWN_IMAGE_AUTO);
                    }
                    cancelSuperseded();
                    if (waitingForSource)
                    {
                        task->Finish();
                        slot->reset();
                        continue; // No candidate/generation attempt consumed.
                    }
                    key = task->request->Key();
                    // Start changes policy generation before host IO. Publish
                    // its pending table promptly, including encoded reuse, so
                    // reading does not wait for a slow provider to complete.
                    QueueImageRefresh();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                    if (_imageFontControlTests)
                        _imageFontControlTests->InitializeTask(task);
#endif
                    if (reuseSvgEncoded)
                    {
                        ObserveImage(get_weak(), Dispatcher(), task, memory, textGeneration, imageTarget, true);
                        snapshot = coordinator->policy->Snapshot();
                        continue;
                    }
                    auto request = make<implementation::MarkdownImageRequest>(
                        _acceptedDocument, _acceptedRevision, textGeneration, object.object, key.request,
                        _imageProviderGeneration, hstring{object.ImageDestination(task->pictureSource)},
                        hstring{std::wstring_view(object.title)}, hstring{std::wstring_view(object.alt)},
                        Native::ImageMaximumEncodedBytes, Native::ImageMaximumPixelCount);
                    // Host code may synchronously change source/provider/unload.
                    // All operation arguments below were captured before entry.
                    task->open = provider.OpenAsync(request);
                    if (!task->open)
                        throw hresult_error(E_UNEXPECTED);
                    auto opening = task->open;
                    auto cancelled = task->request->IsCancelled();
                    ObserveImage(get_weak(), Dispatcher(), task, memory, textGeneration, imageTarget,
                                 false, std::move(rasterReuse), reuseScope);
                    // Cancellation is best effort and never the completion path.
                    // Use the local operation: the observer can clear task->open.
                    if (cancelled)
                    {
                        try
                        {
                            opening.Cancel();
                        }
                        catch (...)
                        {
                        }
                    }
                }
                catch (...)
                {
                    auto code = to_hresult();
                    bool unobservedOperation{};
                    {
                        std::lock_guard lock(task->mutex);
                        if (task->decodeResult.nativeStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                            task->decodeResult.failure == NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE)
                        {
                            task->decodeResult.failure = NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR;
                            task->decodeResult.error = code;
                        }
                        unobservedOperation = static_cast<bool>(task->open) || static_cast<bool>(task->decode);
                        if (unobservedOperation)
                        {
                            // Coroutine setup failed after host IO began. No
                            // observer can prove completion; cancellation does
                            // not authorize releasing its original physical slot.
                            task->svgQuarantined = true;
                            task->svgKeepAlive = task;
                            task->svgFinishPending = true;
                            task->decodeResult.failure = NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR;
                            task->decodeResult.error = code;
                            task->decodeResult.nativeStatus =
                                code == E_OUTOFMEMORY ? NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY : NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                        }
                    }
                    if (unobservedOperation)
                    {
                        task->Cancel();
                        _renderingError = L"Image observer setup failed; original IO remains retained.";
                        return;
                    }
                    // A strong cloned source belongs to this canonical task
                    // before coroutine allocation/scheduling. That window
                    // needs the same checked last-owner cleanup as the job.
                    Native::RetireUnstartedSvgInput(task);
                    try
                    {
                        if (task->request)
                            task->request->Fail(NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR, code);
                    }
                    catch (...)
                    {
                    }
                    task->NotifyDecodeComplete();
                    task->Finish();
                }
                // Never continue traversing an old descriptor after reentrancy.
                if (!_readingSource || !_images->bound || _readingSource->textGeneration != textGeneration ||
                    _imageProviderGeneration != key.epoch.provider_generation)
                    return;
                snapshot = coordinator->policy->Snapshot();
                QueueImageRefresh();
            }
        }
        catch (Native::NativeImageFailure const &error)
        {
            // Retired work may still occupy both physical slots. It is reaped
            // only on actual completion; ordinary busy admission is not a fault.
            if (error.status != NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT)
                _renderingError = L"Image admission failed (" + to_hstring(error.status) + L").";
        }
        catch (...)
        {
            _renderingError = L"Image provider failed: " + hresult_error(to_hresult()).message();
        }
    }
    fire_and_forget MarkdownView::ObserveImage(weak_ref<MarkdownView> owner,
                                               Windows::UI::Core::CoreDispatcher dispatcher,
                                               std::shared_ptr<Native::ImageLoadTask> task,
                                               std::shared_ptr<Native::ReadingMemory> memory, uint64_t textGeneration,
                                               Native::NativeSvgImageTarget target, bool reuseSvgEncoded,
                                               std::shared_ptr<Native::NativeImageReuseCache> rasterReuse, uint64_t reuseScope)
    {
        Native::ModuleLifetime module;
        auto result = std::shared_ptr<Native::NativeImageDecodeResult>{task, &task->decodeResult};
        IRandomAccessStream stream{nullptr};
        try
        {
            if (reuseSvgEncoded)
            {
                co_await resume_background();
                {
                    ImageBackgroundApartment apartment;
                    Native::RenderSvgImageOnBackground(task, memory, {}, target, result);
                }
                memory.reset();
                task->NotifyDecodeComplete(); // This path opened no provider IO.
            }
            else
            {
                IAsyncOperation<IRandomAccessStream> opening{nullptr};
                {
                    std::lock_guard lock(task->mutex);
                    opening = task->open;
                }
                co_await resume_background();
                stream = co_await resume_agile(opening);
                IAsyncOperation<IRandomAccessStream> retiredOpening{nullptr};
                std::shared_ptr<Native::NativeImageRequest> request;
                {
                    std::lock_guard lock(task->mutex);
                    retiredOpening = std::exchange(task->open, nullptr);
                    request = task->request;
                }
                opening = nullptr;
                retiredOpening = nullptr;
                if (!stream)
                {
                    result->failure = NOTEPADS_MARKDOWN_IMAGE_NOT_PROVIDED;
                    result->error = S_OK;
                }
                else
                {
                    // Transfer ownership before invoking the coroutine: its stream
                    // guard closes once even on a synchronous preflight exception.
                    auto decoding = Native::DecodeImageAsync(request, memory, std::exchange(stream, nullptr), 2048,
                                                             result, task, target, std::move(rasterReuse), reuseScope);
                    {
                        std::lock_guard lock(task->mutex);
                        task->decode = decoding;
                    }
                    // Preserve the decoder's acquisition-apartment preflight,
                    // then keep its real tail join off a provider UI apartment.
                    co_await resume_background();
                    if (request->IsCancelled())
                        decoding.Cancel();
                    co_await resume_agile(decoding);
                    if (!task->WaitForDecodeComplete())
                        co_return;
                    IAsyncAction retiredDecode{nullptr};
                    {
                        std::lock_guard lock(task->mutex);
                        retiredDecode = std::exchange(task->decode, nullptr);
                    }
                    decoding = nullptr;
                    retiredDecode = nullptr;
                }
                request.reset();
            }
        }
        catch (...)
        {
            auto code = to_hresult();
            // A cancelled WinRT action can notify a late observer before its
            // body exits. Do not touch the shared result or retire its slot
            // until the decoder has closed/released its actual IO owners.
            if (!task->WaitForDecodeComplete())
                co_return;
            if (reuseSvgEncoded && result->nativeStatus == NOTEPADS_MARKDOWN_CORE_OK && result->failure == NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE)
            {
                result->failure = code == HRESULT_FROM_WIN32(ERROR_CANCELLED) || code == E_ABORT
                                      ? NOTEPADS_MARKDOWN_IMAGE_CANCELLED
                                      : NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR;
                result->error = code;
            }
            if (reuseSvgEncoded)
                Native::RetireUnstartedSvgInput(task);
            if (stream)
            {
                try
                {
                    stream.Close();
                }
                catch (...)
                {
                }
                stream = nullptr;
            }
            if (result->nativeStatus == NOTEPADS_MARKDOWN_CORE_OK && result->failure == NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE)
            {
                result->pixels.reset();
                result->failure = code == HRESULT_FROM_WIN32(ERROR_CANCELLED) || code == E_ABORT
                                      ? NOTEPADS_MARKDOWN_IMAGE_CANCELLED
                                      : NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR;
                result->error = code;
            }
            // Release completed coroutine owners before releasing the IO permit.
            IAsyncAction retiredDecode{nullptr};
            IAsyncOperation<IRandomAccessStream> retiredOpening{nullptr};
            {
                std::lock_guard lock(task->mutex);
                retiredDecode = std::exchange(task->decode, nullptr);
                retiredOpening = std::exchange(task->open, nullptr);
            }
            memory.reset();
            if (reuseSvgEncoded)
                task->NotifyDecodeComplete();
        }
        // The decoder/IO and catch-local coroutine owners have retired before
        // UI delivery. Only independent immutable payloads cross that boundary;
        // a delayed dispatcher completion must not retain the accepted model.
        memory.reset();
        rasterReuse.reset();
        try
        {
            auto dispatched = co_await resume_agile(
                dispatcher.TryRunAsync(Windows::UI::Core::CoreDispatcherPriority::Normal,
                                       [owner, task, result, textGeneration]
                                       {
                                           if (auto control = owner.get())
                                               control->CompleteImage(task, result, textGeneration);
                                           else
                                               task->Finish();
                                       }));
            if (!dispatched)
                task->Finish();
        }
        catch (...)
        {
            task->Finish();
        }
    }
    void MarkdownView::CompleteImage(std::shared_ptr<Native::ImageLoadTask> const &task,
                                     std::shared_ptr<Native::NativeImageDecodeResult> const &result,
                                     uint64_t textGeneration)
    {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        namespace UiTrace = ::Notepads::Markdown::Native::StartupTrace;
        UiTrace::UiScope uiTrace{UiTrace::Phase::UiImageComplete, UiTrace::Phase::UiImageCompleteEnd, _previewLayoutGeneration};
#endif
        try
        {
            std::shared_ptr<Native::NativeImageRequest> request;
            {
                std::lock_guard lock(task->mutex);
                request = task->request;
            }
            if (_loaded && _readingSource && _images && _images->bound &&
                textGeneration == _readingSource->textGeneration && request && request->IsCurrent() &&
                request->BelongsTo(_images->policy.get()) && !request->IsCancelled())
            {
                auto snapshot = _images->policy->Snapshot();
                auto row = std::find_if(snapshot.entries.begin(), snapshot.entries.begin() + snapshot.header.count,
                                        [&](auto const &entry) { return entry.object == request->Key().object; });
                if (row != snapshot.entries.begin() + snapshot.header.count && row->desired_target.target_generation)
                {
                    // A DPI/width/font change may precede its coalesced Pump.
                    // Withdraw that old candidate before considering Ready.
                    auto current = ImageTarget(CaptureImageTarget(row->object, row->desired_target));
                    if (!SameImageTarget(current, row->desired_target))
                    {
                        CaptureImageAnchorBeforeChange();
                        _images->policy->Desire(row->object, current);
                        snapshot = _images->policy->Snapshot();
                        row = std::find_if(snapshot.entries.begin(), snapshot.entries.begin() + snapshot.header.count,
                                           [&](auto const &entry) { return entry.object == request->Key().object; });
                        QueueImageRefresh();
                    }
                }
                auto current = row != snapshot.entries.begin() + snapshot.header.count &&
                               row->pending_request == request->Key().request &&
                               SameImageTarget(row->desired_target, request->Target()) &&
                               SameImageEpoch(snapshot.header.epoch, request->Key().epoch);
                if (current)
                    CaptureImageAnchorBeforeChange();
                if (current && result->Ready() && SUCCEEDED(result->closeError))
                {
                    if (!result->pixels->IsFrozen() || !Native::ValidImageMetadata(result->pixels->metadata, result->pixels->pixels.size()) ||
                        (result->pixels->metadata.kind == NOTEPADS_MARKDOWN_IMAGE_SVG && !result->encoded))
                        throw Native::NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                    auto slot = _images->FindPixels(request->Key().object);
                    if (!slot)
                        slot = _images->FindFreePixels();
                    if (!slot)
                        throw hresult_error(E_BOUNDS);
                    // Keep the old owners strong before the Core transaction.
                    // Copying encoded may fail; the table and slot still agree.
                    auto retiredPixels = slot->value;
                    auto retiredEncoded = slot->encoded;
                    auto cachePixels = result->pixels;
                    auto svg = result->pixels->metadata.kind == NOTEPADS_MARKDOWN_IMAGE_SVG;
                    if (result->reused)
                        request->CompleteReused(result->pixels->metadata, result->pixels->charge);
                    else
                        request->CompleteReady(result->pixels->metadata, result->pixels->charge);
                    // Same dispatcher, no allocation, await, COM or final owner
                    // release until both halves have the exact new identity.
                    slot->object = request->Key().object;
                    slot->publishedRequest = request->Key().request;
                    slot->value = std::move(result->pixels);
                    slot->target = request->Target();
                    slot->animationStart = std::chrono::steady_clock::now();
                    slot->frameIndex = 0;
                    slot->encoded = svg ? std::move(result->encoded) : Native::NativeEncodedImageHandle{};
                    if (_images->useSequence != UINT64_MAX)
                        slot->lastUse = ++_images->useSequence;
                    // Optional retention occurs after the current transaction.
                    // A saturated AddRef, stale scope or allocation fault must
                    // not turn a successful current publication into an error.
                    try
                    {
                        if (!svg && _images->reuse && result->encoded)
                            (void)_images->reuse->Insert(result->reuseScope,
                                {std::move(cachePixels), std::move(result->encoded)}, slot->lastUse);
                    }
                    catch (...) {}
                }
                else if (current)
                    request->Fail(FAILED(result->closeError) ? NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR :
                                      (result->failure ? result->failure : NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR),
                                  FAILED(result->closeError) ? result->closeError : result->error,
                                  result->detail);
                if (current)
                    QueueImageRefresh();
            }
        }
        catch (...)
        {
            _renderingError = L"Image completion rejected: " + hresult_error(to_hresult()).message();
        }
        task->Finish();
        // No observer or completed coroutine now holds a RequestPermit. Ready
        // pixels hold only their distinct byte lease, so IO admission can resume.
        PumpImages();
    }
    static hstring DescribeImage(Native::ImagePresentationSnapshot const *images, uint32_t object)
    {
        if (images)
        {
            if (auto row = images->Find(object))
            {
                auto const &state = row->state;
                if (state.state != row->VisibleState())
                    return hstring{Native::ImageStateDescription(NOTEPADS_MARKDOWN_IMAGE_FAILED, NOTEPADS_MARKDOWN_IMAGE_RELEASED)};
                if (state.state == NOTEPADS_MARKDOWN_IMAGE_FAILED && state.failure == NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR)
                {
                    if (state.error == HRESULT_FROM_WIN32(ERROR_TIMEOUT))
                        return L"Image not loaded: request timed out";
                    auto status = HRESULT_CODE(state.error);
                    if (HRESULT_FACILITY(state.error) == FACILITY_HTTP && status >= 100 && status <= 599)
                        return L"Image not loaded: HTTP " + to_hstring(status);
                }
                return hstring{Native::ImageStateDescription(state.state, state.failure, state.detail, state.status_flags)};
            }
            if (images->policy.header.count >= NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES)
                return hstring{Native::ImageStateDescription(NOTEPADS_MARKDOWN_IMAGE_LIMITED, NOTEPADS_MARKDOWN_IMAGE_QUOTA)};
        }
        return L"Image not loaded";
    }

    uint32_t MarkdownView::ReadingImageState(uint32_t object) const noexcept
    {
        if (_publishedImagePresentation)
            if (auto row = _publishedImagePresentation->Find(object))
                return row->VisibleState();
        return NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED;
    }

    hstring MarkdownView::ReadingImageDescription(uint32_t object) const
    {
        return DescribeImage(_publishedImagePresentation.get(), object);
    }

    static Native::ReadingObjectDescriptor const *ReadingHoverObjectAt(Native::SemanticDescriptors const &descriptors,
                                                                     uint32_t offset) noexcept
    {
        auto object = std::lower_bound(descriptors.objects.begin(), descriptors.objects.end(), offset,
                                       [](auto const &value, uint32_t start) { return value.text.start < start; });
        return object != descriptors.objects.end() && object->text.start == offset ? &*object : nullptr;
    }

    std::optional<MarkdownView::ReadingHoverTarget> MarkdownView::ReadingHoverAt(Point point, std::optional<size_t> link) const
    {
        if (!ReadingAvailable())
            return {};
        point = ControlToViewport(point);
        if (point.X < 0 || point.Y < 0 || point.X >= _presented.geometry.width || point.Y >= _presented.geometry.height)
            return {};
        auto const &frame = *_presented.frame;
        using Kind = ReadingHoverTarget::Kind;
        const auto x = point.X - _padding;
        const auto y = point.Y - _padding;
        if (auto image = Native::NativeDocumentRenderer::ImageAt(frame, x, y))
        {
            if (!image->ready)
                return ReadingHoverTarget{Kind::Fallback, image->offset};
            if (_readingLinkPoint && !_dragging && frame.descriptors)
            {
                auto object = ReadingHoverObjectAt(*frame.descriptors, image->offset);
                if (object && object->kind == Native::ReadingObjectKind::Image && !object->title.empty())
                    return ReadingHoverTarget{Kind::ImageTitle, image->offset};
            }
        }
        if (auto math = Native::NativeDocumentRenderer::MathFallbackAt(frame, x, y))
            return ReadingHoverTarget{Kind::Fallback, *math};
        if (link && frame.descriptors && *link < frame.descriptors->links.size() && !frame.descriptors->links[*link].title.empty())
            return ReadingHoverTarget{Kind::LinkTitle, *link};
        return {};
    }

    void MarkdownView::TrackReadingHover(Point point, std::optional<size_t> link)
    {
        auto hover = ReadingHoverAt(point, link);
        _readingHoverPoint = point;
        if (hover == _readingHover)
            return;
        ClearReadingHover();
        if (!hover)
            return;
        try
        {
            if (!_readingHoverTimer)
            {
                _readingHoverTimer = Windows::UI::Xaml::DispatcherTimer();
                _readingHoverTimer.Interval(std::chrono::milliseconds(500));
                _readingHoverTickRevoker = _readingHoverTimer.Tick(auto_revoke, [weak = get_weak()](auto const &, auto const &)
                {
                    if (auto self = weak.get())
                        self->ShowReadingHover();
                });
            }
            _readingHover = hover;
            _readingHoverTimer.Start();
        }
        catch (...)
        {
            ClearReadingHover();
        }
    }

    void MarkdownView::RefreshReadingHover()
    {
        const auto previous = _readingHover;
        if (!previous)
            return;
        // An image can publish its pixels while the mouse stays over it.
        TrackReadingHover(_readingHoverPoint, ReadingLinkAtPointer(_presented));
        if (_readingHover == previous && _readingHoverToolTip)
            ShowReadingHover();
    }

    void MarkdownView::ShowReadingHover()
    {
        try
        {
            _readingHoverTimer.Stop();
            if (!_readingHover || ReadingHoverAt(_readingHoverPoint, ReadingLinkAtPointer(_presented)) != _readingHover)
            {
                ClearReadingHover();
                return;
            }
            auto frame = _presented.frame;
            auto const &descriptors = *frame->descriptors;
            const auto target = *_readingHover;
            std::wstring text;
            auto appendExcerpt = [&](std::wstring_view value)
            {
                constexpr size_t limit = 256;
                size_t length = std::min(limit, value.size());
                if (length < value.size() && value[length - 1] >= 0xd800 && value[length - 1] <= 0xdbff)
                    --length;
                text.append(value.substr(0, length));
                if (length < value.size())
                    text.push_back(L'\x2026');
            };
            if (target.kind == ReadingHoverTarget::Kind::LinkTitle)
            {
                appendExcerpt(descriptors.links[target.index].title);
            }
            else
            {
                const auto offset = static_cast<uint32_t>(target.index);
                auto object = ReadingHoverObjectAt(descriptors, offset);
                if (!object)
                    return;
                const bool image = object->kind == Native::ReadingObjectKind::Image;
                if (target.kind == ReadingHoverTarget::Kind::ImageTitle)
                    appendExcerpt(object->title);
                else if (image)
                {
                    text = DescribeImage(frame->images.get(), object->object);
                    auto appendLocation = [&](std::wstring_view label, std::wstring_view destination)
                    {
                        if (destination.empty())
                            return;
                        text.append(L"\n").append(label).append(L"\n");
                        appendExcerpt(destination);
                    };
                    appendLocation(L"Image source", object->destination);
                    if (auto link = descriptors.LinkAt(offset); link && descriptors.links[*link].destination != object->destination)
                        appendLocation(L"Link target", descriptors.links[*link].destination);
                }
                else if (object->kind == Native::ReadingObjectKind::Math)
                    text = ReadingMathDescription(object->object, object->syntax);
                else
                    return;
            }
            using namespace Windows::UI::Xaml;
            const hstring details{text};
            if (_readingHoverToolTip)
            {
                auto content = _readingHoverToolTip.Content().as<Controls::TextBlock>();
                if (content.Text() != details)
                    content.Text(details);
                return;
            }
            auto owner = _surfaceTarget;
            if (!owner || Controls::ToolTipService::GetToolTip(owner))
                return;
            Controls::TextBlock content;
            content.Text(details);
            content.MaxWidth(360);
            content.TextWrapping(TextWrapping::Wrap);
            _readingHoverToolTip = Controls::ToolTip();
            _readingHoverToolTip.Content(content);
            const auto point = TransformToVisual(owner).TransformPoint(_readingHoverPoint);
            _readingHoverToolTip.PlacementTarget(owner);
            using Windows::Foundation::IReference;
            using Windows::Foundation::Rect;
            _readingHoverToolTip.PlacementRect(box_value(Rect{point.X, point.Y, 1.0f, 16.0f}).as<IReference<Rect>>());
            _readingHoverToolTip.Placement(Controls::Primitives::PlacementMode::Bottom);
            if (_viewportRoot)
                _readingHoverToolTip.XamlRoot(_viewportRoot);
            _readingHoverToolTip.IsHitTestVisible(false);
            Controls::ToolTipService::SetToolTip(owner, _readingHoverToolTip);
            _readingHoverToolTip.IsOpen(true);
        }
        catch (...)
        {
            ClearReadingHover();
        }
    }

    void MarkdownView::ClearReadingHover() noexcept
    {
        if (!_readingHover)
            return;
        _readingHover.reset();
        auto tooltip = std::exchange(_readingHoverToolTip, nullptr);
        try
        {
            if (_readingHoverTimer)
                _readingHoverTimer.Stop();
            if (tooltip)
            {
                using Windows::UI::Xaml::Controls::ToolTipService;
                auto owner = tooltip.PlacementTarget();
                tooltip.IsOpen(false);
                if (owner && ToolTipService::GetToolTip(owner) == tooltip)
                    ToolTipService::SetToolTip(owner, nullptr);
                tooltip.PlacementTarget(nullptr);
            }
        }
        catch (...)
        {
        }
    }
    void MarkdownView::CaptureImageAnchorBeforeChange()
    {
        if (_viewportToRestore || _imageAnchor || _imageUserScrollY || !_loaded || !_documentAccepted || !_surfaceReady ||
            !_presented.frame || !_automationSnapshot || !_readingSource || !_previewScroll)
            return;
        // Width/theme/resource invalidation can already have withdrawn public
        // navigation. Only capture from the immutable, still-owned source;
        // this does not make the old frame available for drawing or UIA.
        auto frame = _presented.frame;
        auto snapshot = _automationSnapshot;
        if (!frame->main || !frame->navigation || frame->main != _readingSource->Main() ||
            snapshot->main != frame->main || frame->main->SourceGeneration() != _previewSourceGeneration ||
            snapshot->token != _readingSource->textGeneration || frame->main->TextGeneration() != snapshot->token ||
            snapshot->document != _acceptedDocument || snapshot->revision != _acceptedRevision ||
            _displayedDocument != _acceptedDocument || _displayedRevision != _acceptedRevision)
            return;
        int64_t pixelRow{};
        double presentY{};
        if (!Native::QuantizeViewportY(_previewScroll.VerticalOffset(), _presented.geometry.scale, pixelRow, presentY) ||
            presentY != frame->viewportY)
            return; // A newer user scroll target wins over resource reflow.
        auto const &nav = *frame->navigation;
        if (!nav.LineCount())
            return;
        auto top = presentY - frame->theme.padding;
        std::optional<size_t> covering, following;
        size_t last = 0;
        auto earlier = [&](size_t a, size_t b)
        {
            auto const &left = nav.Line(a);
            auto const &right = nav.Line(b);
            return left.y < right.y || (left.y == right.y && left.x < right.x);
        };
        for (size_t i = 0; i < nav.LineCount(); ++i)
        {
            auto const &candidate = nav.Line(i);
            if (earlier(last, i))
                last = i;
            if (candidate.y <= top && candidate.y + candidate.height > top)
            {
                if (!covering || earlier(i, *covering))
                    covering = i;
            }
            else if (candidate.y > top && (!following || earlier(i, *following)))
                following = i;
        }
        auto line = covering.value_or(following.value_or(last));
        auto const &current = nav.Line(line);
        const bool atomic = frame->descriptors &&
            std::any_of(frame->descriptors->objects.begin(), frame->descriptors->objects.end(),
                        [&](auto const &object)
                        {
                            return object.kind != Native::ReadingObjectKind::Task &&
                                   object.text.start == current.text.start && object.text.end == current.text.end;
                        });
        const auto maximum = _previewScroll.ScrollableHeight();
        const auto tolerance = std::min(0.5 / std::max(1.0, _presented.geometry.scale), maximum * 0.5);
        _imageAnchor = ImageAnchor{current.text.start, presentY - current.y, snapshot->token, _imageScrollGeneration,
                                   _previewSourceGeneration, snapshot->document, snapshot->revision, frame->main,
                                   frame->theme.padding, atomic, presentY == 0,
                                   maximum > 0 && presentY >= maximum - tolerance};
        _imageAnchorNavigation = frame->navigation;
    }
    bool MarkdownView::CurrentImageAnchor(ImageAnchor const &anchor) const noexcept
    {
        return _loaded && _documentAccepted && _readingSource && anchor.main && anchor.main == _readingSource->Main() &&
               anchor.sourceGeneration == _previewSourceGeneration &&
               anchor.main->SourceGeneration() == anchor.sourceGeneration &&
               anchor.textGeneration == _readingSource->textGeneration &&
               anchor.main->TextGeneration() == anchor.textGeneration &&
               anchor.document == _acceptedDocument && anchor.revision == _acceptedRevision &&
               anchor.document == _displayedDocument && anchor.revision == _displayedRevision &&
               anchor.scrollGeneration == _imageScrollGeneration;
    }
    bool MarkdownView::ImageAnchorPending(Native::PreviewFrame const &frame) const noexcept
    {
        if (!frame.descriptors)
            return true;
        uint32_t mathOrdinal{}, diagramOrdinal{};
        uint32_t imageOrdinal{};
        for (auto const &object : frame.descriptors->objects)
        {
            if (object.kind == Native::ReadingObjectKind::Math && frame.math.enabled)
            {
                if (mathOrdinal++ >= frame.mathStates.size())
                    continue;
                const auto state = frame.MathState(object.object);
                if (state == Native::NativeMathState::None || state == Native::NativeMathState::Pending)
                    return true;
            }
            else if (object.kind == Native::ReadingObjectKind::Diagram && frame.diagram.enabled)
            {
                if (diagramOrdinal++ >= frame.diagramStates.size())
                    continue;
                const auto state = frame.DiagramState(object.object);
                if (state == Native::NativeDiagramState::None || state == Native::NativeDiagramState::Pending)
                    return true;
            }
            else if (object.kind == Native::ReadingObjectKind::Image)
            {
                if (imageOrdinal++ >= NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES || !_imageProvider)
                    continue; // The existing terminal fallback cannot admit IO.
                const auto entry = frame.images ? frame.images->Find(object.object) : nullptr;
                if (!entry || entry->state.state == NOTEPADS_MARKDOWN_IMAGE_UNREQUESTED || entry->state.state == NOTEPADS_MARKDOWN_IMAGE_PENDING ||
                    entry->state.pending_request ||
                    (entry->state.state == NOTEPADS_MARKDOWN_IMAGE_READY && (entry->state.status_flags & NOTEPADS_MARKDOWN_IMAGE_NEEDS_RENDER)))
                    return true;
            }
        }
        return false;
    }
    double MarkdownView::ImageAnchorTarget(Native::PreviewFrame const &frame, ImageAnchor const &anchor,
                                           double maximum) const
    {
        if (anchor.atTop)
            return 0;
        if (anchor.atBottom)
            return maximum;
        auto const &line = frame.navigation->Line(frame.navigation->LineAt(static_cast<uint32_t>(anchor.offset)));
        auto within = anchor.lineOffset - anchor.padding;
        // A tall atomic object has one logical unit. Preserve its interior
        // displacement, then clamp only if its final geometry became shorter.
        // A Pending label must never consume that original displacement.
        if (anchor.atomic && within >= 0 && !ImageAnchorPending(frame))
            within = std::min(within, std::max(0.0, line.height - 1.0 / _viewportGeometry.scale));
        return std::clamp(line.y + _padding + within, 0.0, maximum);
    }
    void MarkdownView::CompleteImageAnchor(Native::PreviewFrame const &frame)
    {
        if (_imageAnchorExtentPending && CurrentViewportFrame(frame, _viewportGeometry))
        {
            int64_t row{};
            double current{};
            if (Native::QuantizeViewportY(_previewScroll.VerticalOffset(), _viewportGeometry.scale, row, current) &&
                current == frame.viewportY)
                _imageAnchorExtentPending = false;
        }
        if (_imageRestoringY && CurrentViewportFrame(frame, _viewportGeometry))
        {
            int64_t targetRow{}, currentRow{};
            double target{}, current{};
            if (Native::QuantizeViewportY(*_imageRestoringY, _viewportGeometry.scale, targetRow, target) &&
                Native::QuantizeViewportY(_previewScroll.VerticalOffset(), _viewportGeometry.scale, currentRow, current) &&
                targetRow == currentRow && target == frame.viewportY)
                _imageRestoringY.reset();
        }
        if (_imageUserScrollY && !_wheelScroll.Active() && CurrentViewportFrame(frame, _viewportGeometry))
        {
            int64_t targetRow{}, currentRow{};
            double target{}, current{};
            if (Native::QuantizeViewportY(*_imageUserScrollY, _viewportGeometry.scale, targetRow, target) &&
                Native::QuantizeViewportY(_previewScroll.VerticalOffset(), _viewportGeometry.scale, currentRow, current) &&
                targetRow == currentRow && target == frame.viewportY)
                _imageUserScrollY.reset();
        }
        if (!_imageAnchor || !CurrentImageAnchor(*_imageAnchor) ||
            !CurrentViewportFrame(frame, _viewportGeometry) || ImageAnchorPending(frame))
            return;
        int64_t targetRow{}, currentRow{};
        double target{}, current{};
        if (Native::QuantizeViewportY(ImageAnchorTarget(frame, *_imageAnchor, _previewScroll.ScrollableHeight()),
                                     _viewportGeometry.scale, targetRow, target) &&
            Native::QuantizeViewportY(_previewScroll.VerticalOffset(), _viewportGeometry.scale, currentRow, current) &&
            targetRow == currentRow && target == frame.viewportY)
        {
            _imageAnchor.reset();
            _imageAnchorNavigation.reset();
            _imageRestoringY.reset();
        }
    }
    void MarkdownView::UpdateImageAnchorForScroll(double y)
    {
        int64_t pixelRow{};
        double presentY{};
        if (!_presented.frame || !Native::QuantizeViewportY(y, _presented.geometry.scale, pixelRow, presentY) ||
            presentY != _presented.frame->viewportY)
            CancelViewportRestore(y);
    }
    bool MarkdownView::CurrentImageFrame(Native::PreviewFrame const &frame) const noexcept
    {
        try
        {
            if (!_imageProviderGeneration)
                return !frame.images;
            if (!frame.images || !_images || !_images->bound || !_readingSource)
                return false;
            const auto generation = _images->policy->StateGeneration();
            if (frame.images->policy.header.state_generation != generation)
                return false;
            auto const &epoch = frame.images->policy.header.epoch;
            for (auto const &entry : frame.images->entries)
                if (entry.state.state == NOTEPADS_MARKDOWN_IMAGE_READY && entry.state.metadata.kind == NOTEPADS_MARKDOWN_IMAGE_SVG &&
                    entry.state.published_target.font_generation != _imageFontGeneration)
                    return false;
            return frame.images->textGeneration == _readingSource->textGeneration &&
                   epoch.source_generation == _images->epochGeneration &&
                   epoch.provider_generation == _imageProviderGeneration && epoch.source.revision == _acceptedRevision &&
                   std::memcmp(epoch.source.document, &_acceptedDocument, sizeof(_acceptedDocument)) == 0;
        }
        catch (...)
        {
            // A failed policy snapshot cannot qualify this frame for publication or input.
            return false;
        }
    }
    bool MarkdownView::RestoreCachedViewport(Native::PreviewFrame const &frame)
    {
        if (!_viewportToRestore)
            return false;
        auto saved = *_viewportToRestore;
        if (saved.offset > frame.main->Length())
        {
            _viewportToRestore.reset();
            return false;
        }
        const auto extentWidth = frame.key.width + 2 * _padding;
        const auto extentHeight = std::max(ActualHeight(), frame.totalHeight + 2 * _padding);
        if (_previewExtent.Width() != extentWidth || _previewExtent.Height() != extentHeight)
        {
            const auto intent = CaptureScrollIntent();
            _imageAnchorExtentPending = true;
            struct ExtentUpdate
            {
                bool &updating;
                bool previous;
                ~ExtentUpdate() noexcept
                {
                    updating = previous;
                }
            } extentUpdate{_imageAnchorUpdatingExtent, std::exchange(_imageAnchorUpdatingExtent, true)};
            _previewExtent.Width(extentWidth);
            _previewExtent.Height(extentHeight);
            _previewExtent.UpdateLayout();
            if (!CurrentScrollIntent(intent) || !CurrentViewportFrame(frame, _viewportGeometry) || !_viewportToRestore)
                return true;
        }
        const auto maximum = _previewScroll.ScrollableHeight();
        auto within = saved.withinLine;
        if (saved.atomic && within >= _padding && !ImageAnchorPending(frame))
        {
            auto const &line = frame.navigation->Line(frame.navigation->LineAt(saved.offset));
            within = _padding + std::min(within - _padding,
                                        std::max(0.0, line.height - 1.0 / _viewportGeometry.scale));
        }
        const auto anchorY = saved.y == 0 ? 0.0 : frame.navigation->RevealY(saved.offset) + within;
        const auto target = saved.atBottom ? maximum : std::clamp(anchorY, 0.0, maximum);
        int64_t row{};
        double presentY{};
        if (!Native::QuantizeViewportY(target, _viewportGeometry.scale, row, presentY))
            throw hresult_invalid_argument();

        // A short placeholder layout must publish to start loading resources. Keep
        // an anchor clipped by that layout, or the bottom edge, for later reflow.
        const auto resolved = !saved.atBottom && anchorY <= maximum && !ImageAnchorPending(frame);
        if (frame.viewportY == presentY &&
            Native::QuantizeViewportY(_previewScroll.VerticalOffset(), _viewportGeometry.scale, row, presentY) &&
            frame.viewportY == presentY)
        {
            if (resolved)
                _viewportToRestore.reset();
            return false;
        }

        const auto intent = BeginImmediateScroll();
        if (!CurrentScrollIntent(intent) || !CurrentViewportFrame(frame, _viewportGeometry) || !_viewportToRestore)
            return true;
        _imageRestoringY = target;
        if (!_previewScroll.ChangeView(nullptr, target, nullptr, true))
        {
            _imageRestoringY.reset();
            if (resolved)
                _viewportToRestore.reset();
            return false;
        }
        return true;
    }

    bool MarkdownView::RestoreImageAnchor(std::shared_ptr<Native::PreviewFrame> const &frame)
    {
        if (!_imageAnchor || !_previewScroll || !frame->main)
            return false;
        auto anchor = *_imageAnchor;
        if (!CurrentImageAnchor(anchor) || frame->main != anchor.main)
        {
            _imageAnchor.reset();
            _imageAnchorNavigation.reset();
            return false;
        }
        auto intent = BeginImmediateScroll();
        auto current = [&]
        {
            return CurrentScrollIntent(intent) && CurrentImageAnchor(anchor) && _imageAnchor &&
                   _imageAnchor->main == anchor.main && _imageAnchor->offset == anchor.offset &&
                   _imageAnchor->scrollGeneration == anchor.scrollGeneration &&
                   CurrentViewportFrame(*frame, _viewportGeometry);
        };
        if (!current())
            return true;
        const auto width = frame->key.width + 2 * _padding;
        const auto height = std::max(ActualHeight(), frame->totalHeight + 2 * _padding);
        const auto expectedMaximum = std::max(0.0, height - _previewScroll.ViewportHeight());
        if (_previewExtent.Width() != width || _previewExtent.Height() != height ||
            std::abs(_previewScroll.ScrollableHeight() - expectedMaximum) * _viewportGeometry.scale > 1.0)
        {
            _imageAnchorExtentPending = true;
            struct ExtentUpdate
            {
                bool &updating;
                bool previous;
                ~ExtentUpdate() noexcept
                {
                    updating = previous;
                }
            } extentUpdate{_imageAnchorUpdatingExtent, std::exchange(_imageAnchorUpdatingExtent, true)};
            _previewExtent.Width(width);
            _previewExtent.Height(height);
            _previewExtent.UpdateLayout();
            if (!current())
                return true;
        }
        // The old maximum cannot clamp the new content position. XAML's
        // measured new extent, including its scrollbar, owns this clamp.
        const auto y = ImageAnchorTarget(*frame, anchor, _previewScroll.ScrollableHeight());
        int64_t targetRow{}, currentRow{};
        double target{}, present{};
        if (!Native::QuantizeViewportY(y, _viewportGeometry.scale, targetRow, target) ||
            !Native::QuantizeViewportY(_previewScroll.VerticalOffset(), _viewportGeometry.scale, currentRow, present))
            throw hresult_invalid_argument();
        if (frame->viewportY == target && currentRow == targetRow)
            return false;
        if (!current())
            return true; // A reentrant host/layout event superseded this candidate.
        if (currentRow != targetRow)
        {
            _imageRestoringY = y;
            if (!_previewScroll.ChangeView(nullptr, y, nullptr, true))
            {
                CancelViewportRestore();
                _renderingError = L"The image viewport anchor could not be restored.";
            }
        }
        // Keep the unclamped anchor until a successful terminal-geometry
        // publication confirms the target. Pending frames still publish and
        // start resource binding through the ordinary scheduler.
        StartPreviewFrame();
        return true;
    }
}

namespace winrt::Notepads::Markdown::implementation
{
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
    void ImageFontControlTestAccess::Attach(
        Markdown::MarkdownView const &view, std::shared_ptr<Native::ImageFontControlTestHooks> hooks)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        if (control->_imageFontWatch || control->_imageFontControlTests)
            throw hresult_illegal_method_call();
        control->_imageFontControlTests = std::move(hooks);
    }

    bool ImageFontControlTestAccess::CheckRasterScaleTargets(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        auto hooks = control->_imageFontControlTests;
        if (!hooks || !control->_readingSource || !control->CaptureDiagramKey().textGeneration)
            return false;
        struct RestoreInputs
        {
            Native::ImageFontControlTestHooks &hooks;
            std::optional<std::array<double, 2>> previous;
            ~RestoreInputs() { hooks.rasterScaleInputs = previous; }
        } restore{*hooks, hooks->rasterScaleInputs};
        const auto missing = std::numeric_limits<double>::quiet_NaN();
        const auto infinite = std::numeric_limits<double>::infinity();
        const std::array<std::array<double, 3>, 6> samples{{
            {2.0, 1.0, 2.0}, {1.25, 2.0, 1.25}, {missing, 1.5, 1.5},
            {0.0, 2.0, 2.0}, {-1.0, missing, 1.0}, {infinite, 0.0, 1.0}}};
        notepads_markdown_image_target previous{};
        for (auto const &sample : samples)
        {
            hooks->rasterScaleInputs = std::array<double, 2>{sample[0], sample[1]};
            auto image = control->CaptureImageTarget(0, previous);
            auto diagram = control->CaptureDiagramKey();
            auto imageScale = sample[2] * static_cast<double>(control->_previewScroll.ZoomFactor());
            if (control->EffectiveRasterScale() != sample[2] || image.effectivePhysicalScale != imageScale ||
                diagram.dpi != 96.0 * sample[2] || !image.generation ||
                (previous.effective_physical_scale != imageScale && image.generation <= previous.target_generation))
                return false;
            previous = ImageTarget(image);
        }
        return true;
    }

    Native::ImageFontControlObservation ImageFontControlTestAccess::Observe(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        Native::ImageFontControlObservation result;
        result.enabled = control->SvgImagesEnabled();
        result.loaded = control->_loaded;
        result.source = static_cast<bool>(control->_readingSource);
        result.readingMain = control->_readingSource && static_cast<bool>(control->_readingSource->Main());
        result.readingDescriptors = control->_readingSource && static_cast<bool>(control->_readingSource->Descriptors());
        result.mathBound = control->_mathBound;
        result.diagramBound = control->_diagramBound;
        result.surface = control->_surfaceReady;
        result.refreshScheduled = control->_imageRefreshTimer && control->_imageRefreshTimer.IsEnabled();
        result.fontGeneration = control->_imageFontGeneration;
        result.loadedGeneration = control->_imageFontLoadedGeneration;
        result.watchVersion = control->_imageFontWatchVersion;
        result.barrier = control->_imageFontLayoutBarrier;
        result.actualWidth = control->ActualWidth();
        result.viewportWidth = control->_previewScroll ? control->_previewScroll.ViewportWidth() : 0;
        result.measuredWidth = control->_measuredWidth;
        result.error = control->_renderingError;
        if (auto watch = control->_imageFontWatch)
        {
            std::lock_guard lock(watch->gate);
            // Worker-owned fields may only be read after its idle transfer
            // under this same mutex, never while a work body is writing them.
            result.watchIdle = !watch->busy && !watch->closed && !watch->destroying;
            if (result.watchIdle)
                result.watchBaseline = !watch->diagnostics && watch->desiredLoaded &&
                                       watch->activeGeneration == control->_imageFontLoadedGeneration &&
                                       watch->desiredGeneration == watch->activeGeneration && watch->version &&
                                       watch->deliveredVersion == watch->version && !watch->pending;
        }
        uint32_t svgObject = UINT32_MAX, rasterObject = UINT32_MAX;
        if (control->_readingSource)
        {
            result.textGeneration = control->_readingSource->textGeneration;
            if (auto descriptors = control->_readingSource->Descriptors())
                for (auto const &object : descriptors->objects)
                    if (object.kind == Native::ReadingObjectKind::Image)
                    {
                        if (object.destination == L"font-control:text")
                            svgObject = object.object;
                        else if (object.destination == L"font-control:bmp")
                            rasterObject = object.object;
                    }
        }
        if (control->_images && control->_images->bound)
        {
            auto snapshot = control->_images->policy->Snapshot();
            result.epoch = snapshot.header.epoch;
            result.policyGeneration = snapshot.header.state_generation;
            for (uint32_t i = 0; i < snapshot.header.count; ++i)
            {
                auto const &row = snapshot.entries[i];
                if (row.object == svgObject)
                {
                    result.svgFound = true;
                    result.svg = row;
                }
                else if (row.object == rasterObject)
                {
                    result.rasterFound = true;
                    result.raster = row;
                }
            }
            if (auto entry = control->_images->FindPixels(svgObject))
                result.encoded = reinterpret_cast<uintptr_t>(entry->encoded.Get());
            if (auto entry = control->_images->FindPixels(rasterObject))
                result.rasterPixels = reinterpret_cast<uintptr_t>(entry->value.get());
        }
        if (control->_imagePresentation)
        {
            result.table = reinterpret_cast<uintptr_t>(control->_imagePresentation.get());
            result.tableGeneration = control->_imagePresentation->policy.header.state_generation;
            result.tableEpoch = control->_imagePresentation->policy.header.epoch;
            result.tableTextGeneration = control->_imagePresentation->textGeneration;
        }
        result.publishedTable = reinterpret_cast<uintptr_t>(control->_publishedImagePresentation.get());
        if (control->_presented.frame)
        {
            result.frame = reinterpret_cast<uintptr_t>(control->_presented.frame.get());
            result.layoutGeneration = control->_presented.frame->key.layout_generation;
            result.currentImage = control->CurrentImageFrame(*control->_presented.frame);
            result.currentAction = control->CurrentReadingAction(
                {control->_presented.frame, control->_automationSnapshot, control->_presented.serial});
            auto images = control->_presented.frame->images;
            if (images)
            {
                result.frameTable = reinterpret_cast<uintptr_t>(images.get());
                result.frameGeneration = images->policy.header.state_generation;
                if (auto entry = images->Find(svgObject))
                {
                    result.frameSvgFound = true;
                    result.frameSvg = entry->state;
                }
                if (auto entry = images->Find(rasterObject))
                {
                    result.frameRasterFound = true;
                    result.frameRaster = entry->state;
                }
            }
        }
        return result;
    }

    void ImageFontControlTestAccess::EnsureFontWatch(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        control->EnsureImageFontWatch();
    }

    bool ImageFontControlTestAccess::RequestRefresh(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        auto watch = control->_imageFontWatch;
        if (!watch || !control->SvgImagesEnabled() || !control->_loaded)
            return false;
        {
            std::lock_guard lock(watch->gate);
            if (watch->busy || watch->closed || watch->destroying || watch->diagnostics || !watch->desiredLoaded ||
                watch->activeGeneration != control->_imageFontLoadedGeneration ||
                watch->desiredGeneration != watch->activeGeneration || !watch->version ||
                watch->deliveredVersion != watch->version || watch->pending)
                return false;
        }
        watch->Request(ImageFontWatch::Command::Refresh);
        return true;
    }

    Native::CoreControllerLifetime ImageFontControlTestAccess::Lifetime(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        return control->_previewController ? control->_previewController->Lifetime() : Native::CoreControllerLifetime{};
    }

    std::weak_ptr<Native::ReadingMemory> ImageFontControlTestAccess::SourceMemory(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        return control->_readingSource ? control->_readingSource->memory : std::weak_ptr<Native::ReadingMemory>{};
    }

    std::array<std::shared_ptr<Native::ImageLoadTask>, 2> ImageFontControlTestAccess::Tasks(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        return control->_images ? control->_images->tasks : std::array<std::shared_ptr<Native::ImageLoadTask>, 2>{};
    }

    void ImageFontControlTestAccess::DetachAndStopWatch(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        control->_imageFontControlTests.reset();
        control->StopImageFonts(true);
    }
#endif
}

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
namespace Notepads::Markdown::Native
{
    ImageFontWatchDiagnosticResult RunImageFontWatchDiagnostic(std::shared_ptr<ReadingMemory> const &memory,
                                                               uint32_t scenario) noexcept
    {
        using Watch = winrt::Notepads::Markdown::implementation::ImageFontWatch;
        ImageFontWatchDiagnosticResult result;
        // Fixture ownership is independent of the paid production state. On a
        // timeout it is deliberately retained rather than leaving callbacks a
        // dangling stack test context; the failed diagnostic reports that debt.
        auto tests = std::unique_ptr<Watch::Diagnostics>{new (std::nothrow) Watch::Diagnostics};
        if (!tests)
        {
            result.error = E_OUTOFMEMORY;
            return result;
        }
        Watch *state{};
        bool created{};
        auto until = [](auto predicate)
        {
            auto deadline = GetTickCount64() + 5000;
            while (!predicate())
            {
                if (GetTickCount64() >= deadline)
                    return false;
                Sleep(1);
            }
            return true;
        };
        auto require = [](bool condition)
        {
            if (!condition)
                throw winrt::hresult_error(E_UNEXPECTED);
        };
        auto hasNotification = [&]
        {
            std::lock_guard lock(tests->notificationGate);
            return tests->heldNotification.has_value();
        };
        auto takeNotification = [&]
        {
            std::optional<Watch::Notification> notification;
            std::lock_guard lock(tests->notificationGate);
            if (tests->heldNotification)
            {
                notification.emplace(std::move(*tests->heldNotification));
                tests->heldNotification.reset();
            }
            return notification;
        };
        try
        {
            require(memory && scenario < 3 && tests->event && tests->idleReached && tests->idleProceed &&
                    tests->waitReached && tests->waitProceed);
            tests->holdNotifications = scenario == 2;
            state = Watch::Create(memory, {}, nullptr, tests.get());
            created = true;
            if (scenario == 1)
                tests->pauseIdle.store(true, std::memory_order_release);
            state->Request(Watch::Command::Load, 1);
            require(until(
                [&]
                {
                    return tests->refreshedGeneration.load(std::memory_order_acquire) == 1 &&
                           tests->waitsCreated.load(std::memory_order_acquire) == 1;
                }));
            // A real first baseline arms, and never fabricates a font change.
            require(tests->notifications.load(std::memory_order_acquire) == 0 && !hasNotification());
            if (scenario == 0)
            {
                tests->pauseWait.store(true, std::memory_order_release);
                SetEvent(tests->event.get());
                require(WaitForSingleObjectEx(tests->waitReached.get(), 5000, FALSE) == WAIT_OBJECT_0);
                state->Request(Watch::Command::Stop);
                state->Request(Watch::Command::Load, 2);
                // The old callback is still actually executing. Reload must
                // drain it on the background work body before a second wait.
                require(tests->activeWaits.load(std::memory_order_acquire) == 1);
                SetEvent(tests->waitProceed.get());
                require(until(
                    [&]
                    {
                        return tests->refreshedGeneration.load(std::memory_order_acquire) == 2 &&
                               tests->waitsCreated.load(std::memory_order_acquire) == 2 &&
                               tests->notifications.load(std::memory_order_acquire) >= 1;
                    }));
            }
            else if (scenario == 1)
            {
                require(WaitForSingleObjectEx(tests->idleReached.get(), 5000, FALSE) == WAIT_OBJECT_0);
                state->Request(Watch::Command::Destroy);
                std::exchange(state, nullptr)->Release();
                require(until([&] { return tests->workClosed.load(std::memory_order_acquire) == 1; }));
                require(tests->frees.load(std::memory_order_acquire) == 0); // Old W still pins the real paid block.
                SetEvent(tests->idleProceed.get());
            }
            else
            {
                SetEvent(tests->event.get());
                require(until(hasNotification));
                auto old = takeNotification();
                require(old.has_value());
                state->Request(Watch::Command::Stop);
                state->Request(Watch::Command::Load, 2);
                require(until([&] { return tests->refreshedGeneration.load(std::memory_order_acquire) == 2; }));
                old->Invoke();
                old->Invoke(); // Exactly-once Ack, despite repeated execution/destruction.
                old.reset();
                require(until(hasNotification));
                auto current = takeNotification();
                require(current.has_value());
                current->Invoke();
                current->Invoke();
                current.reset();
                require(tests->acknowledgements.load(std::memory_order_acquire) == 2);
                SetEvent(tests->event.get());
                require(until(hasNotification));
                auto rejected = takeNotification();
                require(rejected.has_value());
                rejected.reset(); // Same cleanup as a refused/unexecuted dispatcher delegate.
                require(tests->acknowledgements.load(std::memory_order_acquire) == 3);
                state->Request(Watch::Command::Refresh); // Unblocks delivery, no new state/wait.
                require(until(hasNotification));
                auto late = takeNotification();
                require(late.has_value());
                state->Request(Watch::Command::Destroy);
                std::exchange(state, nullptr)->Release();
                require(until([&] { return tests->workClosed.load(std::memory_order_acquire) == 1; }));
                require(tests->frees.load(std::memory_order_acquire) == 0); // N alone retains context after Stop.
                late->Invoke();
                late.reset();
                require(tests->acknowledgements.load(std::memory_order_acquire) == 4);
            }
            if (state)
            {
                state->Request(Watch::Command::Destroy);
                std::exchange(state, nullptr)->Release();
            }
            require(until([&] { return tests->frees.load(std::memory_order_acquire) == 1; }));
            require(tests->workClosed.load(std::memory_order_acquire) == 1 &&
                    tests->waitsCreated.load(std::memory_order_acquire) ==
                        tests->waitsClosed.load(std::memory_order_acquire) &&
                    tests->maximumWaits.load(std::memory_order_acquire) == 1 &&
                    tests->maximumBodies.load(std::memory_order_acquire) == 1 &&
                    tests->failures.load(std::memory_order_acquire) == 0);
            result.passed = true;
        }
        catch (...)
        {
            result.error = winrt::to_hresult();
        }
        SetEvent(tests->waitProceed.get());
        SetEvent(tests->idleProceed.get());
        tests->pauseWait.store(false, std::memory_order_release);
        tests->pauseIdle.store(false, std::memory_order_release);
        if (state)
        {
            state->Request(Watch::Command::Destroy);
            state->Release();
        }
        {
            auto held = takeNotification();
            held.reset();
        }
        if (created && !until([&] { return tests->frees.load(std::memory_order_acquire) == 1; }))
        {
            result.passed = false;
            if (SUCCEEDED(result.error))
                result.error = HRESULT_FROM_WIN32(ERROR_TIMEOUT);
            result.waitsCreated = tests->waitsCreated.load(std::memory_order_acquire);
            result.waitsClosed = tests->waitsClosed.load(std::memory_order_acquire);
            result.workClosed = tests->workClosed.load(std::memory_order_acquire);
            result.notifications = tests->notifications.load(std::memory_order_acquire);
            result.acknowledgements = tests->acknowledgements.load(std::memory_order_acquire);
            result.frees = tests->frees.load(std::memory_order_acquire);
            tests.release(); // Explicit failed-test fixture quarantine; never a dangling callback context.
            return result;
        }
        result.waitsCreated = tests->waitsCreated.load(std::memory_order_acquire);
        result.waitsClosed = tests->waitsClosed.load(std::memory_order_acquire);
        result.workClosed = tests->workClosed.load(std::memory_order_acquire);
        result.notifications = tests->notifications.load(std::memory_order_acquire);
        result.acknowledgements = tests->acknowledgements.load(std::memory_order_acquire);
        result.frees = tests->frees.load(std::memory_order_acquire);
        return result;
    }
}
#endif

#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
namespace winrt::Notepads::Markdown::implementation
{
    Native::ImageReuseControlObservation ImageReuseControlTestAccess::Observe(Markdown::MarkdownView const &view, uint32_t occurrence)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        Native::ImageReuseControlObservation result;
        result.loaded = control->_loaded;
        result.current = control->CurrentReadingAction(
            {control->_presented.frame, control->_automationSnapshot, control->_presented.serial});
        uint32_t object = UINT32_MAX;
        if (control->_readingSource)
        {
            result.text = control->_readingSource->textGeneration;
            if (auto descriptors = control->_readingSource->Descriptors())
                for (auto const &entry : descriptors->objects)
                    if (entry.kind == Native::ReadingObjectKind::Image && entry.destination == L"reuse-control:bmp")
                    {
                        if (occurrence)
                        {
                            --occurrence;
                            continue;
                        }
                        object = entry.object;
                        result.textStart = entry.text.start;
                        break;
                    }
        }
        if (control->_images)
        {
            if (control->_images->policy)
            {
                auto snapshot = control->_images->policy->Snapshot();
                result.epoch = snapshot.header.epoch;
                result.usage = snapshot.header;
                for (uint32_t index = 0; index < snapshot.header.count; ++index)
                    if (snapshot.entries[index].object == object)
                    {
                        result.found = true;
                        result.row = snapshot.entries[index];
                    }
                if (auto pixels = control->_images->FindPixels(object))
                {
                    result.rowEncoded = reinterpret_cast<uintptr_t>(pixels->encoded.Get());
                    result.animationStartTicks = pixels->animationStart.time_since_epoch().count();
                    result.frameIndex = pixels->frameIndex;
                }
            }
            if (control->_images->reuse)
            {
                auto cached = control->_images->reuse->Inspect();
                result.cacheScope = cached.scope;
                result.cacheCount = cached.count;
                result.cacheMetadata = cached.metadataRequestedBytes;
                for (size_t index = 0; index < cached.count; ++index)
                    result.cacheEncoded += cached.entries[index].encodedCapacity;
            }
        }
        return result;
    }
    std::shared_ptr<Native::ImagePixels const> ImageReuseControlTestAccess::Pixels(Markdown::MarkdownView const &view, uint32_t occurrence)
    {
        auto state = Observe(view, occurrence);
        auto control = get_self<MarkdownView>(view);
        if (state.found && control->_images)
            if (auto pixels = control->_images->FindPixels(state.row.object)) return pixels->value;
        return {};
    }
    Native::NativeEncodedImageHandle ImageReuseControlTestAccess::HoldEncoded(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        if (control->_images && control->_images->reuse)
            return control->_images->reuse->CloneEncodedForDiagnostics();
        return {};
    }
    void ImageReuseControlTestAccess::PressureClear(Markdown::MarkdownView const &view)
    {
        auto control = get_self<MarkdownView>(view);
        control->CheckUiThread();
        if (control->_images) control->_images->ClearReuse();
    }
}
#endif
