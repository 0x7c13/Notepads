// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Svg/NativeSvgJob.h"
#include "Resources/Images/NativeImageCoordinator.h"
#include "Resources/Svg/NativeSvgFontSession.h"
#include "notepads/markdown/svg_job.h"
#include <new>
#include <utility>

namespace Notepads::Markdown::Native
{
    namespace
    {
        constexpr uint64_t StackReservation = 8ull * 1024 * 1024;
        // An existing, explicitly unqualified engineering allowance. This is
        // not a complete allocation map, an RSS bound, or a product admission.
        constexpr uint64_t LibraryAllowance = 32ull * 1024 * 1024;

        notepads_markdown_core_error CoreError() noexcept
        {
            return {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, sizeof(notepads_markdown_core_error)};
        }
        template <typename T> T JobPod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_SVG_JOB_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }
        void Check(int32_t status)
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw NativeImageFailure{status};
        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS) || defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
        int32_t TestGate(HANDLE reached, HANDLE proceed) noexcept
        {
            if (!reached && !proceed)
                return NOTEPADS_MARKDOWN_CORE_OK;
            if (!reached || !proceed || !SetEvent(reached))
                return NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
            return WaitForSingleObjectEx(proceed, 10000, FALSE) == WAIT_OBJECT_0 ? NOTEPADS_MARKDOWN_CORE_OK
                                                                                 : NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
        }
