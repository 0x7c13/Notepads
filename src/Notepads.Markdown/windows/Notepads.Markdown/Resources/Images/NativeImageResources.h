// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#pragma once
#include "Reading/NativeReadingSnapshot.h"
#include "Core/ModuleLifetime.h"
#include "notepads/markdown/images.h"
#include "notepads/markdown/svg_resources.h"
#include "notepads/markdown/svg_job.h"
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Storage.Streams.h>
#include <array>
#include <atomic>
#include <limits>
#include <memory>
#include <mutex>
#include <span>

namespace Notepads::Markdown::Native
{
    inline constexpr uint64_t ImageMaximumEncodedBytes = 16ull * 1024 * 1024;
    inline constexpr uint64_t ImageMaximumPixelCount = 16ull * 1024 * 1024;
    inline constexpr uint32_t ImageMaximumIntrinsicSide = 8192;
    inline constexpr uint32_t ImageMaximumDecodeSide = 4096;
    // Platform-side constants for the core process admission contract.
    inline constexpr uint64_t ImageProcessMaximumEncodedBytes = 128ull * 1024 * 1024;
    inline constexpr uint64_t ImageProcessMaximumDecodedBytes = 512ull * 1024 * 1024;
    inline constexpr uint64_t ImageOffscreenWarmTargetBytes = 128ull * 1024 * 1024;

    struct NativeImageFailure : NativeFailure
    {
        using NativeFailure::NativeFailure;
    };

    struct NativeImageSnapshot
    {
        std::array<notepads_markdown_image_entry, NOTEPADS_MARKDOWN_IMAGE_MAX_ENTRIES> entries{};
        notepads_markdown_image_snapshot header{};
    };

    // Native decoder geometry used to construct the image ABI metadata.
    struct RasterImageMetadata
    {
        uint32_t intrinsic_width{}, intrinsic_height{}, decoded_width{}, decoded_height{}, orientation{}, stride{};
    };

    notepads_markdown_image_metadata MakeRasterImageMetadata(RasterImageMetadata const &value) noexcept;
    notepads_markdown_image_metadata MakeSvgImageMetadata(notepads_markdown_svg_job_plan const &plan) noexcept;
    bool SameImageMetadata(notepads_markdown_image_metadata const &left, notepads_markdown_image_metadata const &right) noexcept;

    class NativeImageRequest;
    class NativeImageBytes;
    class NativeEncodedImage;
    class NativeImageReuseCache;

    // Synchronous construction only. Persisted allocator copies contain a
    // nonce, never this borrowed context, its factory, or a model owner.
    class ImagePayloadConstruction final
    {
      public:
        explicit ImagePayloadConstruction(ReadingMemory const &memory, ReadingAllocationFactory factory = {});
        ~ImagePayloadConstruction();
        ImagePayloadConstruction(ImagePayloadConstruction const &) = delete;
        ImagePayloadConstruction &operator=(ImagePayloadConstruction const &) = delete;
        uint64_t Nonce() const noexcept;
        static void *Allocate(uint64_t nonce, size_t bytes);

      private:
        ReadingMemory const &_memory;
        ReadingAllocationFactory _factory;
        uint64_t _nonce{};
        bool _consumed{};
        ImagePayloadConstruction *_previous{};
        static thread_local ImagePayloadConstruction *_current;
    };

    template <typename T> struct ImagePayloadAllocator
    {
        using value_type = T;
        uint64_t nonce{};
        ImagePayloadAllocator() noexcept = default;
        explicit ImagePayloadAllocator(uint64_t value) noexcept : nonce(value)
        {
        }
        template <typename U> ImagePayloadAllocator(ImagePayloadAllocator<U> const &other) noexcept : nonce(other.nonce)
        {
        }
        T *allocate(size_t count)
        {
            static_assert(alignof(T) <= alignof(std::max_align_t));
            if (count > std::numeric_limits<size_t>::max() / sizeof(T))
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            return static_cast<T *>(ImagePayloadConstruction::Allocate(nonce, count * sizeof(T)));
        }
        void deallocate(T *value, size_t) noexcept
        {
            ReadingMemory::Free(value);
        }
        template <typename U> bool operator==(ImagePayloadAllocator<U> const &other) const noexcept
        {
            return nonce == other.nonce;
        }
    };

