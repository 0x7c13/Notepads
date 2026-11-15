// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "NativeImageTestSupport.h"
#include "NativeSvgRetainedTests.h"
#include "Resources/Images/NativeImageResources.h"
#include <algorithm>
#include <array>
#include <cstring>
#include <exception>
#include <source_location>
#include <string>
#include <utility>

namespace Notepads::Markdown::Native::SvgRetainedTests
{
    namespace
    {
        template <typename T> T CorePod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }

        void Check(int32_t status)
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw NativeImageFailure{status};
        }

        void Require(bool condition, std::source_location location = std::source_location::current())
        {
            if (!condition)
                throw winrt::hresult_error{E_FAIL,
                                           L"Native retained assertion at line " + std::to_wstring(location.line())};
        }

        notepads_markdown_svg_retained_snapshot Read(notepads_markdown_svg_retained const *owner)
        {
            notepads_markdown_svg_retained_snapshot value{};
            value.header = {NOTEPADS_MARKDOWN_SVG_RETAINED_ABI_VERSION, sizeof(value)};
            auto error = CorePod<notepads_markdown_core_error>();
            Check(notepads_markdown_svg_retained_read(owner, &value, &error));
            return value;
        }

        notepads_markdown_svg_work_snapshot Work(notepads_markdown_svg_resources *resources)
        {
            notepads_markdown_svg_work_snapshot value{};
            value.header = {NOTEPADS_MARKDOWN_SVG_WORK_ABI_VERSION, sizeof(value)};
            auto error = CorePod<notepads_markdown_core_error>();
            Check(notepads_markdown_svg_resources_read_work(resources, &value, &error));
            return value;
        }

        void Bound(void *owner) noexcept
        {
            notepads_markdown_svg_retained_bind_physical(static_cast<notepads_markdown_svg_retained *>(owner));
        }

        int32_t Freed(void *owner) noexcept
        {
            return notepads_markdown_svg_retained_after_physical_free_checked(static_cast<notepads_markdown_svg_retained *>(owner));
        }

        class HookScope
        {
          public:
            explicit HookScope(ReadingMemoryTestHooks &hooks) noexcept : _old(ReadingMemory::SetTestHooks(&hooks))
            {
            }
            ~HookScope()
            {
                ReadingMemory::SetTestHooks(_old);
            }

          private:
            ReadingMemoryTestHooks *_old;
        };

        struct Fixture
        {
            std::shared_ptr<NativeImagePolicy> policy;
            std::shared_ptr<NativeImageRequest> request;
            notepads_markdown_core_cancel *flag{};
            notepads_markdown_svg_work *account{};
            notepads_markdown_svg_resources *resources{};
            notepads_markdown_svg_work_snapshot baseline{};

            explicit Fixture(notepads_markdown_core_model *model)
            {
                auto error = CorePod<notepads_markdown_core_error>();
                auto summary = CorePod<notepads_markdown_core_model_summary_result>();
                Check(notepads_markdown_core_model_summary(model, &summary, &error));
                Check(notepads_markdown_core_model_cancel_create(model, &flag, &error));
                try
                {
                    policy = NativeImagePolicy::Create();
                    notepads_markdown_image_epoch epoch{};
                    std::copy(std::begin(summary.identity.document), std::end(summary.identity.document),
                              std::begin(epoch.source.document));
                    epoch.source.revision = summary.identity.revision;
                    epoch.source_generation = 1;
                    epoch.provider_generation = 1;
                    policy->Begin(epoch);
                    request = StartImageTestRequest(policy, 1);
                    Check(
                        notepads_markdown_svg_work_create(NOTEPADS_MARKDOWN_SVG_WORK_ABI_VERSION, request->Handle(), model, flag, &account, &error));
                    Check(notepads_markdown_svg_resources_create(NOTEPADS_MARKDOWN_SVG_RESOURCES_ABI_VERSION, &account, &resources, &error));
                    Require(!account && resources);
                    baseline = Work(resources);
                    // The original account now owns exactly AccountAccess and
                    // resources/shared-first-failure metadata payments. These
                    // are retained baseline costs, never hidden retained debt.
                    Require(baseline.tickets == 2 && baseline.pending == 0 &&
                            baseline.job_bytes == baseline.header_bytes + baseline.root_work_bytes &&
                            baseline.control_bytes == baseline.job_bytes);
                }
                catch (...)
                {
                    RetireAccount();
                    (void)notepads_markdown_core_cancel_destroy(flag);
                    flag = nullptr;
                    throw;
                }
            }

            ~Fixture()
            {
                RetireAccount();
                request.reset();
                policy.reset();
                (void)notepads_markdown_core_cancel_destroy(flag);
            }

            void Reclaim()
            {
                notepads_markdown_svg_resources_retirement retired{};
                retired.header = {NOTEPADS_MARKDOWN_SVG_RESOURCES_ABI_VERSION, sizeof(retired)};
                auto error = CorePod<notepads_markdown_core_error>();
                Check(notepads_markdown_svg_resources_reclaim(&resources, &retired, &error));
                Require(!resources && retired.consumed && retired.cleanup_succeeded && retired.account);
                account = retired.account;
            }

            void RetireAccount() noexcept
            {
                try
                {
                    if (resources)
                        Reclaim();
                    if (account)
                    {
                        auto error = CorePod<notepads_markdown_core_error>();
                        Check(notepads_markdown_svg_work_close(account, &error));
                        Check(notepads_markdown_svg_work_destroy(account, &error));
                        account = nullptr;
                    }
                }
                catch (...)
                {
                    // An uncertain consumed retirement cannot release the
                    // original request/policy/token pins or retry destruction.
                    std::terminate();
                }
            }
        };

        // A distinct admitted metadata block keeps a real reference to the same
        // model-root while target records deallocate, allowing the true root
        // gauge to be observed after a target handle itself becomes invalid.
        struct RootGauge
        {
            Fixture fixture;
            NativeSvgBorrowSet borrow;
            void *allocation{};
            notepads_markdown_svg_retained *owner{};

            RootGauge(notepads_markdown_core_model *model, ReadingMemory const &memory) : fixture(model), borrow(fixture.resources)
            {
                auto error = CorePod<notepads_markdown_core_error>();
                PreparedReadingAllocation prepared;
                notepads_markdown_svg_work_token token{};
                Check(notepads_markdown_svg_resources_retained_prepare(
                    NOTEPADS_MARKDOWN_SVG_RESOURCES_ABI_VERSION, fixture.resources, NOTEPADS_MARKDOWN_SVG_RETAINED_METADATA, nullptr, &prepared.lease,
                    ReadingMemory::AllocationRequestBytes(16), 0, &owner, &token, &error));
                borrow.Add(owner, token);
                prepared.action = {owner, Freed};
                prepared.bindPhysical = Bound;
                try
                {
                    allocation = memory.AllocatePrepared(16, prepared);
                    Check(notepads_markdown_svg_retained_admit(owner, &error));
                }
                catch (...)
                {
                    (void)borrow.Return();
                    if (prepared.lease)
                    {
                        (void)notepads_markdown_preview_allocation_destroy(prepared.lease);
                        notepads_markdown_svg_retained_abort_unallocated(owner);
                    }
                    else if (allocation)
                        ReadingMemory::Free(allocation);
                    throw;
                }
            }

            ~RootGauge()
            {
                (void)borrow.Return();
                ReadingMemory::Free(allocation);
            }

            uint64_t Live() const
            {
                return Read(owner).root_live_bytes;
            }
        };

        struct FreeObservation
        {
            notepads_markdown_svg_retained *owner{};
            std::array<ReadingMemoryTestEvent, 8> events{};
            std::array<notepads_markdown_svg_retained_snapshot, 8> snapshots{};
            uint32_t written{};
            bool failed{};

            static void Observe(void *context, void *, ReadingMemoryTestEvent event) noexcept
            {
                auto &self = *static_cast<FreeObservation *>(context);
                try
                {
                    if (self.written == self.events.size())
                    {
                        self.failed = true;
                        return;
                    }
                    self.events[self.written] = event;
                    self.snapshots[self.written] = Read(self.owner);
                    ++self.written;
                }
                catch (...)
                {
                    self.failed = true;
                }
            }
        };

        struct FailedAllocationObservation
        {
            void *allocation{};
            notepads_markdown_svg_retained *owner{};
            static void Observe(void *context, void *allocation, ReadingMemoryTestEvent event) noexcept
            {
                if (event == ReadingMemoryTestEvent::Allocated)
                {
                    auto &self = *static_cast<FailedAllocationObservation *>(context);
                    self.allocation = allocation;
                    self.owner =
                        static_cast<notepads_markdown_svg_retained *>(ReadingMemory::InspectAllocation(allocation).action.context);
                }
            }
        };

        struct OrdinaryFreeObservation
        {
            NativeImagePolicy *policy{};
            uint64_t beforeFree{};
            bool failed{};
            static void Observe(void *context, void *, ReadingMemoryTestEvent event) noexcept
            {
                if (event == ReadingMemoryTestEvent::BeforeFree)
                {
                    auto &self = *static_cast<OrdinaryFreeObservation *>(context);
                    try
                    {
                        self.beforeFree = self.policy->Snapshot().header.encoded_bytes;
                    }
                    catch (...)
                    {
                        self.failed = true;
                    }
                }
            }
        };

        NativeEncodedImageHandle Encoded(Fixture &fixture, std::shared_ptr<ReadingMemory> const &memory,
                                         NativeSvgBorrowSet &borrow)
        {
            auto charge = fixture.request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, 128);
            auto value = NativeEncodedImage::Create(memory, charge, 128);
            constexpr std::array<uint8_t, 6> bytes{'<', 's', 'v', 'g', '/', '>'};
            value->Append(bytes);
            value->AdoptSvg(borrow);
            return value;
        }
    }

    class MemoryAccess
    {
      public:
        static std::shared_ptr<ReadingMemory> CreateBorrowed(notepads_markdown_core_model *model, notepads_markdown_core_cancel *sourceFlag)
        {
            Require(model && sourceFlag);
            auto error = CorePod<notepads_markdown_core_error>();
            // This actual same-root operation validates the supplied host flag.
            notepads_markdown_preview_allocation *proof{};
            Check(notepads_markdown_preview_model_reserve(model, sourceFlag, 0, &proof, &error));
            Check(notepads_markdown_preview_allocation_destroy(proof));
            auto summary = CorePod<notepads_markdown_core_model_summary_result>();
            Check(notepads_markdown_core_model_summary(model, &summary, &error));
            notepads_markdown_core_cancel *allocationFlag{};
            Check(notepads_markdown_core_model_cancel_create(model, &allocationFlag, &error));
            try
            {
                // Only the fixture's wrapper/control block uses the fixture
                // allocator. Every tested data/control block uses the genuine
                // production ReadingMemory allocator and captured Rust root.
                auto result = std::make_shared<ReadingMemory>(ReadingMemory::ConstructionKey{}, CoreModelPin{},
                                                              allocationFlag, summary.identity);
                allocationFlag = nullptr;
                result->_borrowedTestModel = model;
                Require(!result->ModelOwner());
                return result;
            }
            catch (...)
            {
                (void)notepads_markdown_core_cancel_destroy(allocationFlag);
                throw;
            }
        }
    };

    Results Run(notepads_markdown_core_model *model, notepads_markdown_core_cancel *sourceFlag)
    {
        Results result;
        auto memory = MemoryAccess::CreateBorrowed(model, sourceFlag);
        RootGauge gauge{model, *memory};
        auto runBaseline = gauge.Live();

        {
            Fixture fixture{model};
            NativeSvgBorrowSet borrow{fixture.resources};
            auto encoded = Encoded(fixture, memory, borrow);
            auto owner = encoded->RetainedHandle();
            auto before = Read(owner);
            Require(before.phase == 2 && before.refs == 2 && before.native_bytes > encoded->Capacity());
            auto clone = encoded;
            Require(clone->Bytes().data() == encoded->Bytes().data() && clone->Bytes().size() == 6);
            result.encodedShared = true;
            NativeSvgBorrowSet duplicate{fixture.resources};
            bool duplicateRejected{};
            try
            {
                clone->BorrowSvg(duplicate);
            }
            catch (NativeFailure const &failure)
            {
                duplicateRejected = failure.status == NOTEPADS_MARKDOWN_CORE_INVALID_STATE;
            }
            Require(duplicateRejected && Read(owner).refs == before.refs);
            clone.Reset();
            auto pointer = encoded.Get();
            auto moduleBefore = static_cast<uint32_t>(winrt::get_module_lock());
            FreeObservation observation{owner};
            ReadingMemoryTestHooks hooks{pointer, false, false, true, &observation, FreeObservation::Observe};
            {
                HookScope scope{hooks};
                // Stop all payload reads. Hold only the Rust borrow reference
                // to observe metadata retirement after the physical ref drops.
                encoded.Reset();
                auto failed = Read(owner);
                Require(failed.phase == before.phase && failed.root_live_bytes == before.root_live_bytes &&
                        failed.control_bytes == before.control_bytes &&
                        fixture.policy->Snapshot().header.encoded_bytes == 128 &&
                        ReadingMemory::InspectAllocation(pointer).hasAction &&
                        static_cast<uint32_t>(winrt::get_module_lock()) == moduleBefore);
                result.failedFreePreserved = true;
                Require(ReadingMemory::TryFree(pointer));
            }
            auto after = Read(owner);
            Require(after.phase == 3 && after.refs == 1 &&
                    after.control_bytes + before.native_bytes + before.preview_bytes == before.control_bytes &&
                    fixture.policy->Snapshot().header.encoded_bytes == 0);
            Require(!observation.failed && observation.written == 6 &&
                    observation.events[0] == ReadingMemoryTestEvent::BeforeFree &&
                    observation.events[1] == ReadingMemoryTestEvent::FreeFailed &&
                    observation.events[2] == ReadingMemoryTestEvent::BeforeFree &&
                    observation.events[3] == ReadingMemoryTestEvent::PhysicalFreed &&
                    observation.events[4] == ReadingMemoryTestEvent::RootReleased &&
                    observation.events[5] == ReadingMemoryTestEvent::CallbackCompleted);
            Require(observation.snapshots[3].root_live_bytes == before.root_live_bytes &&
                    observation.snapshots[4].root_live_bytes + before.native_bytes + before.preview_bytes <=
                        before.root_live_bytes &&
                    observation.snapshots[4].control_bytes == before.control_bytes &&
                    observation.snapshots[5].control_bytes == after.control_bytes);
            auto withHeader = gauge.Live();
            Check(borrow.Return());
            Require(gauge.Live() + before.header_bytes == withHeader);
            result.actualRetryReleased = true;
            result.freeOrderObserved = true;
            result.checks += 4;
        }
        Require(gauge.Live() == runBaseline);

        {
            Fixture fixture{model};
            NativeSvgImageAllocation image;
            CreateSvgImagePixels(fixture.request, memory, fixture.resources, 1024, image);
            auto metadata = Read(image.metadataRetained);
            auto payload = Read(image.pixels->RetainedHandle());
            auto work = Work(fixture.resources);
            Require(metadata.kind == NOTEPADS_MARKDOWN_SVG_RETAINED_METADATA && payload.kind == NOTEPADS_MARKDOWN_SVG_RETAINED_PAYLOAD &&
                    metadata.phase == 2 && payload.phase == 2 &&
                    work.job_bytes == work.header_bytes + work.root_work_bytes + metadata.job_borrow_bytes +
                                          payload.job_borrow_bytes);
            auto writable = image.pixels->MutablePixels();
            std::fill(writable.begin(), writable.end(), uint8_t{0x39});
            auto weak = std::weak_ptr<ImagePixels>{image.pixels};
            Check(image.borrows.Return());
            auto metadataOwner = image.metadataRetained;
            image.pixels.reset();
            auto held = Read(metadataOwner);
            Require(held.phase == 2 && held.refs == 1 && fixture.policy->Snapshot().header.decoded_bytes == 0 &&
                    Work(fixture.resources).control_bytes ==
                        fixture.baseline.control_bytes + metadata.job_borrow_bytes);
            result.pixelMetadataFunded = true;
            weak.reset();
            Require(Work(fixture.resources).control_bytes == fixture.baseline.control_bytes);
            result.weakMetadataRetained = true;
            result.checks += 2;
        }
        Require(gauge.Live() == runBaseline);

        {
            Fixture fixture{model};
            ReadingMemoryTestHooks hooks;
            hooks.failNextAllocate = true;
            bool rejected{};
            NativeSvgImageAllocation image;
            ReadingFreeReceipt cleanup;
            {
                ReadingFreeScope freeScope{cleanup};
                HookScope scope{hooks};
                try
                {
                    CreateSvgImagePixels(fixture.request, memory, fixture.resources, 256, image);
                }
                catch (NativeFailure const &failure)
                {
                    rejected = failure.status == NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY;
                }
                image.pixels.reset();
                Check(image.borrows.VerifyPhysicalRetired());
                Check(image.borrows.Return());
            }
            auto work = Work(fixture.resources);
            Require(rejected && cleanup.Succeeded() && work.tickets == fixture.baseline.tickets &&
                    work.control_bytes == fixture.baseline.control_bytes &&
                    fixture.policy->Snapshot().header.reservations == 0);
            result.allocationFailureRetired = true;
            ++result.checks;
        }
        Require(gauge.Live() == runBaseline);

        {
            Fixture fixture{model};
            ReadingMemoryTestHooks hooks;
            hooks.failNextSize = true;
            bool rejected{};
            NativeSvgImageAllocation image;
            ReadingFreeReceipt cleanup;
            {
                ReadingFreeScope freeScope{cleanup};
                HookScope scope{hooks};
                try
                {
                    CreateSvgImagePixels(fixture.request, memory, fixture.resources, 256, image);
                }
                catch (NativeFailure const &failure)
                {
                    rejected = failure.status == NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                }
                image.pixels.reset();
                Check(image.borrows.VerifyPhysicalRetired());
                Check(image.borrows.Return());
            }
            auto work = Work(fixture.resources);
            Require(rejected && cleanup.Succeeded() && cleanup.physicalFreed == 1 && cleanup.rootReleased == 1 &&
                    work.tickets == fixture.baseline.tickets && work.control_bytes == fixture.baseline.control_bytes);
            result.sizeFailureRetired = true;
            ++result.checks;
        }
        Require(gauge.Live() == runBaseline);

        {
            Fixture fixture{model};
            NativeSvgBorrowSet borrow{fixture.resources};
            auto encoded = Encoded(fixture, memory, borrow);
            Check(borrow.Return());
            auto error = CorePod<notepads_markdown_core_error>();
            fixture.Reclaim();
            Check(notepads_markdown_svg_work_close(fixture.account, &error));
            Check(notepads_markdown_svg_work_destroy(fixture.account, &error));
            fixture.account = nullptr;
            Check(notepads_markdown_core_cancel_request(fixture.flag));
            fixture.request.reset();
            auto epoch = fixture.policy->Snapshot().header.epoch;
            ++epoch.source_generation;
            ++epoch.source.revision;
            fixture.policy->Begin(epoch);
            Require(fixture.policy->Snapshot().header.encoded_bytes == 128);
            encoded.Reset();
            Require(fixture.policy->Snapshot().header.encoded_bytes == 0);
            result.cancelledSourceCleanup = true;
            ++result.checks;
        }
        Require(gauge.Live() == runBaseline);

        {
            Fixture fixture{model};
            FailedAllocationObservation observation;
            ReadingMemoryTestHooks hooks;
            hooks.failNextSize = true;
            hooks.failNextFree = true;
            hooks.context = &observation;
            hooks.observe = FailedAllocationObservation::Observe;
            bool firstPreserved{};
            NativeSvgImageAllocation image;
            ReadingFreeReceipt cleanup;
            {
                ReadingFreeScope freeScope{cleanup};
                HookScope scope{hooks};
                try
                {
                    CreateSvgImagePixels(fixture.request, memory, fixture.resources, 256, image);
                }
                catch (NativeFailure const &failure)
                {
                    firstPreserved = failure.status == NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
                }
            }
            // This older standalone retained-owner test deliberately returns
            // its borrow then retries the still-valid raw prefix below. The
            // production ImageLoadTask instead retains its row/slot forever.
            Check(image.borrows.Return());
            auto work = Work(fixture.resources);
            Require(firstPreserved && cleanup.physicalFailures == 1 && !cleanup.Succeeded() && observation.allocation &&
                    observation.owner && work.tickets == fixture.baseline.tickets &&
                    work.control_bytes > fixture.baseline.control_bytes);
            auto failed = Read(observation.owner);
            Require(failed.phase == 1 && failed.refs == 1 &&
                    ReadingMemory::InspectAllocation(observation.allocation).hasAction);
            // The original valid prefix is the canonical owner. The object's
            // construction never completed, so retry only its raw allocation.
            Require(ReadingMemory::TryFree(observation.allocation));
            auto after = Work(fixture.resources);
            Require(after.control_bytes == fixture.baseline.control_bytes);
            result.combinedFailureRetry = true;
            ++result.checks;
        }
        Require(gauge.Live() == runBaseline);

        {
            Fixture fixture{model};
            constexpr std::array<uint8_t, 70> bmp{'B', 'M', 70, 0,   0,   0,   0, 0, 0, 0, 54,  0, 0,   0, 40, 0, 0,  0,
                                                  2,   0,   0,  0,   2,   0,   0, 0, 1, 0, 24,  0, 0,   0, 0,  0, 16, 0,
                                                  0,   0,   0,  0,   0,   0,   0, 0, 0, 0, 0,   0, 0,   0, 0,  0, 0,  0,
                                                  255, 0,   0,  255, 255, 255, 0, 0, 0, 0, 255, 0, 255, 0, 0,  0};
            constexpr std::array<uint8_t, 16> expected{0,   0, 255, 255, 0,   255, 0,   255,
                                                       255, 0, 0,   255, 255, 255, 255, 255};
            winrt::Windows::Storage::Streams::InMemoryRandomAccessStream stream;
            winrt::Windows::Storage::Streams::DataWriter writer{stream};
            writer.WriteBytes(bmp);
            writer.StoreAsync().get();
            writer.DetachStream();
            stream.Seek(0);
            auto decoded = std::make_shared<NativeImageDecodeResult>();
            DecodeImageAsync(fixture.request, memory, stream, ImageMaximumDecodeSide, decoded).get();
            Require(decoded->Ready() && !decoded->encoded && decoded->pixels->pixels.size() == expected.size() &&
                    std::equal(expected.begin(), expected.end(), decoded->pixels->pixels.begin()) &&
                    fixture.policy->Snapshot().header.encoded_bytes == 0);
            fixture.request->CompleteReady(decoded->pixels->metadata, decoded->pixels->charge);
            decoded.reset();
            auto state = fixture.policy->Snapshot();
            Require(state.header.decoded_bytes == 0 && state.header.reservations == 0);
            result.ordinaryRasterPreserved = true;
            ++result.checks;
        }
        Require(gauge.Live() == runBaseline);

        {
            Fixture fixture{model};
            // No external lease copy: the factory must pin its own final image
            // charge through HeapFree, even on the non-SVG successful path.
            auto encoded = NativeEncodedImage::Create(memory, fixture.request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, 96), 96);
            OrdinaryFreeObservation observation{fixture.policy.get()};
            ReadingMemoryTestHooks hooks;
            hooks.target = encoded.Get();
            hooks.context = &observation;
            hooks.observe = OrdinaryFreeObservation::Observe;
            {
                HookScope scope{hooks};
                encoded.Reset();
            }
            Require(!observation.failed && observation.beforeFree == 96 &&
                    fixture.policy->Snapshot().header.encoded_bytes == 0);
            result.ordinaryEncodedChargePinned = true;
            ++result.checks;
        }
        Require(gauge.Live() == runBaseline);

        {
            Fixture fixture{model};
            auto encoded = NativeEncodedImage::Create(memory, fixture.request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, 128), 128);
            auto original = ReadingMemory::InspectAllocation(encoded.Get());
            NativeSvgBorrowSet absent;
            bool refused{};
            try { encoded->AdoptSvg(absent); }
            catch (NativeFailure const &failure) { refused = failure.status == NOTEPADS_MARKDOWN_CORE_INVALID_STATE; }
            auto unchanged = ReadingMemory::InspectAllocation(encoded.Get());
            Require(refused && !encoded->RetainedHandle() && unchanged.hasAction &&
                    unchanged.action.context == original.action.context && fixture.policy->Snapshot().header.encoded_bytes == 128);
            encoded.Reset();
            Require(fixture.policy->Snapshot().header.encoded_bytes == 0);
            result.rawAdoptionRefusalPreserved = true;
            ++result.checks;
        }
        Require(gauge.Live() == runBaseline);

        {
            Fixture fixture{model};
            NativeSvgBorrowSet borrow{fixture.resources};
            auto encoded = NativeEncodedImage::Create(memory, fixture.request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, 128), 128);
            auto rawRecord = ReadingMemory::InspectAllocation(encoded.Get()).action.context;
            ReadingMemoryTestHooks hooks;
            hooks.target = rawRecord;
            hooks.failNextFree = true;
            bool refused{};
            {
                HookScope scope{hooks};
                try { encoded->AdoptSvg(borrow); }
                catch (NativeFailure const &failure) { refused = failure.status == NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT; }
            }
            auto canonical = encoded->RetainedHandle();
            Require(refused && canonical && ReadingMemory::InspectAllocation(encoded.Get()).action.context == canonical &&
                    fixture.policy->Snapshot().header.encoded_bytes == 128);
            encoded.Reset();
            Check(borrow.VerifyPhysicalRetired());
            Check(borrow.Return());
            Require(fixture.policy->Snapshot().header.encoded_bytes == 0 && ReadingMemory::TryFree(rawRecord));
            Require(Work(fixture.resources).control_bytes == fixture.baseline.control_bytes);
            result.failedRawHandoffRecovered = true;
            ++result.checks;
        }
        Require(gauge.Live() == runBaseline);

        return result;
    }
}