#endif
        void Failure(NativeImageDecodeResult &result, int32_t status, HRESULT hr = S_OK) noexcept
        {
            if (result.nativeStatus != NOTEPADS_MARKDOWN_CORE_OK || FAILED(result.error))
                return;
            result.nativeStatus = status;
            result.error = FAILED(hr) ? hr : NativeStatusHresult(status).value;
            result.failure = status == NOTEPADS_MARKDOWN_CORE_CANCELLED || status == NOTEPADS_MARKDOWN_CORE_STALE_RESULT ? NOTEPADS_MARKDOWN_IMAGE_CANCELLED
                             : status == NOTEPADS_MARKDOWN_CORE_UNSUPPORTED                               ? NOTEPADS_MARKDOWN_IMAGE_UNSUPPORTED
                             : status == NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY || status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT ||
                                     status == NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW
                                 ? NOTEPADS_MARKDOWN_IMAGE_QUOTA
                                 : NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR;
        }
        void Quarantine(ImageLoadTask &task, NativeImageDecodeResult &result, int32_t status,
                        HRESULT hr = S_OK) noexcept
        {
            std::lock_guard lock(task.mutex);
            task.svgQuarantined = true;
            Failure(result, status, hr);
            // A successful render cannot mask a failed retirement receipt.
            if (SUCCEEDED(result.closeError))
                result.closeError = FAILED(hr) ? hr : NativeStatusHresult(status).value;
        }
        constexpr uint32_t FaultDetail(notepads_markdown_svg_fault const &fault) noexcept
        {
            return ((fault.kind >= 1 && fault.kind <= 7) || (fault.kind >= 16 && fault.kind <= 24)) &&
                           fault.limit <= 15
                       ? (fault.kind << 16) | fault.limit
                       : 0;
        }
        static_assert(FaultDetail(notepads_markdown_svg_fault{2, 14, 0, 0}) == ((2u << 16) | 14u));
        static_assert(FaultDetail(notepads_markdown_svg_fault{2, 15, 0, 0}) == ((2u << 16) | 15u));
        static_assert(FaultDetail(notepads_markdown_svg_fault{2, 16, 0, 0}) == 0);
        static_assert(FaultDetail(notepads_markdown_svg_fault{8, 14, 0, 0}) == 0);

        void WorkFailure(notepads_markdown_svg_resources *resources, NativeImageDecodeResult &result, int32_t status,
                         HRESULT hr = S_OK) noexcept
        {
            uint32_t detail{};
            // Preserve a Native HRESULT recorded before the transport error.
            if (resources)
            {
                notepads_markdown_svg_fault fault{};
                auto error = CoreError();
                if (notepads_markdown_svg_resources_read_fault(resources, &fault, &error) == NOTEPADS_MARKDOWN_CORE_OK)
                {
                    if (fault.native_hr_present && FAILED(fault.native_hr))
                        hr = fault.native_hr;
                    detail = FaultDetail(fault);
                }
            }
            auto first = result.nativeStatus == NOTEPADS_MARKDOWN_CORE_OK && SUCCEEDED(result.error);
            Failure(result, status, hr);
            if (first)
                result.detail = detail;
        }
    }

    // One exact HeapAlloc block, paid before allocation by the original work
    // account. No queue, replacement task, retry record or hidden shared owner.
    struct NativeSvgJobState final
    {
        ImageLoadTask *task{};
        // The worker never writes its caller's result. A failed wait cannot
        // prove exit; its still-running work must stay isolated from outer IO
        // close/error handling even though the original task pins both owners.
        NativeImageDecodeResult workerResult;
        ReadingFreeReceipt workerFree;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        NativeSvgJobTestFault testFault{NativeSvgJobTestFault::None};
#endif
        NativeSvgImageTarget target;
        notepads_markdown_svg_resources *resources{};
        notepads_markdown_svg_job *job{};
        notepads_markdown_svg_job_cancel *cancel{};
        notepads_markdown_svg_job_identity identity{};
        notepads_markdown_svg_job_plan plan{};
        notepads_markdown_svg_job_retirement engineRetirement{};
        notepads_markdown_svg_job_destroyed destroyed{};
        NativeSvgFontSession fonts;
        MsSvgFontCallbacks callbacks{};
        NativeSvgFontRetirement fontRetirement;
        NativeSvgBorrowSet encodedBorrows{nullptr};
        NativeSvgBorrowSet taskBorrows{nullptr};
        NativeSvgImageAllocation allocation{nullptr, NativeSvgBorrowSet{nullptr}, nullptr};
        notepads_markdown_svg_work_token nodeToken{}, stackToken{}, libraryToken{};
        HANDLE thread{}, cancelIdle{};
        uint64_t stackReserved{}, stackCommitted{}, stackGuard{}, stackHeadroom{};
        uint32_t workerThread{}, reaperThread{}, cancelCalls{};
        bool cancelEntry{}, fontAttempted{}, engineRetired{}, fontRetired{};
        bool stackReservedToken{}, libraryReservedToken{}, stackPaid{}, libraryPaid{}, rendered{}, threadSignaled{};
    };

    namespace
    {
        // Only worker-produced facts. Exit/reaper/destroy observations are
        // deliberately absent until the unique reaper proves real signaling.
        void CopyWorkerReceiptsLocked(ImageLoadTask &task, NativeSvgJobState const &node) noexcept
        {
            task.svgReceipt.engine = node.engineRetirement;
            task.svgReceipt.native = node.fontRetirement;
            task.svgReceipt.coverage = node.fonts.CoverageStatistics();
            task.svgReceipt.workerFree = node.workerFree;
            task.svgReceipt.stackReserved = node.stackReserved;
            task.svgReceipt.stackCommitted = node.stackCommitted;
            task.svgReceipt.stackGuard = node.stackGuard;
            task.svgReceipt.stackHeadroom = node.stackHeadroom;
            task.svgReceipt.workerThread = node.workerThread;
        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        class MemoryHookScope final
        {
          public:
            explicit MemoryHookScope(ReadingMemoryTestHooks *hooks) noexcept : _active(hooks != nullptr)
            {
                if (_active)
                    _old = ReadingMemory::SetTestHooks(hooks);
            }
            explicit MemoryHookScope(ReadingMemoryTestHooks &hooks) noexcept : MemoryHookScope(&hooks)
            {
            }
            ~MemoryHookScope()
            {
                if (_active)
                    ReadingMemory::SetTestHooks(_old);
            }

          private:
            ReadingMemoryTestHooks *_old{};
            bool _active{};
        };
        struct FreeFault
        {
            ReadingMemoryTestHooks hooks;
            bool armed{};
            static void Observe(void *context, void *, ReadingMemoryTestEvent event) noexcept
            {
                auto &fault = *static_cast<FreeFault *>(context);
                if (!fault.armed && event == ReadingMemoryTestEvent::CallbackCompleted)
                {
                    fault.armed = true;
                    fault.hooks.failNextFree = true;
                }
            }
        };

#endif
        bool DiscardUnpublished(ImageLoadTask &task, NativeImageDecodeResult &result, NativeSvgJobState *node) noexcept
        {
            auto &receipt = task.svgReceipt.discardFree;
            ReadingFreeScope scope{receipt};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            FreeFault fault;
            auto pixels = task.svgCandidate ? task.svgCandidate.get() : node ? node->allocation.pixels.get() : nullptr;
            if (node && node->testFault == NativeSvgJobTestFault::PayloadFree && pixels)
            {
                fault.hooks.target = pixels->MutablePixels().data();
                fault.hooks.failNextFree = true;
            }
            else if (node && node->testFault == NativeSvgJobTestFault::MetadataFree)
            {
                fault.hooks.context = &fault;
                fault.hooks.observe = FreeFault::Observe;
            }
#endif
            {
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                auto inject = node && (node->testFault == NativeSvgJobTestFault::PayloadFree ||
                                       node->testFault == NativeSvgJobTestFault::MetadataFree);
                MemoryHookScope hooks{inject ? &fault.hooks : nullptr};
#endif
                if (node)
                    node->allocation.pixels.reset();
                task.svgCandidate.reset();
            }
            bool lastEncoded{};
            if (receipt.Succeeded())
            {
                NativeEncodedImageHandle encoded;
                {
                    std::lock_guard lock(task.mutex);
                    task.svgEncodedShareable = false;
                    encoded = std::move(task.svgEncoded);
                }
                lastEncoded = encoded.ResetAndWasLast();
            }
            else
            {
                std::lock_guard lock(task.mutex);
                task.svgEncodedShareable = false;
            }
            auto status = receipt.Succeeded() ? NOTEPADS_MARKDOWN_CORE_OK : receipt.firstStatus;
            if (status == NOTEPADS_MARKDOWN_CORE_OK && node)
                status = node->allocation.borrows.VerifyPhysicalRetired();
            if (status == NOTEPADS_MARKDOWN_CORE_OK && node && lastEncoded)
                status = node->encodedBorrows.VerifyPhysicalRetired();
            // Keep every original row/prefix on failure. Physical-freed records
            // remain safely queryable through the still-live creator borrow.
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
            {
                Quarantine(task, result, status);
                return false;
            }
            if (node)
            {
                status = node->allocation.borrows.Return();
                if (status == NOTEPADS_MARKDOWN_CORE_OK)
                    status = node->encodedBorrows.Return();
            }
            if (status != NOTEPADS_MARKDOWN_CORE_OK || !receipt.Succeeded())
            {
                Quarantine(task, result, status != NOTEPADS_MARKDOWN_CORE_OK ? status : receipt.firstStatus);
                return false;
            }
            task.svgEncodedCharge.reset();
            return true;
        }
        // Record the actual OS reservation, commit, guard and usable entry
        // headroom. Guard growth remains an explicit Windows platform boundary.
        bool InspectStack(NativeSvgJobState &node, uintptr_t marker) noexcept
        {
            ULONG_PTR lower{}, upper{};
            GetCurrentThreadStackLimits(&lower, &upper);
            MEMORY_BASIC_INFORMATION initial{};
            if (lower >= upper || marker < lower || marker >= upper ||
                VirtualQuery(reinterpret_cast<void const *>(marker), &initial, sizeof(initial)) != sizeof(initial))
                return false;
            auto base = reinterpret_cast<uintptr_t>(initial.AllocationBase);
            if (!base || base > lower || base > marker)
                return false;
            uintptr_t cursor = base, writableLower{};
            bool found{};
            while (cursor < upper)
            {
                MEMORY_BASIC_INFORMATION region{};
                if (VirtualQuery(reinterpret_cast<void const *>(cursor), &region, sizeof(region)) != sizeof(region))
                    return false;
                auto start = reinterpret_cast<uintptr_t>(region.BaseAddress);
                if (region.AllocationBase != initial.AllocationBase || start != cursor || !region.RegionSize ||
                    region.RegionSize > UINTPTR_MAX - start || start + region.RegionSize > upper)
                    return false;
                auto end = start + region.RegionSize;
                if (region.State == MEM_COMMIT)
                {
                    node.stackCommitted += region.RegionSize;
                    if ((region.Protect & PAGE_GUARD) != 0)
                        node.stackGuard += region.RegionSize;
                }
                if (region.State == MEM_COMMIT && region.Protect == PAGE_READWRITE)
                {
                    if (!writableLower)
                        writableLower = (std::max)(start, static_cast<uintptr_t>(lower));
                }
                else
                    writableLower = 0;
                if (marker >= start && marker < end)
                {
                    found = writableLower && marker >= writableLower;
                    if (found)
                        node.stackHeadroom = marker - writableLower;
                }
                cursor = end;
            }
            node.stackReserved = cursor - base;
            return found && node.stackHeadroom && node.stackReserved == StackReservation &&
                   node.stackCommitted <= node.stackReserved;
        }

        DWORD WINAPI Worker(void *context) noexcept
        {
            auto &node = *static_cast<NativeSvgJobState *>(context);
            auto &task = *node.task;
            auto &result = node.workerResult;
            ReadingFreeScope freeScope{node.workerFree};
            node.workerThread = GetCurrentThreadId();
            volatile uint8_t marker{};
            auto apartment = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
            try
            {
                if (FAILED(apartment))
                    throw winrt::hresult_error{apartment};
                if (!InspectStack(node, reinterpret_cast<uintptr_t>(&marker)))
                    throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT};

                node.encodedBorrows = NativeSvgBorrowSet{node.resources};
                if (task.svgEncoded->RetainedHandle())
                    task.svgEncoded->BorrowSvg(node.encodedBorrows);
                else
                    task.svgEncoded->AdoptSvg(node.encodedBorrows);
                {
                    std::lock_guard lock(task.mutex);
                    task.svgEncodedShareable = true;
                }
                node.fontAttempted = true;
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
                if (task.svgQualityConfig)
                    winrt::check_hresult(NativeSvgFontSession::CreateForQuality(
                        node.resources, *task.svgQualityConfig, task.svgEncoded->Bytes(), &task.svgQualityTrace,
                        &node.fonts));
                else
#endif
                winrt::check_hresult(NativeSvgFontSession::Create(node.resources, &node.fonts));
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
                SvgCoverageTestHooks coverageHooks;
                {
                    std::lock_guard lock(task.mutex);
                    coverageHooks.failure_point = task.svgCoverageTestFailurePoint;
                    coverageHooks.retain_byte_owner = task.svgCoverageTestRetainOwner ? 1u : 0u;
                    if (task.testCoverageFontReached || task.testCoverageFontProceed)
                    {
                        coverageHooks.context = &task;
                        coverageHooks.font_ready = [](void *context, std::span<uint8_t const>,
                                                      std::span<SvgCoverageGlyph const>, IDWriteFontFace *)
                        {
                            auto &owner = *static_cast<ImageLoadTask *>(context);
                            if (TestGate(owner.testCoverageFontReached.get(), owner.testCoverageFontProceed.get()) !=
                                NOTEPADS_MARKDOWN_CORE_OK)
                                throw E_UNEXPECTED;
                        };
                    }
                }
                node.fonts.SetCoverageTestHooks(coverageHooks);
#endif
                node.callbacks = node.fonts.Callbacks();
                node.identity.font_generation = node.target.fontGeneration;
                auto error = JobPod<notepads_markdown_svg_job_error>();
                // A returned nonnull failed-initialization owner also reaches
                // the same mandatory worker finish below.
                node.job = nullptr;
                node.cancel = nullptr;
                auto coverage = node.fonts.CoverageCallbacks();
                auto status = notepads_markdown_svg_job_create(NOTEPADS_MARKDOWN_SVG_JOB_ABI_VERSION, node.resources, &node.callbacks,
                                                              &coverage, &node.identity, &node.job, &node.cancel, &error);
                if (node.job)
                {
                    std::lock_guard lock(task.mutex);
                    node.cancelEntry = node.cancel != nullptr;
                }
                Check(status);
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
                if (task.svgQualityConfig && task.svgQualityConfig->engineTraceCapacity)
                {
                    auto &observation = task.svgQualityObservation;
                    observation.installStatus = notepads_markdown_svg_job_quality_enable(
                        node.job, task.svgQualityConfig->engineTraceCapacity, &error);
                    observation.installed = observation.installStatus == NOTEPADS_MARKDOWN_CORE_OK;
                    Check(observation.installStatus);
                }
#endif
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                if (task.testJobCreated)
                {
                    Check(SetEvent(task.testJobCreated.get()) ? NOTEPADS_MARKDOWN_CORE_OK : NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
                    Check(task.testLeaseRetained &&
                                  WaitForSingleObjectEx(task.testLeaseRetained.get(), 10000, FALSE) == WAIT_OBJECT_0
                              ? NOTEPADS_MARKDOWN_CORE_OK
                              : NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
                }
#endif
                auto natural = JobPod<notepads_markdown_svg_job_natural>();
                auto source = task.svgEncoded->Bytes();
                Check(notepads_markdown_svg_job_prepare(node.job, source.data(), source.size(), &natural, &error));
                auto target = JobPod<notepads_markdown_svg_job_target>();
                target.display_width_dip = (std::min)(node.target.maximumWidthDip, natural.width_dip);
                target.effective_physical_scale = node.target.effectivePhysicalScale;
                target.generation = node.target.generation;
                node.plan = JobPod<notepads_markdown_svg_job_plan>();
                Check(notepads_markdown_svg_job_set_target(node.job, &target, &node.plan, &error));
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                if (node.testFault == NativeSvgJobTestFault::PartialMetadata)
                {
                    ReadingMemoryTestHooks hooks;
                    hooks.failNextSize = true;
                    hooks.failNextFree = true;
                    MemoryHookScope hookScope{hooks};
                    CreateSvgImagePixels(task.request, task.svgMemory, node.resources, node.plan.capacity,
                                         node.allocation);
                }
                else
#endif
                    CreateSvgImagePixels(task.request, task.svgMemory, node.resources, node.plan.capacity,
                                         node.allocation);
                {
                    std::lock_guard lock(task.mutex);
                    task.svgCandidate = node.allocation.pixels;
                }
                auto &pixels = *node.allocation.pixels;
                pixels.metadata = MakeSvgImageMetadata(node.plan);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                if (node.testFault == NativeSvgJobTestFault::PayloadFree ||
                    node.testFault == NativeSvgJobTestFault::MetadataFree)
                    CancelSvgImageTask(task);
#endif
                Check(notepads_markdown_svg_job_run(node.job, &node.plan.identity, pixels.MutablePixels().data(), node.plan.capacity,
                                     node.plan.stride, &error));
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
                if (task.svgQualityConfig)
                    winrt::check_hresult(node.fonts.ValidateQualityComplete());
#endif
                node.rendered = true;
            }
            catch (NativeFailure const &error)
            {
                WorkFailure(node.resources, result, error.status, error.code());
            }
            catch (winrt::hresult_error const &error)
            {
                WorkFailure(node.resources, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, error.code());
            }
            catch (std::bad_alloc const &)
            {
                Failure(result, NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY);
            }
            catch (...)
            {
                Failure(result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
            }

            // No cancellation checkpoint may bypass either actual cleanup.
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
            if (node.job && task.svgQualityObservation.installed)
            {
                auto &observation = task.svgQualityObservation;
                observation.info.header = {NOTEPADS_MARKDOWN_SVG_JOB_ABI_VERSION, sizeof(MsSvgQualityTraceInfo)};
                auto diagnosticError = JobPod<notepads_markdown_svg_job_error>();
                observation.infoStatus = notepads_markdown_svg_job_quality_info(node.job, &observation.info, &diagnosticError);
                if (observation.infoStatus == NOTEPADS_MARKDOWN_CORE_OK)
                {
                    observation.copyStatus = notepads_markdown_svg_job_quality_copy(node.job, observation.bytes.data(),
                        observation.bytes.size(), &observation.written, &diagnosticError);
                    observation.copied = observation.copyStatus == NOTEPADS_MARKDOWN_CORE_OK &&
                        observation.written == observation.info.bytes;
                }
                if ((!observation.copied || observation.info.traceError) && SUCCEEDED(result.error))
                {
                    node.rendered = false;
                    auto status = observation.infoStatus != NOTEPADS_MARKDOWN_CORE_OK ? observation.infoStatus
                        : observation.copyStatus != NOTEPADS_MARKDOWN_CORE_OK ? observation.copyStatus
                        : observation.info.traceError == 1 ? NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT : NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                    WorkFailure(node.resources, result, status);
                }
            }
#endif
            if (node.job)
            {
                node.engineRetirement = JobPod<notepads_markdown_svg_job_retirement>();
                auto error = JobPod<notepads_markdown_svg_job_error>();
                auto status = notepads_markdown_svg_job_finish(node.job, &node.engineRetirement, &error);
                node.engineRetired = status == NOTEPADS_MARKDOWN_CORE_OK && node.engineRetirement.cleanup_succeeded == 1 &&
                                     node.engineRetirement.remaining_owners == 0 &&
                                     node.engineRetirement.failed_releases == 0;
                if (!node.engineRetired)
                    Quarantine(task, result, status == NOTEPADS_MARKDOWN_CORE_OK ? NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT : status);
            }
            else
                node.engineRetired = true;
            if (node.fontAttempted)
            {
                auto hr = node.fonts.FinishOnWorker(&node.fontRetirement);
                node.fontRetired = SUCCEEDED(hr) && node.fontRetirement.retired &&
                                   node.fontRetirement.remainingNativeBytes == 0 &&
                                   node.fontRetirement.failedReleases == 0;
                if (!node.fontRetired)
                    Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, FAILED(hr) ? hr : E_UNEXPECTED);
            }
            else
                node.fontRetired = true;
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
            {
                std::lock_guard lock(task.mutex);
                task.svgCoverageTestRetainedOwner = node.fonts.TakeRetainedCoverageTestOwner();
            }
#endif
            if (SUCCEEDED(apartment))
                CoUninitialize();
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            if (task.testWorkerTailReached || task.testWorkerTailProceed)
            {
                {
                    std::lock_guard lock(task.mutex);
                    CopyWorkerReceiptsLocked(task, node);
                }
                // Fixed test-only tail: all COM/engine cleanup has happened,
                // but this real worker still owns its stack and has not exited.
                auto status = TestGate(task.testWorkerTailReached.get(), task.testWorkerTailProceed.get());
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                    Quarantine(task, result, status);
            }
#endif
            return 0;
        }

        bool ReleaseResource(ImageLoadTask &task, NativeImageDecodeResult &result, notepads_markdown_svg_work_token token) noexcept
        {
            auto error = CoreError();
            auto status = task.svgResources ? notepads_markdown_svg_resources_release_after_free(task.svgResources, &token, &error)
                                            : notepads_markdown_svg_work_release_after_free(task.svgAccount, &token, &error);
            if (status == NOTEPADS_MARKDOWN_CORE_OK)
                return true;
            task.svgFailedToken = token;
            Quarantine(task, result, status);
            return false;
        }

        bool RollbackBeforeThread(ImageLoadTask &task, NativeImageDecodeResult &result) noexcept
        {
            auto node = task.svgState;
            // An existing suspended/running thread is never terminated or
            // discarded to turn a failed start into a free physical slot.
            if (!node || node->thread || node->fontAttempted || node->job)
                return false;
            if (node->cancelIdle)
            {
                if (!CloseHandle(node->cancelIdle))
                {
                    Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, HRESULT_FROM_WIN32(GetLastError()));
                    return false;
                }
                node->cancelIdle = nullptr;
            }
            // These allocations provably never happened. Pending tokens can
            // be returned once; a failed release is not retried by a destructor.
            if (node->stackReservedToken && !ReleaseResource(task, result, node->stackToken))
                return false;
            node->stackReservedToken = false;
            if (node->libraryReservedToken && !ReleaseResource(task, result, node->libraryToken))
                return false;
            node->libraryReservedToken = false;
            if (node->taskBorrows.Return() != NOTEPADS_MARKDOWN_CORE_OK)
            {
                Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
                return false;
            }
            auto token = node->nodeToken;
            {
                std::lock_guard lock(task.mutex);
                task.svgState = nullptr;
                task.svgFailedNode = node;
                task.svgFailedToken = token;
            }
            node->~NativeSvgJobState();
            if (!HeapFree(GetProcessHeap(), 0, node))
            {
                Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, HRESULT_FROM_WIN32(GetLastError()));
                return false;
            }
            {
                std::lock_guard lock(task.mutex);
                task.svgFailedNode = nullptr;
                task.svgReceipt.nodeFreed = true;
            }
            if (!ReleaseResource(task, result, token))
                return false;
            task.svgFailedToken = {};
            return true;
        }

        bool Reap(ImageLoadTask &task, NativeImageDecodeResult &result) noexcept
        {
            auto node = task.svgState;
            node->reaperThread = GetCurrentThreadId();
            // This existing background continuation is the unique reaper. It
            // waits for at most one of the two original tasks, with no new queue.
            auto waited = WaitForSingleObjectEx(node->thread, INFINITE, FALSE);
            if (waited != WAIT_OBJECT_0 || WaitForSingleObjectEx(node->thread, 0, FALSE) != WAIT_OBJECT_0)
            {
                Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, HRESULT_FROM_WIN32(GetLastError()));
                return false;
            }
            node->threadSignaled = true;
            auto const &workerResult = node->workerResult;
            if (workerResult.nativeStatus != NOTEPADS_MARKDOWN_CORE_OK || FAILED(workerResult.error))
            {
                auto first = result.nativeStatus == NOTEPADS_MARKDOWN_CORE_OK && SUCCEEDED(result.error);
                Failure(result, workerResult.nativeStatus, workerResult.error);
                if (first)
                    result.detail = workerResult.detail;
            }
            if (FAILED(workerResult.closeError) && SUCCEEDED(result.closeError))
                result.closeError = workerResult.closeError;
            {
                std::lock_guard lock(task.mutex);
                CopyWorkerReceiptsLocked(task, *node);
                task.svgReceipt.reaperThread = node->reaperThread;
                task.svgReceipt.threadSignaled = true;
                node->cancelEntry = false;
                task.svgReceipt.cancelEntryRevoked = true;
            }
            if (node->job)
            {
                auto status = notepads_markdown_svg_job_close_cancel(node->job);
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    Quarantine(task, result, status);
                    return false;
                }
            }
            if (!node->workerFree.Succeeded())
            {
                Quarantine(task, result, node->workerFree.firstStatus);
                return false;
            }
            // ResetEvent/increment and last release/SetEvent use the same gate;
            // a lease which was retained before revocation cannot lose a wake.
            for (;;)
            {
                {
                    std::lock_guard lock(task.mutex);
                    if (task.svgQuarantined)
                        return false;
                    if (!node->cancelCalls)
                    {
                        task.svgReceipt.cancelLeasesZero = true;
                        break;
                    }
                }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                if (task.testReaperWaitingLease && !SetEvent(task.testReaperWaitingLease.get()))
                {
                    Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
                    return false;
                }
#endif
                if (WaitForSingleObjectEx(node->cancelIdle, INFINITE, FALSE) != WAIT_OBJECT_0)
                {
                    Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, HRESULT_FROM_WIN32(GetLastError()));
                    return false;
                }
            }
            if (!node->engineRetired || !node->fontRetired)
                return false;
            if (node->job)
            {
                node->destroyed = JobPod<notepads_markdown_svg_job_destroyed>();
                auto error = JobPod<notepads_markdown_svg_job_error>();
                {
                    std::lock_guard lock(task.mutex);
                    ++task.svgReceipt.destroyAttempts;
                }
                auto status = notepads_markdown_svg_job_destroy(&node->job, &node->destroyed, &error);
                task.svgReceipt.destroyed = node->destroyed;
                if (status != NOTEPADS_MARKDOWN_CORE_OK || node->destroyed.consumed != 1 || node->destroyed.cleanup_succeeded != 1)
                {
                    Quarantine(task, result, status == NOTEPADS_MARKDOWN_CORE_OK ? NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT : status);
                    return false;
                }
                if (node->destroyed.cancelled)
                    Failure(result, NOTEPADS_MARKDOWN_CORE_CANCELLED);
                node->cancel = nullptr;
            }
            if (!CloseHandle(node->thread))
            {
                Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, HRESULT_FROM_WIN32(GetLastError()));
                return false;
            }
            node->thread = nullptr;
            if (!ReleaseResource(task, result, node->stackToken))
                return false;
            node->stackPaid = false;
            if (!CloseHandle(node->cancelIdle))
            {
                Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, HRESULT_FROM_WIN32(GetLastError()));
                return false;
            }
            node->cancelIdle = nullptr;
            if (!node->rendered)
                Failure(result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            auto before = TestGate(task.testBeforeCommitReached.get(), task.testBeforeCommitProceed.get());
            if (before != NOTEPADS_MARKDOWN_CORE_OK)
            {
                Quarantine(task, result, before);
                return false;
            }
#endif
            {
                std::lock_guard lock(task.mutex);
                if (task.request->IsCancelled() || !task.request->IsCurrent())
                    Failure(result, NOTEPADS_MARKDOWN_CORE_CANCELLED);
                // Owner-handoff linearization point, not UI publication. A
                // cancellation before this gate discards under the original
                // borrow/account. After it, fully retired owners move to the
                // independent result even if a late host check rejects UI.
                // Any later retirement failure still pins all owners here.
                task.svgOwnerHandoffCommitted =
                    result.nativeStatus == NOTEPADS_MARKDOWN_CORE_OK && SUCCEEDED(result.error) && SUCCEEDED(result.closeError);
            }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            if (task.svgOwnerHandoffCommitted)
            {
                auto after = TestGate(task.testAfterCommitReached.get(), task.testAfterCommitProceed.get());
                if (after != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    Quarantine(task, result, after);
                    return false;
                }
            }
#endif
            if (!task.svgOwnerHandoffCommitted && !DiscardUnpublished(task, result, node))
                return false;
            if (node->allocation.borrows.Return() != NOTEPADS_MARKDOWN_CORE_OK || node->encodedBorrows.Return() != NOTEPADS_MARKDOWN_CORE_OK)
            {
                Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
                return false;
            }
            if (node->taskBorrows.Return() != NOTEPADS_MARKDOWN_CORE_OK)
            {
                Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
                return false;
            }
            if (!ReleaseResource(task, result, node->libraryToken))
                return false;
            node->libraryPaid = false;
            result.svg = true;
            result.svgPlan = node->plan;
            auto token = node->nodeToken;
            // The original task pins every retained candidate across actual
            // node free, including HeapFree or subsequent refund failure.
            {
                std::lock_guard lock(task.mutex);
                task.svgState = nullptr;
                task.svgFailedNode = node;
                task.svgFailedToken = token;
            }
            node->~NativeSvgJobState();
            if (!HeapFree(GetProcessHeap(), 0, node))
            {
                Quarantine(task, result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, HRESULT_FROM_WIN32(GetLastError()));
                return false;
            }
            {
                std::lock_guard lock(task.mutex);
                task.svgFailedNode = nullptr;
                task.svgReceipt.nodeFreed = true;
            }
            if (!ReleaseResource(task, result, token))
                return false;
            task.svgFailedToken = {};
            return true;
        }

        bool RetireAccount(ImageLoadTask &task, NativeImageDecodeResult &result) noexcept
        {
            if (task.svgResources)
            {
                notepads_markdown_svg_resources_retirement receipt{};
                receipt.header = {NOTEPADS_MARKDOWN_SVG_RESOURCES_ABI_VERSION, sizeof(receipt)};
                auto error = CoreError();
                auto status = notepads_markdown_svg_resources_reclaim(&task.svgResources, &receipt, &error);
                task.svgReceipt.resources = receipt;
                if (receipt.consumed)
                    task.svgAccount = receipt.account;
                if (status != NOTEPADS_MARKDOWN_CORE_OK || receipt.consumed != 1 || receipt.cleanup_succeeded != 1)
                {
                    Quarantine(task, result, status == NOTEPADS_MARKDOWN_CORE_OK ? NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT : status);
                    return false;
                }
            }
            if (task.svgAccount)
            {
                auto error = CoreError();
                auto status = notepads_markdown_svg_work_close(task.svgAccount, &error);
                if (status == NOTEPADS_MARKDOWN_CORE_OK)
                    status = notepads_markdown_svg_work_destroy(task.svgAccount, &error);
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    Quarantine(task, result, status);
                    return false;
                }
                task.svgAccount = nullptr;
            }
            {
                std::lock_guard lock(task.mutex);
                if (task.svgCoreCancel)
                {
                    auto status = notepads_markdown_core_cancel_destroy(task.svgCoreCancel);
                    if (status != NOTEPADS_MARKDOWN_CORE_OK)
                    {
                        task.svgQuarantined = true;
                        Failure(result, status);
                        result.closeError = NativeStatusHresult(status).value;
                        return false;
                    }
                    task.svgCoreCancel = nullptr;
                }
            }
            return true;
        }
    }

    bool RetireUnstartedSvgInput(std::shared_ptr<ImageLoadTask> const &task) noexcept
    {
        if (!task)
            return true;
        NativeEncodedImageHandle encoded;
        {
            std::lock_guard lock(task->mutex);
            if (task->svgStarted)
                return task->svgRetired && !task->svgQuarantined;
            if (task->svgQuarantined)
                return false;
            if (!task->svgEncoded)
                return !task->svgKeepAlive;
            // The original canonical task remains the quarantine owner even
            // while its last source reference is physically being released.
            task->svgKeepAlive = task;
            task->svgEncodedShareable = false;
            encoded = std::move(task->svgEncoded);
        }
        ReadingFreeReceipt receipt;
        bool last{};
        {
            ReadingFreeScope scope{receipt};
            last = encoded.ResetAndWasLast();
        }
        // No borrow exists here: checked callback success is the receipt, and
        // the retained H may already be gone. Never read its former pointer.
        auto status = receipt.Succeeded() ? NOTEPADS_MARKDOWN_CORE_OK : receipt.firstStatus;
        if (status == NOTEPADS_MARKDOWN_CORE_OK &&
            (last ? receipt.attempted != 1 || receipt.physicalFreed != 1 || receipt.rootReleased != 1
                  : receipt.attempted != 0))
            status = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
        {
            std::lock_guard lock(task->mutex);
            task->svgReceipt.discardFree = receipt;
        }
        if (status != NOTEPADS_MARKDOWN_CORE_OK)
        {
            Quarantine(*task, task->decodeResult, status);
            return false;
        }
        task->svgEncodedCharge.reset();
        std::shared_ptr<ImageLoadTask> retiredPin;
        {
            std::lock_guard lock(task->mutex);
            retiredPin = std::move(task->svgKeepAlive);
        }
        return true;
    }

    bool IsSvgImageTaskRetiredLocked(ImageLoadTask const &task) noexcept
    {
        return !task.svgQuarantined && (task.svgStarted ? task.svgRetired : !task.svgEncoded && !task.svgKeepAlive);
    }

    void CancelSvgImageTask(ImageLoadTask &task) noexcept
    {
        NativeSvgJobState *node{};
        notepads_markdown_svg_job_cancel *cancel{};
        {
            std::lock_guard lock(task.mutex);
            if (task.request)
                task.request->Cancel();
            if (!task.svgStarted || task.svgRetired || task.svgQuarantined)
                return;
            node = task.svgState;
            if (!node || !node->cancelEntry)
            {
                // Before sibling publication the same task gate protects the
                // original token. Publication clones that exact token, so a
                // cancellation in this gap is not lost. After entry revocation
                // the worker has exited; the UI/request flag is sufficient.
                if (task.svgCoreCancel && (!node || !task.svgReceipt.cancelEntryRevoked))
                {
                    auto status = notepads_markdown_core_cancel_request(task.svgCoreCancel);
                    if (status != NOTEPADS_MARKDOWN_CORE_OK)
                    {
                        task.svgReceipt.cancellationStatus = status;
                        task.svgQuarantined = true;
                    }
                }
                return;
            }
            if (!node->cancel)
            {
                task.svgReceipt.cancellationStatus = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                task.svgQuarantined = true;
                return;
            }
            cancel = node->cancel;
            auto retained = notepads_markdown_svg_job_cancel_retain(cancel);
            if (retained != NOTEPADS_MARKDOWN_CORE_OK)
            {
                task.svgReceipt.cancellationStatus = retained;
                task.svgQuarantined = true;
                return;
            }
            ++node->cancelCalls;
            task.svgReceipt.activeCancelCalls = node->cancelCalls;
            if (!ResetEvent(node->cancelIdle))
            {
                task.svgReceipt.cancellationError = HRESULT_FROM_WIN32(GetLastError());
                task.svgQuarantined = true;
            }
        }
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
        auto gate = TestGate(task.testLeaseRetained.get(), task.testLeaseProceed.get());
#endif
        auto requested = notepads_markdown_svg_job_cancel_request(cancel);
        auto released = notepads_markdown_svg_job_cancel_release(cancel);
        {
            std::lock_guard lock(task.mutex);
            if (
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                gate != NOTEPADS_MARKDOWN_CORE_OK ||
#endif
                requested != NOTEPADS_MARKDOWN_CORE_OK || released != NOTEPADS_MARKDOWN_CORE_OK)
            {
                if (task.svgReceipt.cancellationStatus == NOTEPADS_MARKDOWN_CORE_OK)
                    task.svgReceipt.cancellationStatus =
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
                        gate != NOTEPADS_MARKDOWN_CORE_OK ? gate :
#endif
                        requested != NOTEPADS_MARKDOWN_CORE_OK ? requested : released;
                task.svgQuarantined = true;
            }
            --node->cancelCalls;
            task.svgReceipt.activeCancelCalls = node->cancelCalls;
            if (!node->cancelCalls && !SetEvent(node->cancelIdle))
            {
                if (SUCCEEDED(task.svgReceipt.cancellationError))
                    task.svgReceipt.cancellationError = HRESULT_FROM_WIN32(GetLastError());
                task.svgQuarantined = true;
            }
        }
    }

    void RenderSvgImageOnBackground(std::shared_ptr<ImageLoadTask> task, std::shared_ptr<ReadingMemory> memory,
                                    NativeEncodedImageHandle encoded, NativeSvgImageTarget target,
                                    std::shared_ptr<NativeImageDecodeResult> result)
    {
        // DecodeImageAsync already resumed in the background and established
        // its MTA. Do not suspend again: a canceled parent's await_transform can
        // otherwise skip awaiting this action and race its still-live result.
        if (!result)
            return;
        bool claimed{};
        try
        {
            if (!task)
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            {
                std::lock_guard lock(task->mutex);
                if (task->svgStarted || task->finished.load(std::memory_order_acquire) || !task->request ||
                    !task->allocationBlock || !task->allocationBytes || (encoded && task->svgEncoded))
                    throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
                task->svgStarted = true;
                claimed = true;
                task->svgKeepAlive = task;
                task->svgMemory = memory;
                task->svgResult = result;
                if (encoded)
                    task->svgEncoded = std::move(encoded);
                task->svgEncodedCharge = task->svgEncoded.Charge();
                task->svgReceipt.nodeBytes = sizeof(NativeSvgJobState);
            }
            APTTYPE apartment{};
            APTTYPEQUALIFIER qualifier{};
            winrt::check_hresult(CoGetApartmentType(&apartment, &qualifier));
            if (apartment != APTTYPE_MTA)
                throw winrt::hresult_wrong_thread();
            if (!memory || !task->svgEncoded || (!task->svgEncoded->RetainedHandle() && !task->svgEncoded.IsUnique()) ||
                !target.enabled || !target.generation || !std::isfinite(target.maximumWidthDip) ||
                target.maximumWidthDip <= 0 || !std::isfinite(target.effectivePhysicalScale) ||
                target.effectivePhysicalScale <= 0)
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            {
                auto const &captured = task->request->Target();
                if (task->request->Kind() == NOTEPADS_MARKDOWN_IMAGE_RASTER || !target.fontGeneration ||
                    target.fontGeneration != captured.font_generation ||
                    target.generation != captured.target_generation ||
                    target.maximumWidthDip != captured.available_width_dip ||
                    target.effectivePhysicalScale != captured.effective_physical_scale)
                    throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            }
        }
        catch (NativeFailure const &error)
        {
            Failure(*result, error.status, error.code());
        }
        catch (winrt::hresult_error const &error)
        {
            Failure(*result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, error.code());
        }
        catch (...)
        {
            Failure(*result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
        }

        if (!claimed)
            return;

        bool started{};
        try
        {
            if (result->nativeStatus != NOTEPADS_MARKDOWN_CORE_OK || FAILED(result->error))
                throw NativeImageFailure{result->nativeStatus};
            auto error = CoreError();
            {
                std::lock_guard lock(task->mutex);
                Check(notepads_markdown_core_model_cancel_create(memory->ModelOwner().Get(), &task->svgCoreCancel, &error));
                if (task->request->IsCancelled())
                    Check(notepads_markdown_core_cancel_request(task->svgCoreCancel));
            }
            Check(notepads_markdown_svg_work_create(NOTEPADS_MARKDOWN_SVG_WORK_ABI_VERSION, task->request->Handle(), memory->ModelOwner().Get(),
                                     task->svgCoreCancel, &task->svgAccount, &error));
            auto snapshot = notepads_markdown_svg_work_snapshot{};
            snapshot.header = {NOTEPADS_MARKDOWN_SVG_WORK_ABI_VERSION, sizeof(snapshot)};
            Check(notepads_markdown_svg_work_read(task->svgAccount, &snapshot, &error));
            // AccountOwner::take requires the original zero-ticket account.
            // Transfer it first; all Native node funding then uses resources.
            Check(
                notepads_markdown_svg_resources_create(NOTEPADS_MARKDOWN_SVG_RESOURCES_ABI_VERSION, &task->svgAccount, &task->svgResources, &error));
            notepads_markdown_svg_work_token nodeToken{};
            Check(notepads_markdown_svg_resources_reserve(task->svgResources, NOTEPADS_MARKDOWN_SVG_RESOURCE_ALLOCATION, sizeof(NativeSvgJobState),
                                           &nodeToken, &error));
            task->svgFailedToken = nodeToken;
            auto storage = HeapAlloc(GetProcessHeap(), 0, sizeof(NativeSvgJobState));
            if (!storage)
            {
                auto status = notepads_markdown_svg_resources_release_after_free(task->svgResources, &nodeToken, &error);
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    task->svgFailedToken = nodeToken;
                    Quarantine(*task, *result, status);
                }
                else
                    task->svgFailedToken = {};
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY};
            }
            auto node = new (storage) NativeSvgJobState{};
            node->task = task.get();
            node->target = target;
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            node->testFault = task->svgTestFault;
#endif
            node->resources = task->svgResources;
            node->nodeToken = nodeToken;
            node->identity = {snapshot.account_nonce, task->request->Key().epoch.source_generation,
                              task->request->Key().epoch.provider_generation, 0, target.generation};
            {
                std::lock_guard lock(task->mutex);
                task->svgState = node;
                task->svgFailedToken = {};
            }
            // A failure after publication leaves this exact node and its fee.
            Check(notepads_markdown_svg_resources_admit(node->resources, &nodeToken, sizeof(NativeSvgJobState), &error));
            node->taskBorrows = NativeSvgBorrowSet{node->resources};
            task->AdoptSvgMetadata(node->taskBorrows);
            Check(notepads_markdown_svg_resources_reserve(node->resources, NOTEPADS_MARKDOWN_SVG_RESOURCE_THREAD_STACK, StackReservation,
                                           &node->stackToken, &error));
            node->stackReservedToken = true;
            Check(notepads_markdown_svg_resources_reserve(node->resources, NOTEPADS_MARKDOWN_SVG_RESOURCE_LIBRARY_ALLOWANCE, LibraryAllowance,
                                           &node->libraryToken, &error));
            node->libraryReservedToken = true;
            Check(notepads_markdown_svg_resources_admit(node->resources, &node->libraryToken, LibraryAllowance, &error));
            node->libraryPaid = true;
            node->cancelIdle = CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET | CREATE_EVENT_INITIAL_SET,
                                              EVENT_MODIFY_STATE | SYNCHRONIZE);
            if (!node->cancelIdle)
                throw winrt::hresult_error{HRESULT_FROM_WIN32(GetLastError())};
            node->thread = CreateThread(nullptr, static_cast<SIZE_T>(StackReservation), Worker, node,
                                        STACK_SIZE_PARAM_IS_A_RESERVATION | CREATE_SUSPENDED, nullptr);
            if (!node->thread)
                throw winrt::hresult_error{HRESULT_FROM_WIN32(GetLastError())};