    // Fixed storage: all read access is immutable, and only ImagePixels can
    // allocate or expose a checked builder span. The prefix owns final free.
    class ImagePixelBlock final
    {
      public:
        ImagePixelBlock() noexcept = default;
        ~ImagePixelBlock();
        ImagePixelBlock(ImagePixelBlock const &) = delete;
        ImagePixelBlock &operator=(ImagePixelBlock const &) = delete;
        uint8_t const *data() const noexcept
        {
            return _data;
        }
        uint8_t const *begin() const noexcept
        {
            return _data;
        }
        uint8_t const *end() const noexcept
        {
            return _size ? _data + _size : _data;
        }
        size_t size() const noexcept
        {
            return _size;
        }
        size_t capacity() const noexcept
        {
            return _size;
        }
        bool empty() const noexcept
        {
            return _size == 0;
        }
        uint8_t const &operator[](size_t index) const noexcept
        {
            return _data[index];
        }

      private:
        friend struct ImagePixels;
        void Allocate(ReadingMemory const &memory, size_t bytes, ReadingAllocationFactory factory);
        uint8_t *_data{};
        size_t _size{};
    };
    struct ImageLoadTask;
    void RegisterImageCapacityNotification() noexcept;
    void UnregisterImageCapacityNotification() noexcept;
    // Optional native reuse reclamation; never a physical capacity return.
    void RequestOptionalImageReclaim() noexcept;

    // Internal diagnostics exercise the actual control's threadpool observer;
    // no WinRT/FFI surface and no installation/removal of system fonts.
    struct ImageFontWatchDiagnosticResult
    {
        bool passed{};
        HRESULT error{S_OK};
        uint32_t waitsCreated{}, waitsClosed{}, workClosed{}, notifications{}, acknowledgements{}, frees{};
    };
    ImageFontWatchDiagnosticResult RunImageFontWatchDiagnostic(std::shared_ptr<ReadingMemory> const &memory,
                                                               uint32_t scenario) noexcept;

    struct NativeSvgImageTarget
    {
        double maximumWidthDip{2048};
        double effectivePhysicalScale{1};
        uint64_t generation{1};
        bool enabled{};
        uint64_t fontGeneration{1};
    };

    // The canonical resources capability pins these original-account tokens
    // until all reads/writes stop. Retained owners never hold the capability.
    // Capacity covers one encoded
    // block or the two actual std::allocate_shared/pixel blocks of a new image.
    class NativeSvgBorrowSet final
    {
      public:
        NativeSvgBorrowSet() noexcept = default;
        explicit NativeSvgBorrowSet(notepads_markdown_svg_resources *resources) noexcept;
        NativeSvgBorrowSet(NativeSvgBorrowSet &&other) noexcept;
        NativeSvgBorrowSet &operator=(NativeSvgBorrowSet &&other) noexcept;
        ~NativeSvgBorrowSet();
        NativeSvgBorrowSet(NativeSvgBorrowSet const &) = delete;
        NativeSvgBorrowSet &operator=(NativeSvgBorrowSet const &) = delete;
        notepads_markdown_svg_resources *Resources() const noexcept;
        bool CanAdd() const noexcept;
        void Add(notepads_markdown_svg_retained *owner, notepads_markdown_svg_work_token token) noexcept;
        void MarkUnallocated(notepads_markdown_svg_retained *owner, notepads_markdown_preview_allocation *root) noexcept;
        int32_t VerifyPhysicalRetired() const noexcept;
        int32_t Return() noexcept;

      private:
        notepads_markdown_svg_resources *_resources{};
        std::array<notepads_markdown_svg_work_token, 2> _tokens{};
        std::array<notepads_markdown_svg_retained *, 2> _owners{};
        std::array<notepads_markdown_preview_allocation *, 2> _unallocated{};
        std::array<bool, 2> _returned{};
        uint32_t _count{};
        int32_t _failure{NOTEPADS_MARKDOWN_CORE_OK};
    };

