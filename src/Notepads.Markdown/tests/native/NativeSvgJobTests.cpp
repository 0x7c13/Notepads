// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeImageTestSupport.h"
#include "NativeSvgJobTests.h"
#include "Resources/Svg/NativeSvgJob.h"
#include "Resources/Images/NativeImageCoordinator.h"
#include <algorithm>
#include <bit>
#include <source_location>
#include <string_view>

namespace Notepads::Markdown::Native::SvgJobTests
{
    namespace
    {
        void Require(bool condition, std::source_location location = std::source_location::current())
        {
            if (!condition)
                throw winrt::hresult_error{E_FAIL, L"SVG worker assertion at line " + std::to_wstring(location.line())};
        }
        void Check(int32_t status)
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw NativeImageFailure{status};
        }
        struct Fixture
        {
            std::shared_ptr<ReadingMemory> memory;
            std::shared_ptr<NativeImagePolicy> policy;
            std::shared_ptr<ImageLoadTask> task;
            std::shared_ptr<NativeImageDecodeResult> result;

            explicit Fixture(std::shared_ptr<ReadingMemory> value, std::shared_ptr<NativeImagePolicy> reuse = {},
                             uint64_t generation = 1)
                : memory(std::move(value))
            {
                Require(memory && memory->ModelOwner().Get());
                policy = reuse ? std::move(reuse) : NativeImagePolicy::Create();
                notepads_markdown_image_epoch epoch{};
                auto const &identity = memory->Identity();
                std::copy(std::begin(identity.document), std::end(identity.document),
                          std::begin(epoch.source.document));
                epoch.source.revision = identity.revision;
                epoch.source_generation = generation;
                epoch.provider_generation = generation;
                policy->Begin(epoch);
                task = ImageLoadTask::Create(memory);
                task->request = StartImageTestRequest(policy, 1, {1, 17, 512, 1},
                                                      NOTEPADS_MARKDOWN_IMAGE_SVG);
                result = std::shared_ptr<NativeImageDecodeResult>{task, &task->decodeResult};
            }
            ~Fixture()
            {
                // Never bypass a failed retirement. In a successful direct
                // test there is no outer provider stream left to close.
                if (task)
                {
                    task->NotifyDecodeComplete();
                    task->Finish();
                }
            }
            NativeSvgImageTarget Target() const noexcept
            {
                return {512, 1, 17, true};
            }
            NativeEncodedImageHandle Encoded(std::string_view source)
            {
                auto charge = task->request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, ImageMaximumEncodedBytes);
                auto encoded = NativeEncodedImage::Create(memory, std::move(charge), ImageMaximumEncodedBytes);
                encoded->Append({reinterpret_cast<uint8_t const *>(source.data()), source.size()});
                return encoded;
            }
            void Decode(std::string_view source)
            {
                using namespace winrt::Windows::Storage::Streams;
                InMemoryRandomAccessStream stream;
                DataWriter writer{stream};
                writer.WriteBytes({reinterpret_cast<uint8_t const *>(source.data()),
                                   reinterpret_cast<uint8_t const *>(source.data() + source.size())});
                (void)writer.StoreAsync().get();
                (void)writer.DetachStream();
                writer.Close();
                stream.Seek(0);
                DecodeImageAsync(task->request, memory, stream, 2048, result, task, Target()).get();
            }
            void RequireRetired(bool outerDecodeComplete = true, bool expectReady = true)
            {
                try
                {
                    if (expectReady)
                        Require(result->Ready());
                    std::lock_guard lock(task->mutex);
                    Require(task->svgStarted && task->svgRetired && !task->svgQuarantined && !task->svgKeepAlive);
                    Require(!task->svgState && !task->svgFailedNode && !task->svgAccount && !task->svgResources &&
                            !task->svgCoreCancel);
                    Require(task->svgDecodeComplete == outerDecodeComplete);
                    auto const &receipt = task->svgReceipt;
                    Require(receipt.threadSignaled && receipt.cancelEntryRevoked && receipt.cancelLeasesZero &&
                            receipt.nodeFreed);
                    Require(receipt.engine.cleanup_succeeded == 1 && receipt.native.retired &&
                            receipt.destroyed.consumed == 1 && receipt.destroyed.cleanup_succeeded == 1 &&
                            receipt.resources.consumed == 1 && receipt.resources.cleanup_succeeded == 1);
                    Require(receipt.stackReserved == 8ull * 1024 * 1024 && receipt.stackCommitted > 0 &&
                            receipt.stackGuard > 0 && receipt.stackHeadroom > 0 && receipt.nodeBytes > 0);
                    Require(receipt.workerThread != 0 && receipt.reaperThread != 0 &&
                            receipt.workerThread != receipt.reaperThread);
                }
                catch (winrt::hresult_error const &error)
                {
                    auto const &receipt = task->svgReceipt;
                    throw winrt::hresult_error{
                        error.code(), error.message() + L" status=" + std::to_wstring(result->nativeStatus) + L" hr=" +
                                          std::to_wstring(static_cast<uint32_t>(result->error)) + L" close=" +
                                          std::to_wstring(static_cast<uint32_t>(result->closeError)) + L" failure=" +
                                          std::to_wstring(result->failure) + L" started=" +
                                          std::to_wstring(task->svgStarted) + L" quarantine=" +
                                          std::to_wstring(task->svgQuarantined) + L" signaled=" +
                                          std::to_wstring(receipt.threadSignaled) + L" nodeFreed=" +
                                          std::to_wstring(receipt.nodeFreed) + L" engine=" +
                                          std::to_wstring(receipt.engine.cleanup_succeeded) + L" native=" +
                                          std::to_wstring(receipt.native.retired)};
                }
            }
        };

        constexpr std::string_view Basic = "<svg xmlns='http://www.w3.org/2000/svg' width='64' height='32'>"
                                           "<rect width='64' height='32' fill='#1256ab'/></svg>";

        uint32_t Ink(NativeImageDecodeResult const &result, double left, double top, double right, double bottom)
        {
            auto scale = static_cast<double>(std::bit_cast<float>(result.svgPlan.scale_bits));
            auto const &pixels = *result.pixels;
            auto x0 = static_cast<uint32_t>((std::max)(0.0, std::floor(left * scale)));
            auto y0 = static_cast<uint32_t>((std::max)(0.0, std::floor(top * scale)));
            auto x1 = (std::min)(result.svgPlan.width, static_cast<uint32_t>(std::ceil(right * scale)));
            auto y1 = (std::min)(result.svgPlan.height, static_cast<uint32_t>(std::ceil(bottom * scale)));
            uint32_t count{};
            for (auto y = y0; y < y1; ++y)
                for (auto x = x0; x < x1; ++x)
                    if (pixels.pixels[static_cast<size_t>(y) * result.svgPlan.stride + static_cast<size_t>(x) * 4 + 3])
                        ++count;
            return count;
        }
        void Event(winrt::handle &handle)
        {
            handle.attach(
                CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET, EVENT_MODIFY_STATE | SYNCHRONIZE));
            Require(static_cast<bool>(handle));
        }
        class Driver final
        {
          public:
            Driver(LPTHREAD_START_ROUTINE entry, void *context, HANDLE unblock = nullptr) : _unblock(unblock)
            {
                _thread.attach(CreateThread(nullptr, 0, entry, context, 0, nullptr));
                Require(static_cast<bool>(_thread));
            }
            ~Driver()
            {
                if (_unblock)
                    (void)SetEvent(_unblock);
                if (_thread)
                    (void)WaitForSingleObjectEx(_thread.get(), INFINITE, FALSE);
            }
            void Join()
            {
                Require(WaitForSingleObjectEx(_thread.get(), INFINITE, FALSE) == WAIT_OBJECT_0);
                _unblock = nullptr;
            }

          private:
            winrt::handle _thread;
            HANDLE _unblock{};
        };
        struct OverlapOwner
        {
            ImageCoordinator coordinator;
            ~OverlapOwner()
            {
                // Driver destructors already released their real tail gates
                // and joined before this owner can drop a physical slot.
                for (auto const &task : coordinator.tasks)
                    if (task)
                    {
                        task->NotifyDecodeComplete();
                        task->Finish();
                    }
            }
        };
        struct RenderContext
        {
            std::shared_ptr<ImageLoadTask> task;
            std::shared_ptr<ReadingMemory> memory;
            std::shared_ptr<NativeImageDecodeResult> result;
            NativeEncodedImageHandle encoded;
            NativeSvgImageTarget target{512, 1, 23, true};
            HRESULT error{S_OK};
            bool returned{};
            static DWORD WINAPI Run(void *value) noexcept
            {
                auto &context = *static_cast<RenderContext *>(value);
                auto apartment = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
                context.error = apartment;
                if (SUCCEEDED(apartment))
                {
                    try
                    {
                        RenderSvgImageOnBackground(context.task, context.memory, std::move(context.encoded),
                                                   context.target, context.result);
                    }
                    catch (...)
                    {
                        context.error = winrt::to_hresult().value;
                    }
                    CoUninitialize();
                }
                context.returned = true;
                return 0;
            }
            void Clear() noexcept
            {
                result.reset();
                task.reset();
                memory.reset();
                encoded.Reset();
            }
        };
        notepads_markdown_image_epoch Epoch(std::shared_ptr<ReadingMemory> const &memory, uint64_t source, uint64_t provider)
        {
            notepads_markdown_image_epoch epoch{};
            auto const &identity = memory->Identity();
            std::copy(std::begin(identity.document), std::end(identity.document), std::begin(epoch.source.document));
            epoch.source.revision = identity.revision;
            epoch.source_generation = source;
            epoch.provider_generation = provider;
            return epoch;
        }
        notepads_markdown_svg_work_snapshot Work(ImageLoadTask &task)
        {
            std::lock_guard lock(task.mutex);
            Require(task.svgResources != nullptr);
            notepads_markdown_svg_work_snapshot work{};
            work.header = {NOTEPADS_MARKDOWN_SVG_WORK_ABI_VERSION, sizeof(work)};
            auto error = notepads_markdown_core_error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
            Check(notepads_markdown_svg_resources_read_work(task.svgResources, &work, &error));
            return work;
        }
        notepads_markdown_svg_retained_snapshot Metadata(ImageLoadTask &task)
        {
            notepads_markdown_svg_retained_snapshot metadata{};
            metadata.header = {NOTEPADS_MARKDOWN_SVG_RETAINED_ABI_VERSION, sizeof(metadata)};
            auto error = notepads_markdown_core_error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
            Check(notepads_markdown_svg_retained_read(task.metadataRetained, &metadata, &error));
            return metadata;
        }
        uint64_t Total(notepads_markdown_svg_retained_snapshot const &owner)
        {
            auto maximum = (std::numeric_limits<uint64_t>::max)();
            Require(owner.native_bytes <= maximum - owner.preview_bytes);
            auto total = owner.native_bytes + owner.preview_bytes;
            Require(total <= maximum - owner.header_bytes);
            return total + owner.header_bytes;
        }
        bool TailBlocked(ImageLoadTask &task)
        {
            Require(task.testWorkerThread &&
                    WaitForSingleObjectEx(task.testWorkerThread.get(), 0, FALSE) == WAIT_TIMEOUT);
            std::lock_guard lock(task.mutex);
            auto const &receipt = task.svgReceipt;
            return task.svgStarted && !task.svgRetired && !task.svgQuarantined && task.svgKeepAlive &&
                   task.svgResources && task.request && receipt.engine.cleanup_succeeded == 1 &&
                   receipt.native.retired && receipt.workerFree.Succeeded() && !receipt.threadSignaled &&
                   receipt.destroyAttempts == 0 && !task.finished.load(std::memory_order_acquire);
        }
        int32_t Busy(std::shared_ptr<NativeImagePolicy> const &policy)
        {
            try
            {
                auto unexpected = StartImageTestRequest(policy, 3);
                return NOTEPADS_MARKDOWN_CORE_OK;
            }
            catch (NativeFailure const &error)
            {
                return error.status;
            }
        }
        struct CancelContext
        {
            ImageLoadTask *task{};
            HANDLE reached{}, proceed{};
            bool failed{};
            static DWORD WINAPI Run(void *value) noexcept
            {
                auto &context = *static_cast<CancelContext *>(value);
                if (WaitForSingleObjectEx(context.reached, 10000, FALSE) != WAIT_OBJECT_0)
                {
                    context.failed = true;
                    return 0;
                }
                CancelSvgImageTask(*context.task);
                if (context.proceed && !SetEvent(context.proceed))
                    context.failed = true;
                return 0;
            }
        };
        struct LeaseContext
        {
            ImageLoadTask *task{};
            bool held{}, failed{};
            static DWORD WINAPI Run(void *value) noexcept
            {
                auto &context = *static_cast<LeaseContext *>(value);
                auto &task = *context.task;
                auto retained = WaitForSingleObjectEx(task.testLeaseRetained.get(), 10000, FALSE);
                auto blocked = WaitForSingleObjectEx(task.testReaperWaitingLease.get(), 10000, FALSE);
                if (retained == WAIT_OBJECT_0 && blocked == WAIT_OBJECT_0)
                {
                    std::lock_guard lock(task.mutex);
                    context.held = task.svgReceipt.threadSignaled && task.svgReceipt.activeCancelCalls == 1 &&
                                   task.svgReceipt.destroyAttempts == 0 && task.svgReceipt.cancelEntryRevoked &&
                                   !task.svgReceipt.cancelLeasesZero && task.svgKeepAlive && task.svgResources &&
                                   task.svgState && task.svgReceipt.engine.cancelled == 0 &&
                                   !task.finished.load(std::memory_order_acquire);
                }
                else
                    context.failed = true;
                if (!SetEvent(task.testLeaseProceed.get()))
                    context.failed = true;
                return 0;
            }
        };
    }

    Results Run(std::shared_ptr<ReadingMemory> const &memory)
    {
        Require(memory && memory->ModelOwner().Get());
        Results results;
        {
            Fixture fixture{memory};
            fixture.Decode(Basic);
            fixture.RequireRetired();
            auto const &value = *fixture.result;
            Require(value.Ready() && value.svg && value.encoded && SUCCEEDED(value.closeError));
            Require(value.svgPlan.natural_width_dip == 64 && value.svgPlan.natural_height_dip == 32 &&
                    value.svgPlan.identity.target_generation == 17 && value.svgPlan.identity.font_generation == 1 &&
                    value.svgPlan.identity.target_generation == fixture.task->request->Target().target_generation &&
                    value.svgPlan.identity.font_generation == fixture.task->request->Target().font_generation &&
                    fixture.task->svgReceipt.native.session != 0);
            notepads_markdown_svg_retained_snapshot taskMetadata{};
            taskMetadata.header = {NOTEPADS_MARKDOWN_SVG_RETAINED_ABI_VERSION, sizeof(taskMetadata)};
            auto taskError = notepads_markdown_core_error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
            Check(notepads_markdown_svg_retained_read(fixture.task->metadataRetained, &taskMetadata, &taskError));
            Require(taskMetadata.kind == NOTEPADS_MARKDOWN_SVG_RETAINED_METADATA && taskMetadata.phase == 2 &&
                    taskMetadata.refs == 1 && taskMetadata.preview_bytes > 0 && taskMetadata.header_bytes > 0 &&
                    taskMetadata.native_bytes == ReadingMemory::AllocationRequestBytes(fixture.task->allocationBytes));
            Require(value.pixels->pixels.size() == value.svgPlan.capacity &&
                    value.svgPlan.stride == value.svgPlan.width * 4 &&
                    std::any_of(value.pixels->pixels.begin(), value.pixels->pixels.end(),
                                [](uint8_t byte) { return byte != 0; }));
            auto const &receipt = fixture.task->svgReceipt;
            Require(receipt.coverage.batches == 0 && receipt.coverage.glyph_draws == 0 &&
                    receipt.coverage.generated_fonts == 0 && receipt.coverage.byte_owners_created == 0);
            results.stackReserved = receipt.stackReserved;
            results.stackCommitted = receipt.stackCommitted;
            results.stackGuard = receipt.stackGuard;
            results.stackHeadroom = receipt.stackHeadroom;
            results.nodeBytes = receipt.nodeBytes;
            results.workerThread = receipt.workerThread;
            results.reaperThread = receipt.reaperThread;
            fixture.task->Finish();
            Require(fixture.task->finished.load(std::memory_order_acquire) && !fixture.task->request);
            auto metadata = fixture.task->metadataRetained;
            auto weakTask = std::weak_ptr<ImageLoadTask>{fixture.task};
            fixture.result.reset();
            fixture.task.reset();
            Require(weakTask.expired());
            // The real control block still exists for this weak owner; its
            // physical reference pins the retained record after Task dies.
            Check(notepads_markdown_svg_retained_read(metadata, &taskMetadata, &taskError));
            Require(taskMetadata.phase == 2 && taskMetadata.refs == 1 && taskMetadata.native_bytes > 0);
            weakTask.reset();
            // Never read metadata after the actual last weak/control free.
            results.basic = true;
            ++results.checks;
        }
        {
            Fixture fixture{memory};
            fixture.Decode("<svg xmlns='http://www.w3.org/2000/svg' width='100.5' height='50.25' "
                           "viewBox='0 0 100.5 50.25'><rect width='100.5' height='50.25' fill='red'/></svg>");
            fixture.RequireRetired();
            auto const &value = *fixture.result;
            Require(value.Ready() && value.svg && value.svgPlan.natural_width_dip == 100.5 &&
                    value.svgPlan.natural_height_dip == 50.25);
            Require(value.pixels->metadata.intrinsic_width == 0 && value.pixels->metadata.intrinsic_height == 0 &&
                    value.svgPlan.extent_width > 0 && value.svgPlan.extent_height > 0 &&
                    value.pixels->pixels.size() == value.svgPlan.capacity);
            results.fractional = true;
            ++results.checks;
        }
        {
            Fixture fixture{memory};
            fixture.Decode("<svg xmlns='http://www.w3.org/2000/svg'><g></svg>");
            fixture.RequireRetired(true, false);
            Require(!fixture.result->Ready() && fixture.result->failure != NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE &&
                    fixture.result->nativeStatus != NOTEPADS_MARKDOWN_CORE_OK && SUCCEEDED(fixture.result->closeError));
            results.invalidXmlStatus = fixture.result->nativeStatus;
            results.invalidXml = true;
            ++results.checks;
        }
        {
            Fixture fixture{memory};
            fixture.task->Cancel();
            fixture.Decode(Basic);
            Require(!fixture.result->Ready() && fixture.result->failure == NOTEPADS_MARKDOWN_IMAGE_CANCELLED);
            fixture.task->Finish();
            Require(fixture.task->finished.load(std::memory_order_acquire) && !fixture.task->request &&
                    !fixture.task->svgStarted && !fixture.task->svgCoreCancel);
            results.preCancelled = true;
            ++results.checks;
        }
        {
            Fixture fixture{memory};
            RenderSvgImageOnBackground(fixture.task, memory, fixture.Encoded(Basic), fixture.Target(), fixture.result);
            fixture.RequireRetired(false);
            Require(fixture.result->Ready());
            fixture.task->Finish();
            Require(!fixture.task->finished.load(std::memory_order_acquire) && fixture.task->request &&
                    fixture.task->svgFinishPending);
            // This direct consumer has no provider IO. Signal its real outer
            // work completion separately from the already-retired SVG worker.
            fixture.task->NotifyDecodeComplete();
            Require(fixture.task->finished.load(std::memory_order_acquire) && !fixture.task->request);
            results.finishGate = true;
            ++results.checks;
        }
        {
            Fixture fixture{memory};
            fixture.Decode("<svg xmlns='http://www.w3.org/2000/svg' width='240' height='150'>"
                           "<text x='8' y='32' font-family='Segoe UI' font-size='20'>English</text>"
                           "<text x='8' y='82' font-family='Segoe UI' font-size='20'>中文</text>"
                           "<text x='8' y='132' font-family='Segoe UI' font-size='20'>العربية</text></svg>");
            fixture.RequireRetired();
            Require(fixture.result->Ready());
            results.latinPixels = Ink(*fixture.result, 0, 0, 200, 45);
            results.chinesePixels = Ink(*fixture.result, 0, 50, 200, 95);
            results.arabicPixels = Ink(*fixture.result, 0, 100, 200, 145);
            // No background exists; the independent right strip must remain
            // transparent, rather than counting an opaque canvas as text.
            Require(results.latinPixels > 0 && results.chinesePixels > 0 && results.arabicPixels > 0 &&
                    Ink(*fixture.result, 210, 0, 240, 150) == 0);
            results.fragmentReads = fixture.task->svgReceipt.native.fragmentReads;
            results.fragmentReleases = fixture.task->svgReceipt.native.fragmentReleases;
            Require(results.fragmentReads > 0 && results.fragmentReads == results.fragmentReleases);
            auto const &coverage = fixture.task->svgReceipt.coverage;
            Require(coverage.batches > 0 && coverage.glyph_draws > 0 && coverage.generated_fonts > 0 &&
                    coverage.byte_owners_created > 0 && coverage.byte_owners_created == coverage.byte_owners_freed &&
                    coverage.quarantined_owners == 0);
            results.mixedText = true;
            ++results.checks;
        }
        return results;
    }

    FreeFailureResults RunUnpublishedFreeFailure(std::shared_ptr<ReadingMemory> const &memory,
                                                 NativeSvgJobTestFault mode)
    {
        Require(mode == NativeSvgJobTestFault::PayloadFree || mode == NativeSvgJobTestFault::MetadataFree ||
                mode == NativeSvgJobTestFault::PartialMetadata);
        Fixture fixture{memory};
        fixture.task->svgTestFault = mode;
        RenderSvgImageOnBackground(fixture.task, memory, fixture.Encoded(Basic), fixture.Target(), fixture.result);
        FreeFailureResults results;
        results.mode = mode;
        auto const &free = mode == NativeSvgJobTestFault::PartialMetadata ? fixture.task->svgReceipt.workerFree
                                                                          : fixture.task->svgReceipt.discardFree;
        results.notReady = !fixture.result->Ready();
        results.quarantined = fixture.task->svgQuarantined && !fixture.task->svgRetired;
        results.selfPinned = fixture.task->svgKeepAlive.get() == fixture.task.get() && fixture.task->svgMemory &&
                             fixture.task->svgCoreCancel;
        results.physicalFailure = free.physicalFailures > 0 && free.firstFailedPayload;
        results.physicalFailures = free.physicalFailures;
        results.rootFailures = free.rootFailures;
        results.cleanupStatus = free.firstStatus;
        results.firstWorkStatus = fixture.result->nativeStatus;
        results.threadSignaled = fixture.task->svgReceipt.threadSignaled;
        fixture.task->NotifyDecodeComplete();
        fixture.task->Finish();
        auto snapshot = fixture.policy->Snapshot();
        results.slotHeld = !fixture.task->finished.load(std::memory_order_acquire) && fixture.task->request &&
                           snapshot.header.active_requests == 1;
        Require(results.physicalFailure);
        auto allocation = ReadingMemory::InspectAllocation(free.firstFailedPayload);
        Require(allocation.hasAction);
        notepads_markdown_svg_retained_snapshot owner{};
        owner.header = {NOTEPADS_MARKDOWN_SVG_RETAINED_ABI_VERSION, sizeof(owner)};
        auto error = notepads_markdown_core_error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        Check(notepads_markdown_svg_retained_read(static_cast<notepads_markdown_svg_retained *>(allocation.action.context), &owner, &error));
        notepads_markdown_svg_work_snapshot work{};
        work.header = {NOTEPADS_MARKDOWN_SVG_WORK_ABI_VERSION, sizeof(work)};
        Check(notepads_markdown_svg_resources_read_work(fixture.task->svgResources, &work, &error));
        results.jobBytes = work.job_bytes;
        results.controlBytes = work.control_bytes;
        results.failedOwnerNativeBytes = owner.native_bytes;
        results.failedOwnerPreviewBytes = owner.preview_bytes;
        results.failedOwnerHeaderBytes = owner.header_bytes;
        results.prefixTotal = allocation.requestedTotal;
        results.failedOwnerRefs = owner.refs;
        results.failedOwnerPhase = owner.phase;
        results.failedOwnerJobBytes = owner.job_borrow_bytes;
        results.rootLiveBytes = owner.root_live_bytes;
        auto maximum = (std::numeric_limits<uint64_t>::max)();
        auto checked = owner.preview_bytes <= maximum - owner.native_bytes;
        auto physicalTotal = checked ? owner.native_bytes + owner.preview_bytes : 0;
        checked = checked && owner.header_bytes <= maximum - physicalTotal;
        results.failedTotal = checked ? physicalTotal + owner.header_bytes : 0;
        // A successful peer's physical callback refunds control P/L while its
        // creator row still holds the complete job attribution until Return.
        // Compare the failed owner's exact cost to each ledger independently.
        results.feesHeld = checked && (owner.phase == 1 || owner.phase == 2) && owner.refs == 2 &&
                           owner.native_bytes == allocation.requestedTotal && owner.native_bytes > 0 &&
                           owner.preview_bytes > 0 && owner.header_bytes > 0 &&
                           owner.job_borrow_bytes == results.failedTotal && work.job_bytes >= results.failedTotal &&
                           work.control_bytes >= results.failedTotal && owner.root_live_bytes >= results.failedTotal;
        if (!(results.notReady && results.quarantined && results.selfPinned && results.slotHeld && results.feesHeld &&
              results.threadSignaled && results.cleanupStatus != NOTEPADS_MARKDOWN_CORE_OK))
            throw winrt::hresult_error{
                E_FAIL,
                L"SVG free failure mode=" + std::to_wstring(static_cast<uint32_t>(mode)) + L" notReady=" +
                    std::to_wstring(results.notReady) + L" quarantined=" + std::to_wstring(results.quarantined) +
                    L" selfPinned=" + std::to_wstring(results.selfPinned) + L" slot=" +
                    std::to_wstring(results.slotHeld) + L" fees=" + std::to_wstring(results.feesHeld) + L" signaled=" +
                    std::to_wstring(results.threadSignaled) + L" physicalFailure=" +
                    std::to_wstring(results.physicalFailure) + L" checked=" + std::to_wstring(checked) +
                    L" workStatus=" + std::to_wstring(results.firstWorkStatus) + L" cleanupStatus=" +
                    std::to_wstring(results.cleanupStatus) + L" phase=" + std::to_wstring(owner.phase) + L" refs=" +
                    std::to_wstring(owner.refs) + L" prefix=" + std::to_wstring(allocation.requestedTotal) + L" P=" +
                    std::to_wstring(owner.native_bytes) + L" L=" + std::to_wstring(owner.preview_bytes) + L" H=" +
                    std::to_wstring(owner.header_bytes) + L" failedTotal=" + std::to_wstring(results.failedTotal) +
                    L" ownerJob=" + std::to_wstring(owner.job_borrow_bytes) + L" job=" +
                    std::to_wstring(work.job_bytes) + L" control=" + std::to_wstring(work.control_bytes) + L" root=" +
                    std::to_wstring(owner.root_live_bytes)};
        results.checks = 1;
        // No automatic or test-only retry of this failed prefix. Its original
        // task and physical request slot remain permanently quarantined.
        return results;
    }

    HandoffResults RunHandoffCancellation(std::shared_ptr<ReadingMemory> const &memory, bool afterCommit)
    {
        Fixture fixture{memory};
        auto &task = *fixture.task;
        auto &reached = afterCommit ? task.testAfterCommitReached : task.testBeforeCommitReached;
        auto &proceed = afterCommit ? task.testAfterCommitProceed : task.testBeforeCommitProceed;
        Event(reached);
        Event(proceed);
        CancelContext cancellation{&task, reached.get(), proceed.get()};
        Driver driver{CancelContext::Run, &cancellation};
        RenderSvgImageOnBackground(fixture.task, memory, fixture.Encoded(Basic), fixture.Target(), fixture.result);
        driver.Join();
        Require(!cancellation.failed);
        fixture.RequireRetired(false, false);
        HandoffResults results;
        results.afterCommit = afterCommit;
        results.noReady = !fixture.result->Ready();
        results.ownersTransferred = fixture.result->pixels && fixture.result->encoded;
        results.discardPhysicalFreed = task.svgReceipt.discardFree.physicalFreed;
        results.discardedBeforeCommit = !task.svgOwnerHandoffCommitted && !fixture.result->pixels &&
                                        !fixture.result->encoded && results.discardPhysicalFreed >= 3;
        results.retired = task.svgRetired && !task.svgQuarantined && task.svgReceipt.destroyed.cleanup_succeeded;
        results.destroyAttempts = task.svgReceipt.destroyAttempts;
        results.status = fixture.result->nativeStatus;
        task.NotifyDecodeComplete();
        task.Finish();
        results.slotReleased = task.finished.load(std::memory_order_acquire) && !task.request &&
                               fixture.policy->Snapshot().header.active_requests == 0;
        Require(results.noReady && results.retired && results.slotReleased && results.destroyAttempts == 1 &&
                results.status == NOTEPADS_MARKDOWN_CORE_CANCELLED);
        if (afterCommit)
            Require(task.svgOwnerHandoffCommitted && results.ownersTransferred && !results.discardedBeforeCommit &&
                    results.discardPhysicalFreed == 0 &&
                    fixture.result->pixels->pixels.size() == fixture.result->svgPlan.capacity);
        else
            Require(!results.ownersTransferred && results.discardedBeforeCommit);
        results.checks = 1;
        return results;
    }

    LeaseResults RunLeaseAcrossExit(std::shared_ptr<ReadingMemory> const &memory)
    {
        Fixture fixture{memory};
        auto &task = *fixture.task;
        Event(task.testJobCreated);
        Event(task.testLeaseRetained);
        Event(task.testLeaseProceed);
        Event(task.testReaperWaitingLease);
        CancelContext cancellation{&task, task.testJobCreated.get(), nullptr};
        LeaseContext held{&task};
        Driver cancelDriver{CancelContext::Run, &cancellation};
        Driver releaseDriver{LeaseContext::Run, &held};
        RenderSvgImageOnBackground(fixture.task, memory, fixture.Encoded(Basic), fixture.Target(), fixture.result);
        cancelDriver.Join();
        releaseDriver.Join();
        Require(!cancellation.failed && !held.failed);
        fixture.RequireRetired(false, false);
        LeaseResults results;
        results.retainedAcrossExit = held.held;
        results.destroyBlockedWhileHeld = held.held;
        results.destroyAttempts = task.svgReceipt.destroyAttempts;
        results.threadSignaled = task.svgReceipt.threadSignaled;
        results.cancellationStatus = task.svgReceipt.cancellationStatus;
        results.tokenUnwrittenAtExit = task.svgReceipt.engine.cancelled == 0;
        results.tokenCancelledAfterLease = task.svgReceipt.destroyed.cancelled == 1;
        results.oldTaskRetired = task.svgRetired && !task.svgQuarantined && task.svgReceipt.activeCancelCalls == 0 &&
                                 task.svgReceipt.cancelLeasesZero && !fixture.result->Ready();
        task.NotifyDecodeComplete();
        task.Finish();
        results.slotReleased = task.finished.load(std::memory_order_acquire) && !task.request &&
                               fixture.policy->Snapshot().header.active_requests == 0;
        // A fresh request in the same policy/model gets its own original Core
        // token. The retired task's entry cannot cancel this successor.
        Fixture successor{memory, fixture.policy, 2};
        CancelSvgImageTask(task);
        RenderSvgImageOnBackground(successor.task, memory, successor.Encoded(Basic), successor.Target(),
                                   successor.result);
        successor.RequireRetired(false);
        results.successorReady = successor.result->Ready() && successor.result->svgPlan.identity.job_nonce !=
                                                                  task.svgReceipt.destroyed.identity.job_nonce;
        Require(results.retainedAcrossExit && results.destroyBlockedWhileHeld && results.oldTaskRetired &&
                results.slotReleased && results.successorReady && results.threadSignaled &&
                results.destroyAttempts == 1 && results.cancellationStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                results.tokenUnwrittenAtExit && results.tokenCancelledAfterLease);
        results.checks = 1;
        return results;
    }

    OverlapResults RunOverlapAndRetire(std::shared_ptr<ReadingMemory> const &memory, OverlapRetireMode mode)
    {
        Require(memory && memory->ModelOwner().Get());
        Require(mode == OverlapRetireMode::SourceEpoch || mode == OverlapRetireMode::ProviderEpoch ||
                mode == OverlapRetireMode::CoordinatorRetire);
        auto error = notepads_markdown_core_error{NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        notepads_markdown_core_model_summary_result summary{};
        summary.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(summary)};
        Check(notepads_markdown_core_model_summary(memory->ModelOwner().Get(), &summary, &error));
        Require(summary.objects >= 3);

        OverlapResults results;
        results.mode = mode;
        results.sourceGenerationBefore = 1;
        results.providerGenerationBefore = 1;
        OverlapOwner owner;
        auto &coordinator = owner.coordinator;
        coordinator.policy = NativeImagePolicy::Create();
        auto epoch = Epoch(memory, 1, 1);
        coordinator.policy->Begin(epoch);
        std::array<RenderContext, 2> contexts;
        for (size_t index = 0; index < contexts.size(); ++index)
        {
            auto task = ImageLoadTask::Create(memory);
            task->request = StartImageTestRequest(coordinator.policy, static_cast<uint32_t>(index + 1),
                                                  {1, 23, 512, 1}, NOTEPADS_MARKDOWN_IMAGE_SVG);
            Event(task->testWorkerTailReached);
            Event(task->testWorkerTailProceed);
            coordinator.tasks[index] = task;
            auto &context = contexts[index];
            context.task = task;
            context.memory = memory;
            context.result = std::shared_ptr<NativeImageDecodeResult>{task, &task->decodeResult};
            // Direct entry has no provider IO. The independent stream consumer
            // proves Size-based allocation; this fixture uses one exact owner.
            auto charge = task->request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, Basic.size());
            context.encoded = NativeEncodedImage::Create(memory, std::move(charge), Basic.size());
            context.encoded->Append({reinterpret_cast<uint8_t const *>(Basic.data()), Basic.size()});
        }
        Driver firstDriver{RenderContext::Run, &contexts[0], coordinator.tasks[0]->testWorkerTailProceed.get()};
        Driver secondDriver{RenderContext::Run, &contexts[1], coordinator.tasks[1]->testWorkerTailProceed.get()};
        auto first = coordinator.tasks[0];
        auto second = coordinator.tasks[1];
        Require(WaitForSingleObjectEx(first->testWorkerTailReached.get(), 10000, FALSE) == WAIT_OBJECT_0);
        Require(WaitForSingleObjectEx(second->testWorkerTailReached.get(), 10000, FALSE) == WAIT_OBJECT_0);
        results.originalSlots = coordinator.tasks.size() == 2 && first == contexts[0].task &&
                                second == contexts[1].task && first != second && first->request->Key().object == 1 &&
                                second->request->Key().object == 2;
        results.overlappingWorkers = TailBlocked(*first) && TailBlocked(*second);
        auto firstWork = Work(*first);
        auto secondWork = Work(*second);
        results.firstWorkerThread = first->svgReceipt.workerThread;
        results.secondWorkerThread = second->svgReceipt.workerThread;
        results.distinctWorkers = results.firstWorkerThread != 0 && results.secondWorkerThread != 0 &&
                                  results.firstWorkerThread != results.secondWorkerThread;
        results.distinctJobs = firstWork.account_nonce != 0 && secondWork.account_nonce != 0 &&
                               firstWork.account_nonce != secondWork.account_nonce;
        results.firstJobBytes = firstWork.job_bytes;
        results.secondJobBytes = secondWork.job_bytes;
        results.controlBytesBefore = firstWork.control_bytes;
        Require(firstWork.control_bytes == secondWork.control_bytes);
        for (auto const &task : coordinator.tasks)
        {
            std::lock_guard lock(task->mutex);
            Require(task->svgEncoded && task->svgEncoded->Capacity() == Basic.size() && task->svgCandidate &&
                    task->svgReceipt.engine.job_error_kind == NOTEPADS_MARKDOWN_SVG_JOB_ERROR_NONE &&
                    task->svgCandidate->metadata.decoded_width == 64 &&
                    task->svgCandidate->metadata.decoded_height == 32);
            auto const &image = *task->svgCandidate;
            Require(image.pixels[static_cast<size_t>(16) * image.metadata.stride + 32 * 4 + 3] == 255);
        }
        auto before = coordinator.policy->Snapshot();
        results.activeBefore = before.header.active_requests;
        results.encodedBefore = before.header.encoded_bytes;
        Require(results.activeBefore == 2 && results.encodedBefore == Basic.size() * 2 &&
                before.header.decoded_bytes > 0);
        results.busyBeforeStatus = Busy(coordinator.policy);
        results.thirdBusyBefore = results.busyBeforeStatus == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT &&
                                  coordinator.policy->Snapshot().header.active_requests == 2;

        if (mode == OverlapRetireMode::SourceEpoch)
        {
            ++epoch.source_generation;
            coordinator.policy->Begin(epoch);
        }
        else if (mode == OverlapRetireMode::ProviderEpoch)
        {
            ++epoch.provider_generation;
            coordinator.policy->Begin(epoch);
        }
        else
            coordinator.Retire(); // Native simulation; no XAML Unloaded event is claimed.
        results.sourceGenerationRetired = epoch.source_generation;
        results.providerGenerationRetired = epoch.provider_generation;
        results.activeAfterRetire = coordinator.policy->Snapshot().header.active_requests;
        results.busyAfterStatus = Busy(coordinator.policy);
        results.thirdBusyAfter = results.busyAfterStatus == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT &&
                                 coordinator.policy->Snapshot().header.active_requests == 2;
        auto firstAfter = Work(*first);
        auto secondAfter = Work(*second);
        results.firstJobBytesAfter = firstAfter.job_bytes;
        results.secondJobBytesAfter = secondAfter.job_bytes;
        results.controlBytesAfter = firstAfter.control_bytes;
        results.retirementKeptSlots = coordinator.tasks[0] == first && coordinator.tasks[1] == second &&
                                      results.activeAfterRetire == 2 && TailBlocked(*first) && TailBlocked(*second);
        results.retirementKeptFees =
            firstWork.job_bytes > 0 && secondWork.job_bytes > 0 && firstAfter.job_bytes == firstWork.job_bytes &&
            secondAfter.job_bytes == secondWork.job_bytes && firstAfter.root_work_bytes == firstWork.root_work_bytes &&
            secondAfter.root_work_bytes == secondWork.root_work_bytes &&
            firstAfter.control_bytes == firstWork.control_bytes && secondAfter.control_bytes == firstWork.control_bytes;
        first->Finish();
        second->Finish();
        results.earlyFinishBlocked = !first->finished.load(std::memory_order_acquire) &&
                                     !second->finished.load(std::memory_order_acquire) && first->request &&
                                     second->request && first->svgFinishPending && second->svgFinishPending &&
                                     coordinator.policy->Snapshot().header.active_requests == 2;
        Require(results.originalSlots && results.overlappingWorkers && results.distinctWorkers &&
                results.distinctJobs && results.thirdBusyBefore && results.thirdBusyAfter &&
                results.retirementKeptSlots && results.retirementKeptFees && results.earlyFinishBlocked);

        Require(SetEvent(first->testWorkerTailProceed.get()) != FALSE);
        firstDriver.Join();
        Require(contexts[0].returned && SUCCEEDED(contexts[0].error));
        Require(WaitForSingleObjectEx(first->testWorkerThread.get(), 0, FALSE) == WAIT_OBJECT_0);
        first->NotifyDecodeComplete(); // No IO exists in this direct consumer; its real reaper returned.
        auto afterFirst = coordinator.policy->Snapshot();
        results.activeAfterFirst = afterFirst.header.active_requests;
        results.encodedAfterFirst = afterFirst.header.encoded_bytes;
        results.firstReleaseIndependent = first->finished.load(std::memory_order_acquire) && !first->request &&
                                          results.activeAfterFirst == 1 && results.encodedAfterFirst == Basic.size() &&
                                          TailBlocked(*second) && coordinator.tasks[0] == first &&
                                          coordinator.tasks[1] == second;
        Require(results.firstReleaseIndependent);

        Require(SetEvent(second->testWorkerTailProceed.get()) != FALSE);
        secondDriver.Join();
        Require(contexts[1].returned && SUCCEEDED(contexts[1].error));
        Require(WaitForSingleObjectEx(second->testWorkerThread.get(), 0, FALSE) == WAIT_OBJECT_0);
        second->NotifyDecodeComplete();
        auto afterBoth = coordinator.policy->Snapshot();
        results.activeAfterBoth = afterBoth.header.active_requests;
        results.encodedAfterBoth = afterBoth.header.encoded_bytes;
        results.decodedAfterBoth = afterBoth.header.decoded_bytes;
        auto retired = [](ImageLoadTask const &task)
        {
            auto const &receipt = task.svgReceipt;
            return task.svgRetired && !task.svgQuarantined && task.finished.load(std::memory_order_acquire) &&
                   !task.request && !task.svgState && !task.svgResources && !task.svgAccount && !task.svgCoreCancel &&
                   !task.svgKeepAlive && receipt.threadSignaled && receipt.cancelEntryRevoked &&
                   receipt.cancelLeasesZero && receipt.destroyAttempts == 1 &&
                   receipt.destroyed.cleanup_succeeded == 1 && receipt.engine.cleanup_succeeded == 1 &&
                   receipt.native.retired && receipt.resources.cleanup_succeeded == 1;
        };
        results.bothRetired = retired(*first) && retired(*second);
        results.noReadyAfterRetire = !contexts[0].result->Ready() && !contexts[1].result->Ready() &&
                                     contexts[0].result->nativeStatus == NOTEPADS_MARKDOWN_CORE_CANCELLED &&
                                     contexts[1].result->nativeStatus == NOTEPADS_MARKDOWN_CORE_CANCELLED;
        auto firstMetadata = Metadata(*first);
        auto secondMetadata = Metadata(*second);
        auto firstCost = Total(firstMetadata);
        auto secondCost = Total(secondMetadata);
        Require(firstCost <= (std::numeric_limits<uint64_t>::max)() - secondCost);
        results.remainingTaskMetadataBytes = firstCost + secondCost;
        results.controlBytesAfterBoth = firstMetadata.control_bytes;
        results.transientChargesReleased = results.activeAfterBoth == 0 && results.encodedAfterBoth == 0 &&
                                           results.decodedAfterBoth == 0 && afterBoth.header.reservations == 0 &&
                                           firstMetadata.phase == 2 && secondMetadata.phase == 2 &&
                                           firstMetadata.refs == 1 && secondMetadata.refs == 1 &&
                                           firstMetadata.control_bytes == results.remainingTaskMetadataBytes &&
                                           secondMetadata.control_bytes == results.remainingTaskMetadataBytes;
        Require(results.bothRetired && results.noReadyAfterRetire && results.transientChargesReleased);

        auto firstWeak = std::weak_ptr<ImageLoadTask>{first};
        auto secondWeak = std::weak_ptr<ImageLoadTask>{second};
        contexts[0].Clear();
        contexts[1].Clear();
        coordinator.tasks[0].reset();
        coordinator.tasks[1].reset();
        first.reset();
        second.reset();
        Require(firstWeak.expired() && secondWeak.expired());
        ReadingFreeReceipt taskRelease;
        {
            ReadingFreeScope scope{taskRelease};
            firstWeak.reset();
            secondWeak.reset();
        }
        results.taskMetadataPhysicalFrees = taskRelease.physicalFreed;
        results.taskMetadataRootReleases = taskRelease.rootReleased;
        results.taskMetadataReleased = taskRelease.Succeeded() && taskRelease.attempted == 2 &&
                                       taskRelease.physicalFreed == 2 && taskRelease.rootReleased == 2;
        Require(results.taskMetadataReleased);

        // Reuse the same coordinator, policy and original slot after both real
        // worker/reapers/permits have retired. No replacement scheduler exists.
        ++epoch.source_generation;
        coordinator.policy->Begin(epoch);
        auto successor = ImageLoadTask::Create(memory);
        successor->request = StartImageTestRequest(coordinator.policy, 1, {1, 24, 512, 1},
                                                  NOTEPADS_MARKDOWN_IMAGE_SVG);
        coordinator.tasks[0] = successor;
        auto successorResult = std::shared_ptr<NativeImageDecodeResult>{successor, &successor->decodeResult};
        auto charge = successor->request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, Basic.size());
        auto encoded = NativeEncodedImage::Create(memory, std::move(charge), Basic.size());
        encoded->Append({reinterpret_cast<uint8_t const *>(Basic.data()), Basic.size()});
        RenderSvgImageOnBackground(successor, memory, std::move(encoded), {512, 1, 24, true}, successorResult);
        results.successorReady = successorResult->Ready() && successorResult->svg &&
                                 successorResult->svgPlan.identity.source_generation == epoch.source_generation &&
                                 successorResult->svgPlan.identity.provider_generation == epoch.provider_generation;
        successor->NotifyDecodeComplete();
        successor->Finish();
        Require(results.successorReady && retired(*successor));
        ReadingFreeReceipt successorRelease;
        {
            ReadingFreeScope scope{successorRelease};
            successorResult.reset();
            coordinator.tasks[0].reset();
            successor.reset();
        }
        auto clean = coordinator.policy->Snapshot();
        results.successorPhysicalFrees = successorRelease.physicalFreed;
        results.successorReleased = successorRelease.Succeeded() && successorRelease.physicalFreed == 4 &&
                                    successorRelease.rootReleased == 4 && clean.header.active_requests == 0 &&
                                    clean.header.encoded_bytes == 0 && clean.header.decoded_bytes == 0 &&
                                    clean.header.reservations == 0;
        Require(results.successorReleased);
        results.checks = 1;
        return results;
    }
}