#if defined(NOTEPADS_MARKDOWN_DIAGNOSTICS)
            if (task->testWorkerTailReached || task->testWorkerTailProceed)
            {
                if (!task->testWorkerTailReached || !task->testWorkerTailProceed)
                    throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
                HANDLE duplicate{};
                winrt::check_bool(DuplicateHandle(GetCurrentProcess(), node->thread, GetCurrentProcess(), &duplicate,
                                                  SYNCHRONIZE, FALSE, 0));
                task->testWorkerThread.attach(duplicate);
            }
#endif
            Check(notepads_markdown_svg_resources_admit(node->resources, &node->stackToken, StackReservation, &error));
            node->stackPaid = true;
            if (ResumeThread(node->thread) == static_cast<DWORD>(-1))
                throw winrt::hresult_error{HRESULT_FROM_WIN32(GetLastError())};
            started = true;
        }
        catch (NativeFailure const &error)
        {
            WorkFailure(task->svgResources, *result, error.status, error.code());
        }
        catch (winrt::hresult_error const &error)
        {
            WorkFailure(task->svgResources, *result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT, error.code());
        }
        catch (...)
        {
            Failure(*result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
        }

        bool quarantined{};
        {
            std::lock_guard lock(task->mutex);
            quarantined = task->svgQuarantined;
        }
        auto safe = started ? Reap(*task, *result)
                            : !quarantined && DiscardUnpublished(*task, *result, task->svgState) &&
                                  (!task->svgState || RollbackBeforeThread(*task, *result));
        if (!safe)
            Quarantine(*task, *result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
        else
            safe = RetireAccount(*task, *result);
        {
            std::lock_guard lock(task->mutex);
            safe = safe && !task->svgQuarantined;
        }
        if (safe && task->svgOwnerHandoffCommitted)
        {
            try
            {
                // Both entry points converge here after the actual worker,
                // cancellation leases, borrows and account have retired. The
                // independent result must never expose a writable candidate.
                if (!task->svgCandidate)
                    throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
                task->svgCandidate->Freeze();
            }
            catch (NativeFailure const &error)
            {
                Quarantine(*task, *result, error.status, error.code());
                safe = false;
            }
            catch (...)
            {
                Quarantine(*task, *result, NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT);
                safe = false;
            }
        }
        bool finishPending{};
        if (safe)
        {
            if (task->request->IsCancelled() || !task->request->IsCurrent())
                Failure(*result, NOTEPADS_MARKDOWN_CORE_CANCELLED);
            if (task->svgOwnerHandoffCommitted)
            {
                result->pixels = std::move(task->svgCandidate);
                std::lock_guard lock(task->mutex);
                task->svgEncodedShareable = false;
                result->encoded = std::move(task->svgEncoded);
            }
            // Error owners were already freed/verified while their creator
            // borrows and original account were live. Never last-drop here.
            task->svgEncodedCharge.reset();
            task->svgResult.reset();
            std::shared_ptr<ReadingMemory> retiredMemory;
            std::shared_ptr<ImageLoadTask> retiredPin;
            {
                std::lock_guard lock(task->mutex);
                task->svgRetired = true;
                retiredMemory = std::move(task->svgMemory);
                retiredPin = std::move(task->svgKeepAlive);
                finishPending = task->svgFinishPending;
            }
        }
        if (finishPending)
            task->Finish();
        memory.reset();
        result.reset();
        task.reset();
        return;
    }
}