    class NativeEncodedImageHandle final
    {
      public:
        NativeEncodedImageHandle() noexcept = default;
        NativeEncodedImageHandle(NativeEncodedImageHandle const &other);
        NativeEncodedImageHandle(NativeEncodedImageHandle &&other) noexcept;
        NativeEncodedImageHandle &operator=(NativeEncodedImageHandle const &other);
        NativeEncodedImageHandle &operator=(NativeEncodedImageHandle &&other) noexcept;
        ~NativeEncodedImageHandle();
        NativeEncodedImage *Get() const noexcept;
        NativeEncodedImage *operator->() const noexcept;
        explicit operator bool() const noexcept;
        bool IsUnique() const noexcept;
        std::shared_ptr<NativeImageBytes> Charge() const noexcept;
        void Reset() noexcept;
        // Actual intrusive fetch_sub==1, not a physical-free success receipt.
        bool ResetAndWasLast() noexcept;

      private:
        friend class NativeEncodedImage;
        explicit NativeEncodedImageHandle(NativeEncodedImage *value) noexcept;
        NativeEncodedImage *_value{};
    };

    // Intrusive owner and the fixed-capacity source bytes share one exact
    // ReadingMemory block. No second source copy or shared_ptr control block.
    class NativeEncodedImage final
    {
      public:
        static NativeEncodedImageHandle Create(std::shared_ptr<ReadingMemory> memory,
                                               std::shared_ptr<NativeImageBytes> charge, size_t capacity);
        std::span<uint8_t const> Bytes() const noexcept;
        size_t Capacity() const noexcept;
        void Append(std::span<uint8_t const> bytes);
        void Freeze();
        bool IsFrozen() const noexcept;
        void AdoptSvg(NativeSvgBorrowSet &borrows);
        void BorrowSvg(NativeSvgBorrowSet &borrows) const;
        notepads_markdown_svg_retained *RetainedHandle() const noexcept;
        bool IsUnique() const noexcept;

      private:
        friend class NativeEncodedImageHandle;
        struct RawRetirement;
        static int32_t RawFreed(void *context) noexcept;
        NativeEncodedImage(std::shared_ptr<NativeImageBytes> charge,
                           size_t capacity, RawRetirement *retirement) noexcept;
        ~NativeEncodedImage() = default;
        void AddRef();
        bool Release() noexcept;
        uint8_t *Data() noexcept;
        std::atomic<uint32_t> _references{1};
        std::shared_ptr<NativeImageBytes> _charge;
        size_t _size{};
        size_t _capacity{};
        notepads_markdown_svg_retained *_retained{};
        RawRetirement *_rawRetirement{};
        bool _frozen{};
    };

    class NativeImagePolicy final : public std::enable_shared_from_this<NativeImagePolicy>
    {
        struct ConstructionKey
        {
        };

      public:
        static std::shared_ptr<NativeImagePolicy> Create();
        NativeImagePolicy(ConstructionKey, notepads_markdown_image_policy *policy) noexcept;
        ~NativeImagePolicy();
        NativeImagePolicy(NativeImagePolicy const &) = delete;
        NativeImagePolicy &operator=(NativeImagePolicy const &) = delete;
        void Begin(notepads_markdown_image_epoch const &epoch);
        void Desire(uint32_t object, notepads_markdown_image_target const &target);
        std::shared_ptr<NativeImageRequest> Start(uint32_t object, uint32_t kind = NOTEPADS_MARKDOWN_IMAGE_AUTO);
        uint64_t StateGeneration() const;
        NativeImageSnapshot Snapshot() const;
        void Evict(uint32_t object);
        void ReplaceSource(uint32_t object);
        bool TryForget(uint32_t object);
        // Cached successful Begin is an early rejection only. Complete always
        // asks Rust to revalidate the exact request and budget authority.
        bool IsCurrent(notepads_markdown_image_key const &key) const noexcept;

      private:
        friend class NativeImageRequest;
        ModuleLifetime _module;
        mutable std::mutex _mutex;
        notepads_markdown_image_policy *_policy{};
        notepads_markdown_image_epoch _epoch{};
        bool _hasEpoch{};
        void Complete(notepads_markdown_image_request const *request, notepads_markdown_image_completion const &completion,
                      notepads_markdown_image_bytes const *decoded);
        void CompleteReused(notepads_markdown_image_request const *request, notepads_markdown_image_completion const &completion,
                            notepads_markdown_image_bytes const *decoded);
    };

    class NativeImageBytes final
    {
        struct ConstructionKey
        {
        };

      public:
        NativeImageBytes(ConstructionKey, std::shared_ptr<NativeImagePolicy> policy, notepads_markdown_image_bytes *bytes, uint32_t kind) noexcept;
        ~NativeImageBytes();
        NativeImageBytes(NativeImageBytes const &) = delete;
        NativeImageBytes &operator=(NativeImageBytes const &) = delete;
        // Exactly once, after allocation. On error free that allocation before
        // releasing this lease. Native allocation funding is independent.
        void Admit(uint64_t actual);
        notepads_markdown_image_bytes const *Handle() const noexcept;
        bool IsAdmitted(uint32_t kind, uint64_t capacity) const noexcept;

      private:
        friend class NativeImageRequest;
        std::shared_ptr<NativeImagePolicy> _policy;
        notepads_markdown_image_bytes *_bytes{};
        uint32_t const _kind;
        std::atomic<uint64_t> _admitted{};
    };

    class NativeImageRequest final
    {
        struct ConstructionKey
        {
        };

      public:
        NativeImageRequest(ConstructionKey, std::shared_ptr<NativeImagePolicy> policy, notepads_markdown_image_request *request,
                           notepads_markdown_image_key const &key, notepads_markdown_image_target target, uint32_t kind) noexcept;
        ~NativeImageRequest();
        NativeImageRequest(NativeImageRequest const &) = delete;
        NativeImageRequest &operator=(NativeImageRequest const &) = delete;
        notepads_markdown_image_key const &Key() const noexcept;
        notepads_markdown_image_request const *Handle() const noexcept;
        void Cancel() noexcept;
        bool IsCancelled() const noexcept;
        bool IsCurrent() const noexcept;
        uint32_t Kind() const noexcept;
        notepads_markdown_image_target const &Target() const noexcept;
        bool BelongsTo(NativeImagePolicy const *policy) const noexcept;
        std::shared_ptr<NativeImageBytes> Reserve(uint32_t kind, uint64_t bytes);
        void CompleteReady(notepads_markdown_image_metadata const &metadata, std::shared_ptr<NativeImageBytes> const &decoded);
        void CompleteReused(notepads_markdown_image_metadata const &metadata, std::shared_ptr<NativeImageBytes> const &decoded);
        void NoteMetadata(notepads_markdown_image_metadata const &metadata);
        void Fail(uint32_t kind, HRESULT error, uint32_t detail = 0);

      private:
        friend class NativeImagePolicy;
        std::shared_ptr<NativeImagePolicy> _policy;
        notepads_markdown_image_request *_request{};
        notepads_markdown_image_key _key{};
        notepads_markdown_image_target const _target;
        uint32_t const _kind;
        std::atomic<bool> _cancelled{};
        std::atomic<uint32_t> _reclaimSentKinds{};
    };

    struct ImageFramePosition
    {
        uint32_t index{};
        uint32_t nextDelayMilliseconds{};
    };

    struct ImagePixels final
    {
      private:
        struct RasterRetirement;
        RasterRetirement *_rasterRetirement{};
        static int32_t RasterFreed(void *context) noexcept;
        struct SvgPayloadContext
        {
            NativeSvgBorrowSet *borrows{};
            notepads_markdown_image_bytes const *image{};
            notepads_markdown_svg_retained *retained{};
        };
        SvgPayloadContext _svg;
        bool _frozen{};
        uint32_t _frameCount{1}, _playthroughs{1}, _durationMilliseconds{};
        static void *AllocateSvgPayload(void *context, ReadingMemory const &memory, size_t bytes);

      public:
        // Reverse destruction: pixels are freed by their model-root allocator
        // before the image subquota lease is returned. No request/stream/GPU pin.
        std::shared_ptr<NativeImageBytes> charge;
        notepads_markdown_image_metadata metadata{};
        ImagePixelBlock pixels;
        ImagePixels(std::shared_ptr<ReadingMemory> memory, std::shared_ptr<NativeImageBytes> lease);
        ImagePixels(std::shared_ptr<ReadingMemory> memory, std::shared_ptr<NativeImageBytes> lease,
                    NativeSvgBorrowSet *borrows);
        ~ImagePixels();
        void AllocatePixels(ReadingMemory const &memory, size_t bytes);
        std::span<uint8_t> MutablePixels();
        void SetAnimation(std::span<uint32_t const> frameEnds, uint32_t playthroughs);
        bool IsAnimated() const noexcept { return _frameCount > 1; }
        std::span<uint8_t const> FramePixels(uint32_t index) const noexcept;
        ImageFramePosition FrameAt(uint64_t elapsedMilliseconds) const noexcept;
        void Freeze();
        bool IsFrozen() const noexcept;
        void BindRasterAllocation();
        void FinishSvgAllocation();
        notepads_markdown_svg_retained *RetainedHandle() const noexcept;
        ImagePixels(ImagePixels const &) = delete;
        ImagePixels &operator=(ImagePixels const &) = delete;
    };

    struct NativeSvgImageAllocation
    {
        // Reverse destruction returns job borrows before the actual last image
        // owner is released. A worker can explicitly Return before publication.
        std::shared_ptr<ImagePixels> pixels;
        NativeSvgBorrowSet borrows;
        notepads_markdown_svg_retained *metadataRetained{};
    };

    void CreateSvgImagePixels(std::shared_ptr<NativeImageRequest> const &request,
                              std::shared_ptr<ReadingMemory> const &memory, notepads_markdown_svg_resources *resources,
                              uint64_t capacity, NativeSvgImageAllocation &output);

    struct NativeImageDecodeResult
    {
        std::shared_ptr<ImagePixels> pixels;
        NativeEncodedImageHandle encoded;
        uint64_t reuseScope{};
        bool reused{};
        // Actual renderer identity/plan is retained for UI target validation;
        // the pixels contain the single canonical metadata80 value.
        notepads_markdown_svg_job_plan svgPlan{};
        bool svg{};
        uint32_t failure{NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE};
        HRESULT error{S_OK};
        int32_t nativeStatus{NOTEPADS_MARKDOWN_CORE_OK};
        uint32_t detail{};
        // Preserve cleanup failure even when decoding already failed.
        HRESULT closeError{S_OK};
        bool Ready() const noexcept;
    };

    // Every coroutine parameter is owned by value. Once read/decode/close and
    // result transfer finish, this releases its request parameter explicitly;
    // a completed IAsyncAction cannot prolong its physical IO permit.
    // Only agile independent readable streams are accepted. Started reads join
    // their actual Completed notification before Close/permit retirement;
    // cancellation is forwarded after the observer is registered. Every acquired
    // stream is closed exactly once, including rejection/cancel/stale branches,
    // except an unobservable read: its original owners and Close authority are
    // quarantined under the original physical IO permit, never reported retired.
    // This fills result only; the UI coordinator commits Rust Ready/Fail together
    // with its pixel map after rechecking current source/provider/request.
    winrt::Windows::Foundation::IAsyncAction DecodeImageAsync(
        std::shared_ptr<NativeImageRequest> request, std::shared_ptr<ReadingMemory> memory,
        winrt::Windows::Storage::Streams::IRandomAccessStream stream, uint32_t maximumDecodeSide,
        std::shared_ptr<NativeImageDecodeResult> result, std::shared_ptr<ImageLoadTask> svgTask = {},
        NativeSvgImageTarget svgTarget = {}, std::shared_ptr<NativeImageReuseCache> reuseCache = {}, uint64_t reuseScope = 0);
}
