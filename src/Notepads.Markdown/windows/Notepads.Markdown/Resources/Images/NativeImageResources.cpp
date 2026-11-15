// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Resources/Images/NativeImageResources.h"
#include "Resources/Svg/NativeSvgJob.h"
#include "Resources/Images/NativeImageCoordinator.h"
#include "Resources/Images/NativeImageReuseCache.h"
#include <wincodec.h>
#include <robuffer.h>
#include <objidl.h>
#include <algorithm>
#include <cstring>
#include <span>
#include <new>
#include <utility>

namespace Notepads::Markdown::Native
{
    thread_local ImagePayloadConstruction *ImagePayloadConstruction::_current{};

    ImagePayloadConstruction::ImagePayloadConstruction(ReadingMemory const &memory, ReadingAllocationFactory factory)
        : _memory(memory), _factory(factory)
    {
        static std::atomic<uint64_t> lastNonce{};
        auto previous = lastNonce.load(std::memory_order_relaxed);
        for (;;)
        {
            if (previous == UINT64_MAX)
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            if (lastNonce.compare_exchange_weak(previous, previous + 1, std::memory_order_relaxed))
                break;
        }
        _nonce = previous + 1;
        _previous = _current;
        _current = this;
    }

    ImagePayloadConstruction::~ImagePayloadConstruction()
    {
        _current = _previous;
    }

    uint64_t ImagePayloadConstruction::Nonce() const noexcept
    {
        return _nonce;
    }

    void *ImagePayloadConstruction::Allocate(uint64_t nonce, size_t bytes)
    {
        auto context = _current;
        if (!context || !nonce || context->_nonce != nonce || context->_consumed || !bytes)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        // Consume before entering a factory: even reentrant construction cannot
        // allocate another block under this original permission.
        context->_consumed = true;
        if (context->_factory.allocate)
            return context->_factory.allocate(context->_factory.context, context->_memory, bytes);
        return context->_memory.Allocate(bytes);
    }

    ImagePixelBlock::~ImagePixelBlock()
    {
        ReadingMemory::Free(_data);
    }

    void ImagePixelBlock::Allocate(ReadingMemory const &memory, size_t bytes, ReadingAllocationFactory factory)
    {
        if (_data || !bytes || bytes > ImageMaximumPixelCount * 4)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        auto allocation = factory.allocate ? factory.allocate(factory.context, memory, bytes) : memory.Allocate(bytes);
        _data = static_cast<uint8_t *>(allocation);
        _size = bytes;
    }

    namespace
    {
        template <typename T> T CorePod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_CORE_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }

        template <typename T> T ImagePod() noexcept
        {
            T value{};
            value.header = {NOTEPADS_MARKDOWN_IMAGE_ABI_VERSION, static_cast<uint32_t>(sizeof(T))};
            return value;
        }

        void Check(int32_t status)
        {
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                throw NativeImageFailure{status};
        }

        bool SameEpoch(notepads_markdown_image_epoch const &left, notepads_markdown_image_epoch const &right) noexcept
        {
            return left.source.revision == right.source.revision && left.source_generation == right.source_generation &&
                   left.provider_generation == right.provider_generation &&
                   std::equal(std::begin(left.source.document), std::end(left.source.document),
                              std::begin(right.source.document));
        }

        struct DecodeFailure : winrt::hresult_error
        {
            uint32_t kind;
            DecodeFailure(uint32_t value, HRESULT code) noexcept : winrt::hresult_error(code), kind(value)
            {
            }
        };

        void Require(bool condition, uint32_t failure, HRESULT code)
        {
            if (!condition)
                throw DecodeFailure{failure, code};
        }

        void CheckActive(std::shared_ptr<NativeImageRequest> const &request)
        {
            if (request->IsCancelled() || !request->IsCurrent())
                throw winrt::hresult_canceled();
        }

        struct StreamOwner
        {
            winrt::Windows::Storage::Streams::IRandomAccessStream stream;
            bool closed{};
            HRESULT Close() noexcept
            {
                if (closed || !stream)
                    return S_OK;
                closed = true;
                try
                {
                    stream.Close();
                    return S_OK;
                }
                catch (...)
                {
                    return winrt::to_hresult().value;
                }
            }
            ~StreamOwner()
            {
                (void)Close();
            }
        };

        void RetainedBound(void *owner) noexcept
        {
            notepads_markdown_svg_retained_bind_physical(static_cast<notepads_markdown_svg_retained *>(owner));
        }

        int32_t RetainedFreed(void *owner) noexcept
        {
            return notepads_markdown_svg_retained_after_physical_free_checked(static_cast<notepads_markdown_svg_retained *>(owner));
        }

        struct SvgAllocationContext
        {
            NativeSvgBorrowSet *borrows{};
            notepads_markdown_image_bytes const *image{};
            notepads_markdown_svg_retained *retained{};
            uint32_t kind{};
        };

        void *AllocateSvgBlock(SvgAllocationContext &context, ReadingMemory const &memory, size_t bytes)
        {
            if (!context.borrows || !context.borrows->CanAdd())
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
            auto error = CorePod<notepads_markdown_core_error>();
            PreparedReadingAllocation prepared;
            notepads_markdown_svg_work_token initial{};
            notepads_markdown_svg_retained *retained{};
            auto payload = context.kind == NOTEPADS_MARKDOWN_SVG_RETAINED_PAYLOAD ? bytes : 0;
            Check(notepads_markdown_svg_resources_retained_prepare(
                NOTEPADS_MARKDOWN_SVG_RESOURCES_ABI_VERSION, context.borrows->Resources(), context.kind, context.image,
                &prepared.lease, ReadingMemory::AllocationRequestBytes(bytes), payload, &retained, &initial, &error));
            prepared.action = {retained, RetainedFreed};
            prepared.bindPhysical = RetainedBound;
            context.retained = retained;
            context.borrows->Add(retained, initial);
            void *value{};
            try
            {
                value = memory.AllocatePrepared(bytes, prepared);
            }
            catch (...)
            {
                if (prepared.lease)
                {
                    // Keep the original row and real preview owner in the
                    // caller's fixed borrow set. Its one retirement checks
                    // return -> actual preview destroy -> canonical abort.
                    context.borrows->MarkUnallocated(retained, prepared.lease);
                }
                // A transferred descriptor instead belongs to a real prefix,
                // including when a subsequent HeapSize/admit/free failed.
                throw;
            }
            if (context.kind == NOTEPADS_MARKDOWN_SVG_RETAINED_METADATA)
            {
                auto status = notepads_markdown_svg_retained_admit(retained, &error);
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                {
                    ReadingMemory::Free(value);
                    throw NativeImageFailure{status};
                }
            }
            return value;
        }

        void *AllocateSvgMetadata(void *context, ReadingMemory const &memory, size_t bytes)
        {
            return AllocateSvgBlock(*static_cast<SvgAllocationContext *>(context), memory, bytes);
        }

        struct DecodeApartment
        {
            DecodeApartment()
            {
                winrt::check_hresult(::CoInitializeEx(nullptr, COINIT_MULTITHREADED));
            }
            ~DecodeApartment()
            {
                ::CoUninitialize();
            }
        };

        using ImageReadOperation = winrt::Windows::Foundation::IAsyncOperationWithProgress<
            winrt::Windows::Storage::Streams::IBuffer, uint32_t>;
        using ImageReadHandler = winrt::Windows::Foundation::AsyncOperationWithProgressCompletedHandler<
            winrt::Windows::Storage::Streams::IBuffer, uint32_t>;
        class ImageReadState;

        // This delegate shares the source allocator's physical-free contract.
        // Its weak state link cannot keep an ordinary completed decode alive.
        struct ImageReadCompleted final : winrt::impl::abi_t<ImageReadHandler>
        {
            std::atomic<uint32_t> references{1};
            ModuleLifetime module;
            std::weak_ptr<ImageReadState> state;
            explicit ImageReadCompleted(std::weak_ptr<ImageReadState> value) noexcept : state(std::move(value))
            {
            }
            int32_t __stdcall QueryInterface(winrt::guid const &id, void **output) noexcept override
            {
                if (!output)
                    return E_POINTER;
                *output = nullptr;
                if (id != winrt::guid_of<ImageReadHandler>() &&
                    id != winrt::guid_of<winrt::Windows::Foundation::IUnknown>() && id != winrt::guid_of<IAgileObject>())
                    return E_NOINTERFACE;
                *output = static_cast<winrt::impl::abi_t<ImageReadHandler> *>(this);
                AddRef();
                return S_OK;
            }
            uint32_t __stdcall AddRef() noexcept override
            {
                return references.fetch_add(1) + 1;
            }
            uint32_t __stdcall Release() noexcept override
            {
                auto remaining = references.fetch_sub(1, std::memory_order_acq_rel) - 1;
                if (!remaining)
                {
                    ModuleLifetime tail;
                    this->~ImageReadCompleted();
                    ReadingMemory::Free(this);
                }
                return remaining;
            }
            int32_t __stdcall Invoke(void *, int32_t status) noexcept override;
        };

        class ImageReadState final : public std::enable_shared_from_this<ImageReadState>
        {
          public:
            static constexpr uint32_t ChunkBytes = 64 * 1024;
            ImageReadState(std::shared_ptr<NativeImageRequest> request,
                           winrt::Windows::Storage::Streams::IRandomAccessStream stream)
                : _request(std::move(request)), _stream(std::move(stream)), _staging(ChunkBytes),
                  _completed(CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET, EVENT_MODIFY_STATE | SYNCHRONIZE)),
                  _cancel(CreateEventExW(nullptr, nullptr, CREATE_EVENT_MANUAL_RESET, EVENT_MODIFY_STATE | SYNCHRONIZE))
            {
                winrt::check_bool(static_cast<bool>(_completed));
                winrt::check_bool(static_cast<bool>(_cancel));
            }
            static std::shared_ptr<ImageReadState> Create(std::shared_ptr<NativeImageRequest> const &request,
                                                          std::shared_ptr<ReadingMemory> const &memory,
                                                          winrt::Windows::Storage::Streams::IRandomAccessStream const &stream)
            {
                auto state = std::allocate_shared<ImageReadState>(ReadingAllocator<ImageReadState>{memory}, request, stream);
                auto storage = memory->Allocate(sizeof(ImageReadCompleted));
                auto completed = new (storage) ImageReadCompleted(state);
                winrt::attach_abi(state->_handler, static_cast<winrt::impl::abi_t<ImageReadHandler> *>(completed));
                return state;
            }
            void Cancel() noexcept
            {
                _request->Cancel();
                _cancelled.store(true, std::memory_order_release);
                SetEvent(_cancel.get());
            }
            void Completed(int32_t status) noexcept
            {
                // C++/WinRT's setter can synchronously report a pre-cancelled
                // operation before its coroutine body has physically exited.
                // We never send Cancel until that setter has returned. An
                // already-cancelled provider operation has no trustworthy join.
                if (status == static_cast<int32_t>(winrt::Windows::Foundation::AsyncStatus::Started) ||
                    (status == static_cast<int32_t>(winrt::Windows::Foundation::AsyncStatus::Canceled) &&
                     _registering.load(std::memory_order_acquire)))
                    _uncertain.store(true, std::memory_order_release);
                SetEvent(_completed.get()); // Last state access; the callback owns a strong local pin.
            }
            winrt::Windows::Storage::Streams::IBuffer Read(uint32_t count)
            {
                DecodeApartment apartment; // No suspension or apartment switch in this scope.
                if (_cancelled.load(std::memory_order_acquire))
                    throw winrt::hresult_canceled();
                winrt::check_bool(ResetEvent(_completed.get()));
                _uncertain.store(false, std::memory_order_release);
                try
                {
                    _read = _stream.ReadAsync(_staging, count, winrt::Windows::Storage::Streams::InputStreamOptions::Partial);
                    if (!_read)
                        throw winrt::hresult_error(E_UNEXPECTED);
                    _registering.store(true, std::memory_order_release);
                    _read.Completed(_handler);
                    _registering.store(false, std::memory_order_release);
                    if (_uncertain.load(std::memory_order_acquire))
                        throw winrt::hresult_error(HRESULT_FROM_WIN32(ERROR_IO_INCOMPLETE));
                    HANDLE events[]{_completed.get(), _cancel.get()};
                    auto waited = WaitForMultipleObjectsEx(2, events, FALSE, INFINITE, FALSE);
                    if (waited == WAIT_OBJECT_0 + 1)
                    {
                        // The persistent cancel event covers ReadAsync creation
                        // and Completed registration. Only this MTA caller ever
                        // invokes child Cancel, and it returns before the join.
                        try
                        {
                            _read.Cancel();
                        }
                        catch (...)
                        {
                        }
                        waited = WaitForSingleObjectEx(_completed.get(), INFINITE, FALSE);
                    }
                    if (waited != WAIT_OBJECT_0)
                        throw winrt::hresult_error(waited == WAIT_FAILED ? HRESULT_FROM_WIN32(GetLastError()) : E_UNEXPECTED);
                    if (_uncertain.load(std::memory_order_acquire))
                        throw winrt::hresult_error(HRESULT_FROM_WIN32(ERROR_IO_INCOMPLETE));
                }
                catch (...)
                {
                    _registering.store(false, std::memory_order_release);
                    if (_read)
                    {
                        _retirementError = winrt::to_hresult();
                        // No observer proof: preserve this exact read, buffer,
                        // stream, original request permit and module. Bounded by
                        // the policy's original two physical IO slots.
                        _quarantine = shared_from_this();
                    }
                    throw;
                }
                // GetResults/COM releases are after the real Completed gate,
                // never an IAsyncInfo status check or a cancellation exception.
                auto retired = std::exchange(_read, nullptr);
                return retired.GetResults();
            }
            bool Quarantined() const noexcept
            {
                return static_cast<bool>(_quarantine);
            }
            HRESULT RetirementError() const noexcept
            {
                return _retirementError;
            }

          private:
            ModuleLifetime _module;
            std::shared_ptr<NativeImageRequest> _request;
            winrt::Windows::Storage::Streams::IRandomAccessStream _stream;
            winrt::Windows::Storage::Streams::Buffer _staging;
            winrt::handle _completed, _cancel;
            ImageReadHandler _handler{nullptr};
            ImageReadOperation _read{nullptr};
            std::atomic<bool> _cancelled{}, _registering{}, _uncertain{};
            HRESULT _retirementError{S_OK};
            std::shared_ptr<ImageReadState> _quarantine;
        };

        int32_t __stdcall ImageReadCompleted::Invoke(void *, int32_t status) noexcept
        {
            if (auto owner = state.lock())
                owner->Completed(status);
            return S_OK;
        }

        GUID DecoderClass(std::span<uint8_t const> bytes)
        {
            constexpr std::array<uint8_t, 8> png{137, 80, 78, 71, 13, 10, 26, 10};
            if (bytes.size() >= png.size() && std::equal(png.begin(), png.end(), bytes.begin()))
                return CLSID_WICPngDecoder2;
            if (bytes.size() >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff)
                return CLSID_WICJpegDecoder;
            if (bytes.size() >= 2 && bytes[0] == 'B' && bytes[1] == 'M')
                return CLSID_WICBmpDecoder;
            if (bytes.size() >= 6 &&
                (std::memcmp(bytes.data(), "GIF87a", 6) == 0 || std::memcmp(bytes.data(), "GIF89a", 6) == 0))
                return CLSID_WICGifDecoder;
            // Unknown bytes never reach installed codec discovery. An explicit
            // SVG worker capability may ask controlled XML/SVG prepare instead.
            return GUID_NULL;
        }

        template <typename ActiveCheck>
        bool IsXmlImageCandidate(std::span<uint8_t const> bytes, ActiveCheck checkActive)
        {
            checkActive();
            size_t offset = bytes.size() >= 3 && bytes[0] == 0xef && bytes[1] == 0xbb && bytes[2] == 0xbf ? 3 : 0;
            size_t sinceCheck = offset;
            for (; offset < bytes.size(); ++offset)
            {
                if (sinceCheck == 4096)
                {
                    checkActive();
                    sinceCheck = 0;
                }
                auto byte = bytes[offset];
                ++sinceCheck;
                if (byte != ' ' && byte != '\t' && byte != '\r' && byte != '\n')
                {
                    // Dispatch only. Controlled SVG prepare still validates
                    // UTF8, XML declarations/comments, root and capabilities.
                    auto candidate = byte == '<';
                    checkActive();
                    return candidate;
                }
            }
            checkActive();
            return false;
        }

        uint32_t Orientation(IWICBitmapFrameDecode *frame)
        {
            winrt::com_ptr<IWICMetadataQueryReader> reader;
            auto status = frame->GetMetadataQueryReader(reader.put());
            if (status == WINCODEC_ERR_UNSUPPORTEDOPERATION)
                return 1;
            winrt::check_hresult(status);
            PROPVARIANT value{};
            status = reader->GetMetadataByName(L"/app1/ifd/{ushort=274}", &value);
            uint32_t orientation = 1;
            if (SUCCEEDED(status))
            {
                if (value.vt == VT_UI2)
                    orientation = value.uiVal;
                else if (value.vt == VT_UI4)
                    orientation = value.ulVal;
                else
                    status = WINCODEC_ERR_BADMETADATAHEADER;
            }
            else if (status == WINCODEC_ERR_PROPERTYNOTFOUND || status == WINCODEC_ERR_PROPERTYNOTSUPPORTED)
                status = S_OK;
            auto cleanup = ::PropVariantClear(&value);
            winrt::check_hresult(status);
            winrt::check_hresult(cleanup);
            Require(orientation >= 1 && orientation <= 8, NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR, WINCODEC_ERR_BADMETADATAHEADER);
            return orientation;
        }

        WICBitmapTransformOptions Transform(uint32_t orientation)
        {
            switch (orientation)
            {
            case 1:
                return WICBitmapTransformRotate0;
            case 2:
                return WICBitmapTransformFlipHorizontal;
            case 3:
                return WICBitmapTransformRotate180;
            case 4:
                return WICBitmapTransformFlipVertical;
            case 5:
                return WICBitmapTransformRotate90;
            case 6:
                return WICBitmapTransformRotate90;
            case 7:
                return WICBitmapTransformRotate270;
            case 8:
                return WICBitmapTransformRotate270;
            default:
                throw DecodeFailure{NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR, WINCODEC_ERR_BADMETADATAHEADER};
            }
        }

        constexpr uint32_t MaximumAnimatedGifFrames = 120;
        constexpr uint64_t MaximumAnimatedGifBytes = 24ull * 1024 * 1024;
        constexpr uint64_t MaximumAnimatedGifCanvasPixels = 32ull * 1024 * 1024;

        struct GifMetadataValue
        {
            PROPVARIANT value{};
            ~GifMetadataValue() { (void)::PropVariantClear(&value); }
        };

        uint32_t GifNumber(IWICMetadataQueryReader *reader, wchar_t const *name, uint32_t fallback,
                           bool required = false)
        {
            GifMetadataValue property;
            const auto status = reader->GetMetadataByName(name, &property.value);
            if (!required && status == WINCODEC_ERR_PROPERTYNOTFOUND)
                return fallback;
            winrt::check_hresult(status);
            switch (property.value.vt)
            {
            case VT_UI1: return property.value.bVal;
            case VT_UI2: return property.value.uiVal;
            case VT_UI4: return property.value.ulVal;
            case VT_BOOL: return property.value.boolVal != VARIANT_FALSE;
            default: throw DecodeFailure{NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR, WINCODEC_ERR_BADMETADATAHEADER};
            }
        }

        uint32_t GifPlaythroughs(IWICBitmapDecoder *decoder)
        {
            winrt::com_ptr<IWICMetadataQueryReader> reader;
            winrt::check_hresult(decoder->GetMetadataQueryReader(reader.put()));
            // WIC exposes application blocks through a numbered metadata path.
            // Only the recognized loop extension controls playback.
            for (uint32_t index = 0; index != 16; ++index)
            {
                std::array<wchar_t, 48> name{};
                swprintf_s(name.data(), name.size(), L"/[%u]appext/Application", index);
                GifMetadataValue application;
                const auto status = reader->GetMetadataByName(name.data(), &application.value);
                if (status == WINCODEC_ERR_PROPERTYNOTFOUND)
                    return 1;
                winrt::check_hresult(status);
                if (application.value.vt != (VT_UI1 | VT_VECTOR) || application.value.caub.cElems != 11 ||
                    !application.value.caub.pElems ||
                    (std::memcmp(application.value.caub.pElems, "NETSCAPE2.0", 11) != 0 &&
                     std::memcmp(application.value.caub.pElems, "ANIMEXTS1.0", 11) != 0))
                    continue;
                swprintf_s(name.data(), name.size(), L"/[%u]appext/Data", index);
                GifMetadataValue data;
                const auto dataStatus = reader->GetMetadataByName(name.data(), &data.value);
                if (dataStatus == WINCODEC_ERR_PROPERTYNOTFOUND)
                    return 1;
                winrt::check_hresult(dataStatus);
                if (data.value.vt != (VT_UI1 | VT_VECTOR) || data.value.caub.cElems < 4 || !data.value.caub.pElems ||
                    data.value.caub.pElems[0] != 3 || data.value.caub.pElems[1] != 1)
                    return 1;
                auto const *bytes = data.value.caub.pElems;
                auto repeats = uint32_t{bytes[2]} | (uint32_t{bytes[3]} << 8);
                return repeats ? repeats + 1 : 0;
            }
            return 1;
        }

        std::shared_ptr<ImagePixels> AllocateRaster(
            std::shared_ptr<NativeImageRequest> const &request, std::shared_ptr<ReadingMemory> const &memory, size_t capacity)
        {
            auto charge = request->Reserve(NOTEPADS_MARKDOWN_IMAGE_DECODED, capacity);
            std::shared_ptr<ImagePixels> pixels;
            {
                ImagePayloadConstruction construction{*memory};
                pixels = std::allocate_shared<ImagePixels>(ImagePayloadAllocator<ImagePixels>{construction.Nonce()}, memory, charge);
            }
            pixels->AllocatePixels(*memory, capacity);
            pixels->BindRasterAllocation();
            charge->Admit(pixels->pixels.capacity());
            return pixels;
        }

        struct GifFrame
        {
            uint32_t left{}, top{}, width{}, height{};
            uint32_t disposal{}, delay{};
            bool transparent{};
        };

        void ScaleGifCanvas(std::shared_ptr<NativeImageRequest> const &request, std::span<uint8_t const> source,
                            uint32_t width, uint32_t height, std::span<uint8_t> output,
                            uint32_t decodedWidth, uint32_t decodedHeight)
        {
            // Average premultiplied channels over the authored source area.
            // Integer weights preserve thin updates on either side of a
            // rounded output row, including partially transparent pixels.
            const uint64_t totalWeight = uint64_t{width} * height;
            for (uint32_t y = 0; y < decodedHeight; ++y)
            {
                if ((y & 7) == 0)
                    CheckActive(request);
                const auto top = y * height;
                const auto bottom = top + height;
                for (uint32_t x = 0; x < decodedWidth; ++x)
                {
                    const auto left = x * width;
                    const auto right = left + width;
                    std::array<uint64_t, 4> color{};
                    for (uint32_t sy = top / decodedHeight; sy <= (bottom - 1) / decodedHeight; ++sy)
                    {
                        const auto vertical = std::min(bottom, (sy + 1) * decodedHeight) - std::max(top, sy * decodedHeight);
                        for (uint32_t sx = left / decodedWidth; sx <= (right - 1) / decodedWidth; ++sx)
                        {
                            const auto horizontal = std::min(right, (sx + 1) * decodedWidth) - std::max(left, sx * decodedWidth);
                            const auto weight = uint64_t{vertical} * horizontal;
                            auto pixel = source.data() + (size_t{sy} * width + sx) * 4;
                            for (size_t channel = 0; channel < color.size(); ++channel)
                                color[channel] += weight * pixel[channel];
                        }
                    }
                    auto pixel = output.data() + (size_t{y} * decodedWidth + x) * 4;
                    for (size_t channel = 0; channel < color.size(); ++channel)
                        pixel[channel] = static_cast<uint8_t>((color[channel] + totalWeight / 2) / totalWeight);
                }
            }
        }

        std::shared_ptr<ImagePixels> DecodeGif(std::shared_ptr<NativeImageRequest> const &request,
            std::shared_ptr<ReadingMemory> const &memory, IWICImagingFactory *factory, IWICBitmapDecoder *decoder,
            std::span<uint8_t const> encoded, uint32_t width, uint32_t height)
        {
            auto intrinsic = MakeRasterImageMetadata({width, height, width, height, 1, width * 4});
            auto plan = ImagePod<notepads_markdown_image_raster_plan>();
            auto error = CorePod<notepads_markdown_core_error>();
            Check(notepads_markdown_image_plan_raster(&intrinsic, &request->Target(), &plan, &error));
            const auto longest = std::max(width, height);
            const auto side = std::min(plan.maximum_side, longest);
            const auto decodedWidth = std::max(1u, static_cast<uint32_t>(uint64_t{width} * side / longest));
            const auto decodedHeight = std::max(1u, static_cast<uint32_t>(uint64_t{height} * side / longest));
            const auto stride = decodedWidth * 4;
            const auto frameBytes = size_t{stride} * decodedHeight;
            const auto canvasStride = width * 4;
            const auto canvasBytes = size_t{canvasStride} * height;
            const bool scaled = decodedWidth != width || decodedHeight != height;
            auto metadata = MakeRasterImageMetadata({width, height, decodedWidth, decodedHeight, 1, stride});
            request->NoteMetadata(metadata);
            UINT frameCount{};
            winrt::check_hresult(decoder->GetFrameCount(&frameCount));
            Require(frameCount > 0, NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR, WINCODEC_ERR_BADIMAGE);
            if (frameCount > MaximumAnimatedGifFrames ||
                uint64_t{frameCount} * (frameBytes + sizeof(uint32_t)) > MaximumAnimatedGifBytes ||
                uint64_t{frameCount} * width * height > MaximumAnimatedGifCanvasPixels)
                frameCount = 1;
            const size_t scheduleBytes = frameCount > 1 ? size_t{frameCount} * sizeof(uint32_t) : 0;
            auto pixels = AllocateRaster(request, memory, size_t{frameCount} * frameBytes + scheduleBytes);
            pixels->metadata = metadata;
            auto output = pixels->MutablePixels();

            std::array<uint8_t, 4> background{};
            if (encoded[10] & 0x80)
            {
                const size_t colors = size_t{2} << (encoded[10] & 7);
                Require(encoded.size() >= 13 + colors * 3, NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR, WINCODEC_ERR_BADIMAGE);
                if (encoded[11] < colors)
                {
                    auto color = &encoded[13 + size_t{encoded[11]} * 3];
                    background = {color[2], color[1], color[0], 255};
                }
            }
            const std::array<uint8_t, 4> transparent{};
            auto fill = [&](std::span<uint8_t> canvas, GifFrame const &rect, std::array<uint8_t, 4> const &color)
            {
                for (uint32_t y = 0; y < rect.height; ++y)
                {
                    if ((y & 31) == 0)
                        CheckActive(request);
                    auto row = canvas.subspan(size_t{rect.top + y} * canvasStride + rect.left * 4, rect.width * 4);
                    for (size_t x = 0; x < row.size(); x += 4)
                        std::memcpy(row.data() + x, color.data(), color.size());
                }
            };
            // Each authored canvas obeys the pixel-block cap independently.
            // Their combined bytes still share the request/process budgets.
            std::shared_ptr<ImagePixels> canvasStorage, patchStorage, previousStorage;
            auto scratch = [&](std::shared_ptr<ImagePixels> &storage) -> std::span<uint8_t>
            {
                if (!storage)
                    storage = AllocateRaster(request, memory, canvasBytes);
                return storage->MutablePixels();
            };
            auto previousCanvas = [&] { return scratch(previousStorage); };
            std::array<uint32_t, MaximumAnimatedGifFrames> ends{};
            GifFrame prior{};
            for (uint32_t index = 0; index < frameCount; ++index)
            {
                CheckActive(request);
                winrt::com_ptr<IWICBitmapFrameDecode> frame;
                winrt::check_hresult(decoder->GetFrame(index, frame.put()));
                winrt::com_ptr<IWICMetadataQueryReader> reader;
                winrt::check_hresult(frame->GetMetadataQueryReader(reader.put()));
                const auto left = GifNumber(reader.get(), L"/imgdesc/Left", 0, true);
                const auto top = GifNumber(reader.get(), L"/imgdesc/Top", 0, true);
                UINT frameWidth{}, frameHeight{};
                winrt::check_hresult(frame->GetSize(&frameWidth, &frameHeight));
                Require(frameWidth && frameHeight && left < width && frameWidth <= width - left &&
                            top < height && frameHeight <= height - top,
                        NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR, WINCODEC_ERR_BADIMAGE);
                auto rect = GifFrame{left, top, frameWidth, frameHeight,
                    GifNumber(reader.get(), L"/grctlext/Disposal", 0),
                    GifNumber(reader.get(), L"/grctlext/Delay", 0),
                    GifNumber(reader.get(), L"/grctlext/TransparencyFlag", 0) != 0};
                Require(rect.delay <= UINT16_MAX, NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR, WINCODEC_ERR_BADMETADATAHEADER);
                // Browser treatment for missing or nominally zero delay.
                const auto milliseconds = rect.delay < 2 ? 100u : rect.delay * 10;
                ends[index] = index ? ends[index - 1] + milliseconds : milliseconds;

                auto rendered = output.subspan(size_t{index} * frameBytes, frameBytes);
                const bool fullFirst = index == 0 && !rect.left && !rect.top && rect.width == width && rect.height == height;
                // A whole-canvas still needs only the target-size output. When
                // composition is required, keep the authored pixel grid until
                // all frame updates and disposals have been applied.
                const bool scaleDirect = scaled && frameCount == 1 && fullFirst;
                auto canvas = scaled && !scaleDirect ? scratch(canvasStorage) : rendered;
                if (!scaleDirect)
                {
                    if (index)
                    {
                        if (prior.disposal == 3)
                            std::memcpy(canvas.data(), previousCanvas().data(), canvasBytes);
                        else if (!scaled)
                            std::memcpy(canvas.data(), output.data() + size_t{index - 1} * frameBytes, canvasBytes);
                        if (prior.disposal == 2)
                            fill(canvas, prior, prior.transparent ? transparent : background);
                    }
                    else if (!fullFirst || (frameCount > 1 && rect.disposal == 3))
                        fill(canvas, {0, 0, width, height}, rect.transparent ? transparent : background);
                    if (index + 1 < frameCount && rect.disposal == 3)
                        std::memcpy(previousCanvas().data(), canvas.data(), canvasBytes);
                }

                winrt::com_ptr<IWICFormatConverter> converter;
                winrt::check_hresult(factory->CreateFormatConverter(converter.put()));
                winrt::check_hresult(converter->Initialize(frame.get(), GUID_WICPixelFormat32bppPBGRA,
                    WICBitmapDitherTypeNone, nullptr, 0, WICBitmapPaletteTypeCustom));
                if (scaleDirect)
                {
                    winrt::com_ptr<IWICBitmapScaler> scaler;
                    winrt::check_hresult(factory->CreateBitmapScaler(scaler.put()));
                    winrt::check_hresult(scaler->Initialize(converter.get(), decodedWidth, decodedHeight, WICBitmapInterpolationModeFant));
                    winrt::check_hresult(scaler->CopyPixels(nullptr, stride, static_cast<UINT>(rendered.size()), rendered.data()));
                }
                else
                {
                    const auto patchStride = rect.width * 4;
                    auto patch = fullFirst ? canvas : scratch(patchStorage).first(size_t{patchStride} * rect.height);
                    winrt::check_hresult(converter->CopyPixels(nullptr, patchStride, static_cast<UINT>(patch.size()), patch.data()));
                    if (!fullFirst)
                        for (uint32_t row = 0; row < rect.height; ++row)
                        {
                            if ((row & 31) == 0)
                                CheckActive(request);
                            auto destination = canvas.subspan(size_t{rect.top + row} * canvasStride + rect.left * 4, patchStride);
                            auto input = patch.subspan(size_t{row} * patchStride, patchStride);
                            for (size_t channel = 0; channel < patchStride; channel += 4)
                            {
                                const auto inverse = 255u - input[channel + 3];
                                for (size_t component = channel; component < channel + 4; ++component)
                                    destination[component] = static_cast<uint8_t>(std::min(255u,
                                        input[component] + (destination[component] * inverse + 127) / 255));
                            }
                        }
                    if (scaled)
                        ScaleGifCanvas(request, canvas, width, height, rendered, decodedWidth, decodedHeight);
                }
                prior = rect;
            }
            CheckActive(request);
            if (frameCount > 1)
                pixels->SetAnimation(std::span<uint32_t const>{ends}.first(frameCount), GifPlaythroughs(decoder));
            pixels->Freeze();
            return pixels;
        }

        std::shared_ptr<ImagePixels> Decode(std::shared_ptr<NativeImageRequest> const &request,
                                            std::shared_ptr<ReadingMemory> const &memory,
                                            std::span<uint8_t const> encoded, uint32_t maximumDecodeSide)
        {
            // This synchronous scope has no await; initialization and balanced
            // uninitialization always run on the same worker thread.
            DecodeApartment apartment;
            CheckActive(request);
            auto decoderClass = DecoderClass(encoded);
            Require(!::IsEqualGUID(decoderClass, GUID_NULL), NOTEPADS_MARKDOWN_IMAGE_UNSUPPORTED, WINCODEC_ERR_COMPONENTNOTFOUND);
            uint32_t gifWidth{}, gifHeight{};
            auto gif = ::IsEqualGUID(decoderClass, CLSID_WICGifDecoder) != 0;
            if (gif)
            {
                Require(encoded.size() >= 13, NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR, WINCODEC_ERR_BADIMAGE);
                gifWidth = uint32_t{encoded[6]} | (uint32_t{encoded[7]} << 8);
                gifHeight = uint32_t{encoded[8]} | (uint32_t{encoded[9]} << 8);
                Require(gifWidth != 0 && gifHeight != 0, NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR, WINCODEC_ERR_BADIMAGE);
                Require(gifWidth <= ImageMaximumIntrinsicSide && gifHeight <= ImageMaximumIntrinsicSide &&
                            uint64_t{gifWidth} * gifHeight <= ImageMaximumPixelCount,
                        NOTEPADS_MARKDOWN_IMAGE_QUOTA, HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA));
            }
            winrt::com_ptr<IWICImagingFactory> factory;
            winrt::check_hresult(::CoCreateInstance(CLSID_WICImagingFactory2, nullptr, CLSCTX_INPROC_SERVER,
                                                    __uuidof(IWICImagingFactory), factory.put_void()));
            winrt::com_ptr<IWICStream> stream;
            winrt::check_hresult(factory->CreateStream(stream.put()));
            Require(encoded.size() <= UINT32_MAX, NOTEPADS_MARKDOWN_IMAGE_QUOTA, HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA));
            // WIC accepts a mutable pointer but this read-only decoder never
            // writes it. The encoded owner outlives all WIC objects in this call.
            winrt::check_hresult(
                stream->InitializeFromMemory(const_cast<BYTE *>(encoded.data()), static_cast<DWORD>(encoded.size())));
            winrt::com_ptr<IWICBitmapDecoder> decoder;
            winrt::check_hresult(::CoCreateInstance(decoderClass, nullptr, CLSCTX_INPROC_SERVER,
                                                    __uuidof(IWICBitmapDecoder), decoder.put_void()));
            winrt::check_hresult(decoder->Initialize(stream.get(), WICDecodeMetadataCacheOnDemand));
            if (gif)
                return DecodeGif(request, memory, factory.get(), decoder.get(), encoded, gifWidth, gifHeight);
            winrt::com_ptr<IWICBitmapFrameDecode> frame;
            winrt::check_hresult(decoder->GetFrame(0, frame.put()));
            UINT width{}, height{};
            winrt::check_hresult(frame->GetSize(&width, &height));
            Require(width != 0 && height != 0, NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR, WINCODEC_ERR_BADIMAGE);
            Require(width <= ImageMaximumIntrinsicSide && height <= ImageMaximumIntrinsicSide &&
                        uint64_t{width} * height <= ImageMaximumPixelCount,
                    NOTEPADS_MARKDOWN_IMAGE_QUOTA, HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA));
            auto orientation = ::IsEqualGUID(decoderClass, CLSID_WICJpegDecoder) ? Orientation(frame.get()) : 1u;
            CheckActive(request);
            winrt::com_ptr<IWICBitmapSource> source = frame.as<IWICBitmapSource>();
            auto transform = Transform(orientation);
            if (transform != WICBitmapTransformRotate0)
            {
                winrt::com_ptr<IWICBitmapFlipRotator> rotated;
                winrt::check_hresult(factory->CreateBitmapFlipRotator(rotated.put()));
                winrt::check_hresult(rotated->Initialize(source.get(), transform));
                source = rotated.as<IWICBitmapSource>();
            }
            if (orientation == 5 || orientation == 7)
            {
                // Specify rotation then horizontal mirroring as two operations;
                // do not depend on a combined flag's transform ordering.
                winrt::com_ptr<IWICBitmapFlipRotator> mirrored;
                winrt::check_hresult(factory->CreateBitmapFlipRotator(mirrored.put()));
                winrt::check_hresult(mirrored->Initialize(source.get(), WICBitmapTransformFlipHorizontal));
                source = mirrored.as<IWICBitmapSource>();
            }
            UINT orientedWidth{}, orientedHeight{};
            winrt::check_hresult(source->GetSize(&orientedWidth, &orientedHeight));
            auto expectedWidth = orientation >= 5 ? height : width;
            auto expectedHeight = orientation >= 5 ? width : height;
            Require(orientedWidth == expectedWidth && orientedHeight == expectedHeight, NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR,
                    WINCODEC_ERR_BADIMAGE);
            {
                auto intrinsic = MakeRasterImageMetadata({width, height, orientedWidth, orientedHeight, orientation, orientedWidth * 4});
                auto plan = ImagePod<notepads_markdown_image_raster_plan>();
                auto error = CorePod<notepads_markdown_core_error>();
                Check(notepads_markdown_image_plan_raster(&intrinsic, &request->Target(), &plan, &error));
                maximumDecodeSide = plan.maximum_side;
            }
            auto longest = std::max(orientedWidth, orientedHeight);
            auto decodedWidth = orientedWidth;
            auto decodedHeight = orientedHeight;
            if (longest > maximumDecodeSide)
            {
                // Both factors were validated <=8192/4096 before this multiply.
                decodedWidth =
                    std::max<UINT>(1, static_cast<UINT>(uint64_t{orientedWidth} * maximumDecodeSide / longest));
                decodedHeight =
                    std::max<UINT>(1, static_cast<UINT>(uint64_t{orientedHeight} * maximumDecodeSide / longest));
                winrt::com_ptr<IWICBitmapScaler> scaled;
                winrt::check_hresult(factory->CreateBitmapScaler(scaled.put()));
                winrt::check_hresult(
                    scaled->Initialize(source.get(), decodedWidth, decodedHeight, WICBitmapInterpolationModeFant));
                source = scaled.as<IWICBitmapSource>();
            }
            auto stride64 = uint64_t{decodedWidth} * 4;
            Require(decodedHeight != 0 && stride64 <= UINT32_MAX && stride64 <= UINT64_MAX / decodedHeight,
                    NOTEPADS_MARKDOWN_IMAGE_QUOTA, HRESULT_FROM_WIN32(ERROR_ARITHMETIC_OVERFLOW));
            auto capacity = stride64 * decodedHeight;
            Require(capacity <= UINT32_MAX && capacity <= SIZE_MAX &&
                        uint64_t{decodedWidth} * decodedHeight <= ImageMaximumPixelCount,
                    NOTEPADS_MARKDOWN_IMAGE_QUOTA, HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA));
            winrt::com_ptr<IWICFormatConverter> converter;
            winrt::check_hresult(factory->CreateFormatConverter(converter.put()));
            winrt::check_hresult(converter->Initialize(source.get(), GUID_WICPixelFormat32bppPBGRA,
                                                       WICBitmapDitherTypeNone, nullptr, 0,
                                                       WICBitmapPaletteTypeCustom));
            CheckActive(request);
            auto metadata = MakeRasterImageMetadata({width, height, decodedWidth, decodedHeight, orientation, static_cast<uint32_t>(stride64)});
            request->NoteMetadata(metadata);
            auto pixels = AllocateRaster(request, memory, static_cast<size_t>(capacity));
            pixels->metadata = metadata;
            // WIC itself is not cooperatively preemptible. Only checked outer
            // boundaries prevent obsolete publication and bound our allocations.
            winrt::check_hresult(converter->CopyPixels(nullptr, pixels->metadata.stride, static_cast<UINT>(capacity),
                                                       pixels->MutablePixels().data()));
            CheckActive(request);
            pixels->Freeze();
            return pixels;
        }

        bool HasFailure(NativeImageDecodeResult const &result) noexcept
        {
            return result.failure != NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE || FAILED(result.error) || result.nativeStatus != NOTEPADS_MARKDOWN_CORE_OK;
        }

        void RecordDecodeFailure(NativeImageDecodeResult &result, uint32_t failure, HRESULT error) noexcept
        {
            if (!HasFailure(result))
            {
                result.failure = failure;
                result.error = error;
            }
        }

        void RecordNativeFailure(NativeImageDecodeResult &result, int32_t status) noexcept
        {
            if (HasFailure(result))
                return;
            result.nativeStatus = status;
            result.error = NativeStatusHresult(status).value;
            if (status == NOTEPADS_MARKDOWN_CORE_CANCELLED || status == NOTEPADS_MARKDOWN_CORE_STALE_RESULT)
            {
                result.failure = NOTEPADS_MARKDOWN_IMAGE_CANCELLED;
                result.error = HRESULT_FROM_WIN32(ERROR_CANCELLED);
            }
            else if (status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT || status == NOTEPADS_MARKDOWN_CORE_OUT_OF_MEMORY ||
                     status == NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW)
                result.failure = NOTEPADS_MARKDOWN_IMAGE_QUOTA;
            else
                result.failure = NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR;
        }
    }

    notepads_markdown_image_metadata MakeRasterImageMetadata(RasterImageMetadata const &value) noexcept
    {
        notepads_markdown_image_metadata result{};
        result.intrinsic_width = value.intrinsic_width;
        result.intrinsic_height = value.intrinsic_height;
        result.decoded_width = value.decoded_width;
        result.decoded_height = value.decoded_height;
        result.orientation = value.orientation;
        result.stride = value.stride;
        result.natural_width_dip = value.orientation >= 5 ? value.intrinsic_height : value.intrinsic_width;
        result.natural_height_dip = value.orientation >= 5 ? value.intrinsic_width : value.intrinsic_height;
        result.source_width = value.decoded_width;
        result.source_height = value.decoded_height;
        return result;
    }

    notepads_markdown_image_metadata MakeSvgImageMetadata(notepads_markdown_svg_job_plan const &plan) noexcept
    {
        notepads_markdown_image_metadata result{};
        result.kind = NOTEPADS_MARKDOWN_IMAGE_SVG;
        result.flags = plan.resolution_limited;
        result.decoded_width = plan.width;
        result.decoded_height = plan.height;
        result.orientation = 1;
        result.stride = plan.stride;
        result.natural_width_dip = plan.natural_width_dip;
        result.natural_height_dip = plan.natural_height_dip;
        result.source_width = plan.extent_width;
        result.source_height = plan.extent_height;
        result.viewport_width_bits = plan.viewport_width_bits;
        result.viewport_height_bits = plan.viewport_height_bits;
        result.scale_bits = plan.scale_bits;
        return result;
    }

    bool SameImageMetadata(notepads_markdown_image_metadata const &a, notepads_markdown_image_metadata const &b) noexcept
    {
        return a.kind == b.kind && a.flags == b.flags && a.intrinsic_width == b.intrinsic_width &&
               a.intrinsic_height == b.intrinsic_height && a.decoded_width == b.decoded_width &&
               a.decoded_height == b.decoded_height && a.orientation == b.orientation && a.stride == b.stride &&
               a.natural_width_dip == b.natural_width_dip && a.natural_height_dip == b.natural_height_dip &&
               a.source_width == b.source_width && a.source_height == b.source_height &&
               a.viewport_width_bits == b.viewport_width_bits && a.viewport_height_bits == b.viewport_height_bits &&
               a.scale_bits == b.scale_bits && a.reserved == b.reserved;
    }

    NativeSvgBorrowSet::NativeSvgBorrowSet(notepads_markdown_svg_resources *resources) noexcept : _resources(resources)
    {
    }

    NativeSvgBorrowSet::NativeSvgBorrowSet(NativeSvgBorrowSet &&other) noexcept
        : _resources(std::exchange(other._resources, nullptr)), _tokens(other._tokens), _owners(other._owners),
          _unallocated(other._unallocated), _returned(other._returned), _count(std::exchange(other._count, 0)),
          _failure(std::exchange(other._failure, NOTEPADS_MARKDOWN_CORE_OK))
    {
    }

    NativeSvgBorrowSet &NativeSvgBorrowSet::operator=(NativeSvgBorrowSet &&other) noexcept
    {
        if (this != &other && Return() == NOTEPADS_MARKDOWN_CORE_OK)
        {
            _resources = std::exchange(other._resources, nullptr);
            _tokens = other._tokens;
            _owners = other._owners;
            _unallocated = other._unallocated;
            _returned = other._returned;
            _count = std::exchange(other._count, 0);
            _failure = std::exchange(other._failure, NOTEPADS_MARKDOWN_CORE_OK);
        }
        return *this;
    }

    NativeSvgBorrowSet::~NativeSvgBorrowSet()
    {
        (void)Return();
    }

    notepads_markdown_svg_resources *NativeSvgBorrowSet::Resources() const noexcept
    {
        return _resources;
    }

    bool NativeSvgBorrowSet::CanAdd() const noexcept
    {
        return _resources && _failure == NOTEPADS_MARKDOWN_CORE_OK && _count < _tokens.size();
    }

    void NativeSvgBorrowSet::Add(notepads_markdown_svg_retained *owner, notepads_markdown_svg_work_token token) noexcept
    {
        // All callers preflight capacity before publishing a new Rust row.
        _tokens[_count] = token;
        _owners[_count] = owner;
        _unallocated[_count] = nullptr;
        _returned[_count] = false;
        ++_count;
    }

    void NativeSvgBorrowSet::MarkUnallocated(notepads_markdown_svg_retained *owner, notepads_markdown_preview_allocation *root) noexcept
    {
        for (uint32_t index = 0; index < _count; ++index)
            if (_owners[index] == owner)
            {
                _unallocated[index] = root;
                return;
            }
        _failure = NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
    }

    int32_t NativeSvgBorrowSet::VerifyPhysicalRetired() const noexcept
    {
        if (_failure != NOTEPADS_MARKDOWN_CORE_OK)
            return _failure;
        for (uint32_t index = 0; index < _count; ++index)
        {
            if (_unallocated[index])
                continue;
            notepads_markdown_svg_retained_snapshot value{};
            value.header = {NOTEPADS_MARKDOWN_SVG_RETAINED_ABI_VERSION, sizeof(value)};
            auto error = CorePod<notepads_markdown_core_error>();
            auto status = notepads_markdown_svg_retained_read(_owners[index], &value, &error);
            if (status != NOTEPADS_MARKDOWN_CORE_OK)
                return status;
            // Layout fields describe the original allocation permanently;
            // phase is the canonical physical/root retirement marker.
            if (value.phase != 3)
                return NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
        }
        return NOTEPADS_MARKDOWN_CORE_OK;
    }

    int32_t NativeSvgBorrowSet::Return() noexcept
    {
        if (_failure != NOTEPADS_MARKDOWN_CORE_OK)
            return _failure;
        while (_count)
        {
            auto index = _count - 1;
            auto error = CorePod<notepads_markdown_core_error>();
            if (!_returned[index])
            {
                auto status = notepads_markdown_svg_resources_return_retained(_resources, &_tokens[index], &error);
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                    return _failure = status;
                _returned[index] = true;
            }
            if (_unallocated[index])
            {
                auto root = _unallocated[index];
                auto status = notepads_markdown_preview_allocation_destroy(root);
                ReadingFreeScope::ObserveRootRelease(root, status);
                if (status != NOTEPADS_MARKDOWN_CORE_OK)
                    return _failure = status;
                _unallocated[index] = nullptr;
                notepads_markdown_svg_retained_abort_unallocated(_owners[index]);
            }
            _owners[index] = nullptr;
            --_count;
        }
        _resources = nullptr;
        return NOTEPADS_MARKDOWN_CORE_OK;
    }

    NativeEncodedImageHandle::NativeEncodedImageHandle(NativeEncodedImage *value) noexcept : _value(value)
    {
    }

    NativeEncodedImageHandle::NativeEncodedImageHandle(NativeEncodedImageHandle const &other) : _value(other._value)
    {
        if (_value)
            _value->AddRef();
    }

    NativeEncodedImageHandle::NativeEncodedImageHandle(NativeEncodedImageHandle &&other) noexcept
        : _value(std::exchange(other._value, nullptr))
    {
    }

    NativeEncodedImageHandle &NativeEncodedImageHandle::operator=(NativeEncodedImageHandle const &other)
    {
        if (this != &other)
        {
            auto copy = other;
            std::swap(_value, copy._value);
        }
        return *this;
    }

    NativeEncodedImageHandle &NativeEncodedImageHandle::operator=(NativeEncodedImageHandle &&other) noexcept
    {
        if (this != &other)
        {
            Reset();
            _value = std::exchange(other._value, nullptr);
        }
        return *this;
    }

    NativeEncodedImageHandle::~NativeEncodedImageHandle()
    {
        Reset();
    }

    NativeEncodedImage *NativeEncodedImageHandle::Get() const noexcept
    {
        return _value;
    }

    NativeEncodedImage *NativeEncodedImageHandle::operator->() const noexcept
    {
        return _value;
    }

    NativeEncodedImageHandle::operator bool() const noexcept
    {
        return _value != nullptr;
    }

    bool NativeEncodedImageHandle::IsUnique() const noexcept
    {
        return _value && _value->IsUnique();
    }
    std::shared_ptr<NativeImageBytes> NativeEncodedImageHandle::Charge() const noexcept
    {
        return _value ? _value->_charge : nullptr;
    }

    void NativeEncodedImageHandle::Reset() noexcept
    {
        (void)ResetAndWasLast();
    }

    bool NativeEncodedImageHandle::ResetAndWasLast() noexcept
    {
        auto value = std::exchange(_value, nullptr);
        return value && value->Release();
    }

    struct NativeEncodedImage::RawRetirement
    {
        std::shared_ptr<NativeImageBytes> charge;
        ModuleLifetime module;
    };

    int32_t NativeEncodedImage::RawFreed(void *context) noexcept
    {
        auto retirement = static_cast<RawRetirement *>(context);
        auto charge = std::move(retirement->charge);
        ModuleLifetime tail;
        retirement->~RawRetirement();
        return ReadingMemory::TryFree(retirement) ? NOTEPADS_MARKDOWN_CORE_OK : NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
    }

    NativeEncodedImage::NativeEncodedImage(std::shared_ptr<NativeImageBytes> charge,
                                           size_t capacity, RawRetirement *retirement) noexcept
        : _charge(std::move(charge)), _capacity(capacity), _rawRetirement(retirement)
    {
    }

    NativeEncodedImageHandle NativeEncodedImage::Create(std::shared_ptr<ReadingMemory> memory,
                                                        std::shared_ptr<NativeImageBytes> charge, size_t capacity)
    {
        if (!memory || !charge || capacity == 0 || capacity > ImageMaximumEncodedBytes ||
            capacity > SIZE_MAX - sizeof(NativeEncodedImage))
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        // Prepay the independent retirement record before any raw encoded
        // bytes exist. A failed payload/root free cannot release its subquota.
        auto retirement = ::new (memory->Allocate(sizeof(RawRetirement))) RawRetirement{charge};
        void *allocation{};
        try
        {
            allocation = memory->Allocate(sizeof(NativeEncodedImage) + capacity);
        }
        catch (...)
        {
            (void)RawFreed(retirement); // No encoded allocation was published.
            throw;
        }
        auto image = ::new (allocation) NativeEncodedImage{std::move(charge), capacity, retirement};
        ReadingMemory::BindFreeAction(image, {retirement, RawFreed});
        NativeEncodedImageHandle result{image};
        image->_charge->Admit(capacity);
        return result;
    }

    void NativeEncodedImage::AddRef()
    {
        auto previous = _references.load(std::memory_order_relaxed);
        for (;;)
        {
            if (previous == UINT32_MAX)
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_ARITHMETIC_OVERFLOW};
            if (_references.compare_exchange_weak(previous, previous + 1, std::memory_order_relaxed))
                return;
        }
    }

    bool NativeEncodedImage::Release() noexcept
    {
        if (_references.fetch_sub(1, std::memory_order_acq_rel) == 1)
        {
            // The prefix and original retirement owner carry all funding even
            // if physical free fails after this object's fields die.
            auto charge = std::move(_charge);
            this->~NativeEncodedImage();
            ReadingMemory::Free(this);
            return true;
        }
        return false;
    }

    uint8_t *NativeEncodedImage::Data() noexcept
    {
        return reinterpret_cast<uint8_t *>(this + 1);
    }

    std::span<uint8_t const> NativeEncodedImage::Bytes() const noexcept
    {
        return {reinterpret_cast<uint8_t const *>(this + 1), _size};
    }

    size_t NativeEncodedImage::Capacity() const noexcept
    {
        return _capacity;
    }

    void NativeEncodedImage::Append(std::span<uint8_t const> bytes)
    {
        if (_frozen || _retained || _references.load(std::memory_order_acquire) != 1 || bytes.size() > _capacity - _size)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        if (!bytes.empty())
            std::memcpy(Data() + _size, bytes.data(), bytes.size());
        _size += bytes.size();
    }

    void NativeEncodedImage::Freeze()
    {
        if (_frozen)
            return;
        if (_references.load(std::memory_order_acquire) != 1)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        _frozen = true;
    }

    bool NativeEncodedImage::IsFrozen() const noexcept
    {
        return _frozen;
    }

    void NativeEncodedImage::AdoptSvg(NativeSvgBorrowSet &borrows)
    {
        auto info = ReadingMemory::InspectAllocation(this);
        if (_retained || !_rawRetirement || !info.hasAction || info.action.context != _rawRetirement ||
            info.action.afterFree != RawFreed || !borrows.CanAdd())
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        auto error = CorePod<notepads_markdown_core_error>();
        notepads_markdown_svg_work_token initial{};
        notepads_markdown_svg_retained *retained{};
        Check(notepads_markdown_svg_resources_retained_prepare(NOTEPADS_MARKDOWN_SVG_RESOURCES_ABI_VERSION, borrows.Resources(),
                                                NOTEPADS_MARKDOWN_SVG_RETAINED_PAYLOAD, _charge->Handle(), &info.lease,
                                                info.requestedTotal, _capacity, &retained, &initial, &error));
        _retained = retained;
        _frozen = true;
        ReadingMemory::BindFreeAction(this, {retained, RetainedFreed});
        RetainedBound(retained);
        borrows.Add(retained, initial);
        // The canonical SVG retained owner now holds the same byte lease and
        // actual root receipt. Replace only our own known action, then retire
        // the prepaid raw record before worker publication. No new allocation
        // or second byte charge is introduced by this handoff.
        auto raw = std::exchange(_rawRetirement, nullptr);
        Check(RawFreed(raw));
        Check(notepads_markdown_svg_retained_admit(retained, &error));
    }

    void NativeEncodedImage::BorrowSvg(NativeSvgBorrowSet &borrows) const
    {
        if (!_retained || !borrows.CanAdd())
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        auto error = CorePod<notepads_markdown_core_error>();
        notepads_markdown_svg_work_token token{};
        Check(notepads_markdown_svg_resources_borrow_retained(borrows.Resources(), _retained, &token, &error));
        borrows.Add(_retained, token);
    }

    notepads_markdown_svg_retained *NativeEncodedImage::RetainedHandle() const noexcept
    {
        return _retained;
    }

    bool NativeEncodedImage::IsUnique() const noexcept
    {
        return _references.load(std::memory_order_acquire) == 1;
    }

    std::shared_ptr<NativeImagePolicy> NativeImagePolicy::Create()
    {
        auto error = CorePod<notepads_markdown_core_error>();
        notepads_markdown_image_policy *policy{};
        Check(notepads_markdown_image_policy_create(&policy, &error));
        std::unique_ptr<notepads_markdown_image_policy, decltype(&notepads_markdown_image_policy_destroy)> owner{policy, notepads_markdown_image_policy_destroy};
        auto result = std::make_shared<NativeImagePolicy>(ConstructionKey{}, policy);
        owner.release();
        return result;
    }

    NativeImagePolicy::NativeImagePolicy(ConstructionKey, notepads_markdown_image_policy *policy) noexcept
        : _policy(policy)
    {
        RegisterImageCapacityNotification();
    }

    NativeImagePolicy::~NativeImagePolicy()
    {
        UnregisterImageCapacityNotification();
        if (_policy)
            (void)notepads_markdown_image_policy_destroy(_policy);
    }

    void NativeImagePolicy::Begin(notepads_markdown_image_epoch const &epoch)
    {
        std::lock_guard guard(_mutex);
        auto descriptor = ImagePod<notepads_markdown_image_epoch_descriptor>();
        descriptor.epoch = epoch;
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_image_policy_begin(_policy, &descriptor, &error));
        _epoch = epoch;
        _hasEpoch = true;
    }

    void NativeImagePolicy::Evict(uint32_t object)
    {
        std::lock_guard guard(_mutex);
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_image_policy_evict(_policy, object, &error));
    }

    void NativeImagePolicy::ReplaceSource(uint32_t object)
    {
        std::lock_guard guard(_mutex);
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_image_policy_replace_source(_policy, object, &error));
    }

    bool NativeImagePolicy::TryForget(uint32_t object)
    {
        std::lock_guard guard(_mutex);
        auto error = CorePod<notepads_markdown_core_error>();
        auto status = notepads_markdown_image_policy_forget(_policy, object, &error);
        if (status == NOTEPADS_MARKDOWN_CORE_INVALID_STATE)
            return false;
        Check(status);
        return true;
    }

    void NativeImagePolicy::Desire(uint32_t object, notepads_markdown_image_target const &target)
    {
        std::lock_guard guard(_mutex);
        auto desired = ImagePod<notepads_markdown_image_desired>();
        desired.target = target;
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_image_policy_desire(_policy, object, &desired, &error));
    }

    std::shared_ptr<NativeImageRequest> NativeImagePolicy::Start(uint32_t object, uint32_t kind)
    {
        std::lock_guard guard(_mutex);
        auto info = ImagePod<notepads_markdown_image_request_info>();
        auto error = CorePod<notepads_markdown_core_error>();
        notepads_markdown_image_request *request{};
        Check(notepads_markdown_image_policy_start(_policy, object, kind, &request, &info, &error));
        std::unique_ptr<notepads_markdown_image_request, decltype(&notepads_markdown_image_request_destroy)> owner{request, notepads_markdown_image_request_destroy};
        auto result = std::make_shared<NativeImageRequest>(NativeImageRequest::ConstructionKey{}, shared_from_this(),
                                                           request, info.key, info.target, kind);
        owner.release();
        return result;
    }

    uint64_t NativeImagePolicy::StateGeneration() const
    {
        std::lock_guard guard(_mutex);
        uint64_t generation{};
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_image_policy_generation(_policy, &generation, &error));
        return generation;
    }

    NativeImageSnapshot NativeImagePolicy::Snapshot() const
    {
        std::lock_guard guard(_mutex);
        NativeImageSnapshot result;
        result.header = ImagePod<notepads_markdown_image_snapshot>();
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_image_policy_read(_policy, result.entries.data(), static_cast<uint32_t>(result.entries.size()),
                                      sizeof(notepads_markdown_image_entry), &result.header, &error));
        if (result.header.count > result.entries.size())
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT};
        return result;
    }

    bool NativeImagePolicy::IsCurrent(notepads_markdown_image_key const &key) const noexcept
    {
        try
        {
            std::lock_guard guard(_mutex);
            return _hasEpoch && SameEpoch(key.epoch, _epoch);
        }
        catch (...)
        {
            return false;
        }
    }

    void NativeImagePolicy::Complete(notepads_markdown_image_request const *request, notepads_markdown_image_completion const &completion,
                                     notepads_markdown_image_bytes const *decoded)
    {
        std::lock_guard guard(_mutex);
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_image_policy_complete(_policy, request, &completion, decoded, &error));
    }

    void NativeImagePolicy::CompleteReused(notepads_markdown_image_request const *request, notepads_markdown_image_completion const &completion,
                                           notepads_markdown_image_bytes const *decoded)
    {
        std::lock_guard guard(_mutex);
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_image_policy_complete_reused(_policy, request, &completion, decoded, &error));
    }

    NativeImageBytes::NativeImageBytes(ConstructionKey, std::shared_ptr<NativeImagePolicy> policy,
                                       notepads_markdown_image_bytes *bytes, uint32_t kind) noexcept
        : _policy(std::move(policy)), _bytes(bytes), _kind(kind)
    {
    }

    NativeImageBytes::~NativeImageBytes()
    {
        if (_bytes)
            (void)notepads_markdown_image_bytes_destroy(_bytes);
    }

    void NativeImageBytes::Admit(uint64_t actual)
    {
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_image_bytes_admit(_bytes, actual, &error));
        _admitted.store(actual, std::memory_order_release);
    }

    notepads_markdown_image_bytes const *NativeImageBytes::Handle() const noexcept
    {
        return _bytes;
    }

    bool NativeImageBytes::IsAdmitted(uint32_t kind, uint64_t capacity) const noexcept
    {
        return capacity && kind == _kind && _admitted.load(std::memory_order_acquire) == capacity;
    }

    NativeImageRequest::NativeImageRequest(ConstructionKey, std::shared_ptr<NativeImagePolicy> policy,
                                           notepads_markdown_image_request *request, notepads_markdown_image_key const &key,
                                           notepads_markdown_image_target target, uint32_t kind) noexcept
        : _policy(std::move(policy)), _request(request), _key(key), _target(target), _kind(kind)
    {
    }

    NativeImageRequest::~NativeImageRequest()
    {
        if (_request)
            (void)notepads_markdown_image_request_destroy(_request);
    }

    notepads_markdown_image_key const &NativeImageRequest::Key() const noexcept
    {
        return _key;
    }

    notepads_markdown_image_request const *NativeImageRequest::Handle() const noexcept
    {
        return _request;
    }

    void NativeImageRequest::Cancel() noexcept
    {
        _cancelled.store(true, std::memory_order_release);
    }

    bool NativeImageRequest::IsCancelled() const noexcept
    {
        return _cancelled.load(std::memory_order_acquire);
    }

    bool NativeImageRequest::IsCurrent() const noexcept
    {
        return !IsCancelled() && _policy->IsCurrent(_key);
    }

    void NativeImageRequest::NoteMetadata(notepads_markdown_image_metadata const &metadata)
    {
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_image_request_note_metadata(_request, &metadata, &error));
    }

    uint32_t NativeImageRequest::Kind() const noexcept
    {
        return _kind;
    }

    notepads_markdown_image_target const &NativeImageRequest::Target() const noexcept
    {
        return _target;
    }

    bool NativeImageRequest::BelongsTo(NativeImagePolicy const *policy) const noexcept
    {
        return _policy.get() == policy;
    }

    std::shared_ptr<NativeImageBytes> NativeImageRequest::Reserve(uint32_t kind, uint64_t bytes)
    {
        if (IsCancelled())
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_CANCELLED};
        auto error = CorePod<notepads_markdown_core_error>();
        notepads_markdown_image_bytes *lease{};
        auto status = notepads_markdown_image_request_reserve(_request, kind, bytes, &lease, &error);
        auto encoded = kind == NOTEPADS_MARKDOWN_IMAGE_ENCODED;
        auto validBytes = bytes && ((encoded && bytes <= ImageMaximumEncodedBytes) ||
            (kind == NOTEPADS_MARKDOWN_IMAGE_DECODED && bytes <= ImageMaximumPixelCount * 4));
        auto processMaximum = encoded ? ImageProcessMaximumEncodedBytes : ImageProcessMaximumDecodedBytes;
        const uint32_t capacityKind = encoded ? NOTEPADS_MARKDOWN_CORE_AUXILIARY_CAPACITY : NOTEPADS_MARKDOWN_CORE_DECODED_BYTES;
        if (status == NOTEPADS_MARKDOWN_CORE_RESOURCE_LIMIT &&
            validBytes &&
            error.limit_kind == capacityKind &&
            error.requested > error.limit && error.limit == processMaximum && IsCurrent())
        {
            // Reserve has returned and released both policy/process locks.
            // The error carries the actual authority's limit; a current process
            // snapshot distinguishes global pressure from a local-only denial.
            auto usage = ImagePod<notepads_markdown_image_process_capacity>();
            auto ignored = CorePod<notepads_markdown_core_error>();
            if (notepads_markdown_image_get_process_capacity(&usage, &ignored) == NOTEPADS_MARKDOWN_CORE_OK)
            {
                auto used = encoded ? usage.encoded_bytes : usage.decoded_bytes;
                auto notice = encoded ? uint32_t{1} : uint32_t{2};
                if (used > processMaximum - bytes &&
                    !(_reclaimSentKinds.fetch_or(notice, std::memory_order_acq_rel) & notice))
                    RequestOptionalImageReclaim();
            }
        }
        Check(status);
        std::unique_ptr<notepads_markdown_image_bytes, decltype(&notepads_markdown_image_bytes_destroy)> owner{lease, notepads_markdown_image_bytes_destroy};
        auto result = std::make_shared<NativeImageBytes>(NativeImageBytes::ConstructionKey{}, _policy, lease, kind);
        owner.release();
        return result;
    }

    void NativeImageRequest::CompleteReady(notepads_markdown_image_metadata const &metadata,
                                           std::shared_ptr<NativeImageBytes> const &decoded)
    {
        if (!decoded)
            throw winrt::hresult_invalid_argument();
        if (IsCancelled())
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_CANCELLED};
        auto completion = ImagePod<notepads_markdown_image_completion>();
        completion.ready = 1;
        completion.metadata = metadata;
        completion.target = _target;
        _policy->Complete(_request, completion, decoded->Handle());
    }

    void NativeImageRequest::CompleteReused(notepads_markdown_image_metadata const &metadata,
                                            std::shared_ptr<NativeImageBytes> const &decoded)
    {
        if (!decoded)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        if (!IsCurrent())
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_CANCELLED};
        auto completion = ImagePod<notepads_markdown_image_completion>();
        completion.ready = 1;
        completion.metadata = metadata;
        completion.target = _target;
        _policy->CompleteReused(_request, completion, decoded->Handle());
    }

    void NativeImageRequest::Fail(uint32_t kind, HRESULT error, uint32_t detail)
    {
        auto completion = ImagePod<notepads_markdown_image_completion>();
        completion.target = _target;
        completion.failure = kind;
        completion.error = error;
        completion.detail = detail;
        _policy->Complete(_request, completion, nullptr);
    }

    struct ImagePixels::RasterRetirement
    {
        std::shared_ptr<NativeImageBytes> charge;
        ModuleLifetime module;
    };

    ImagePixels::ImagePixels(std::shared_ptr<ReadingMemory> memory, std::shared_ptr<NativeImageBytes> lease)
        : charge(std::move(lease))
    {
        // The independent prefix and this subquota owner survive a failed
        // pixel HeapFree or root-receipt retirement. No model/IO/task is held.
        auto block = memory->Allocate(sizeof(RasterRetirement));
        _rasterRetirement = ::new (block) RasterRetirement{charge};
    }

    ImagePixels::ImagePixels(std::shared_ptr<ReadingMemory> memory, std::shared_ptr<NativeImageBytes> lease,
                             NativeSvgBorrowSet *borrows)
        : _svg{borrows, lease ? lease->Handle() : nullptr, nullptr}, charge(std::move(lease))
    {
        if (!memory || !borrows)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
    }

    ImagePixels::~ImagePixels()
    {
        if (_rasterRetirement)
            (void)RasterFreed(_rasterRetirement);
    }

    void ImagePixels::AllocatePixels(ReadingMemory const &memory, size_t bytes)
    {
        if (_frozen || !pixels.empty())
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        pixels.Allocate(memory, bytes, _svg.borrows ? ReadingAllocationFactory{&_svg, AllocateSvgPayload} : ReadingAllocationFactory{});
    }

    std::span<uint8_t> ImagePixels::MutablePixels()
    {
        if (_frozen || pixels.empty())
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        return {pixels._data, pixels._size};
    }

    void ImagePixels::SetAnimation(std::span<uint32_t const> frameEnds, uint32_t playthroughs)
    {
        const auto frameBytes = uint64_t{metadata.stride} * metadata.decoded_height;
        if (_frozen || IsAnimated() || metadata.kind != NOTEPADS_MARKDOWN_IMAGE_RASTER || !frameBytes ||
            frameEnds.size() < 2 || frameEnds.size() > MaximumAnimatedGifFrames ||
            uint64_t{frameEnds.size()} * (frameBytes + sizeof(uint32_t)) != pixels.size())
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        uint32_t previous{};
        for (auto end : frameEnds)
        {
            if (end <= previous)
                throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
            previous = end;
        }
        auto destination = MutablePixels().last(frameEnds.size_bytes());
        std::memcpy(destination.data(), frameEnds.data(), destination.size());
        _frameCount = static_cast<uint32_t>(frameEnds.size());
        _playthroughs = playthroughs;
        _durationMilliseconds = previous;
    }

    std::span<uint8_t const> ImagePixels::FramePixels(uint32_t index) const noexcept
    {
        if (index >= _frameCount)
            return {};
        const auto bytes = size_t{metadata.stride} * metadata.decoded_height;
        if (!bytes || index >= pixels.size() / bytes)
            return {};
        return {pixels.data() + index * bytes, bytes};
    }

    ImageFramePosition ImagePixels::FrameAt(uint64_t elapsedMilliseconds) const noexcept
    {
        if (!IsAnimated())
            return {};
        if (_playthroughs && elapsedMilliseconds >= uint64_t{_playthroughs} * _durationMilliseconds)
            return {_frameCount - 1, 0};
        auto within = static_cast<uint32_t>(elapsedMilliseconds % _durationMilliseconds);
        auto ends = pixels.end() - _frameCount * sizeof(uint32_t);
        for (uint32_t index = 0; index < _frameCount; ++index)
        {
            uint32_t end{};
            std::memcpy(&end, ends + index * sizeof(uint32_t), sizeof(end));
            if (within < end)
                return {index, end - within};
        }
        return {};
    }

    void ImagePixels::Freeze()
    {
        if (pixels.empty() || _svg.borrows || _rasterRetirement || !ReadingMemory::InspectAllocation(pixels._data).hasAction)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        _frozen = true;
    }

    bool ImagePixels::IsFrozen() const noexcept
    {
        return _frozen;
    }

    int32_t ImagePixels::RasterFreed(void *context) noexcept
    {
        auto retirement = static_cast<RasterRetirement *>(context);
        auto charge = std::move(retirement->charge);
        ModuleLifetime tail;
        retirement->~RasterRetirement();
        return ReadingMemory::TryFree(retirement) ? NOTEPADS_MARKDOWN_CORE_OK : NOTEPADS_MARKDOWN_CORE_INTERNAL_INVARIANT;
    }

    void ImagePixels::BindRasterAllocation()
    {
        if (!_rasterRetirement || pixels.empty() || ReadingMemory::InspectAllocation(pixels._data).hasAction)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        ReadingMemory::BindFreeAction(pixels._data, {_rasterRetirement, RasterFreed});
        _rasterRetirement = nullptr;
    }

    void *ImagePixels::AllocateSvgPayload(void *context, ReadingMemory const &memory, size_t bytes)
    {
        auto &state = *static_cast<SvgPayloadContext *>(context);
        // This context lives in the immutable image object. Finish clears its
        // transient account pointer, so a later accidental resize is rejected
        // without accessing the retired worker or a former stack context.
        if (!state.borrows)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        SvgAllocationContext allocation{state.borrows, state.image, nullptr, NOTEPADS_MARKDOWN_SVG_RETAINED_PAYLOAD};
        auto result = AllocateSvgBlock(allocation, memory, bytes);
        state.retained = allocation.retained;
        return result;
    }

    void ImagePixels::FinishSvgAllocation()
    {
        if (!_svg.borrows || !_svg.retained)
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_STATE};
        auto error = CorePod<notepads_markdown_core_error>();
        Check(notepads_markdown_svg_retained_admit(_svg.retained, &error));
        _svg.borrows = nullptr;
        _svg.image = nullptr;
    }

    notepads_markdown_svg_retained *ImagePixels::RetainedHandle() const noexcept
    {
        return _svg.retained;
    }

    void CreateSvgImagePixels(std::shared_ptr<NativeImageRequest> const &request,
                              std::shared_ptr<ReadingMemory> const &memory, notepads_markdown_svg_resources *resources,
                              uint64_t capacity, NativeSvgImageAllocation &result)
    {
        if (!request || !memory || !resources || capacity == 0 || capacity > UINT32_MAX || capacity > SIZE_MAX ||
            result.pixels || result.borrows.Resources())
            throw NativeImageFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT};
        auto charge = request->Reserve(NOTEPADS_MARKDOWN_IMAGE_DECODED, capacity);
        result.borrows = NativeSvgBorrowSet{resources};
        // The metadata allocator sees the actual implementation's n*sizeof(T),
        // never an assumed std::shared_ptr control-block size. Its context is
        // borrowed only during this one construction; deallocate ignores it.
        SvgAllocationContext metadata{&result.borrows, nullptr, nullptr, NOTEPADS_MARKDOWN_SVG_RETAINED_METADATA};
        try
        {
            {
                ImagePayloadConstruction construction{*memory, {&metadata, AllocateSvgMetadata}};
                result.pixels = std::allocate_shared<ImagePixels>(ImagePayloadAllocator<ImagePixels>{construction.Nonce()}, memory, charge, &result.borrows);
            }
            result.metadataRetained = metadata.retained;
            result.pixels->AllocatePixels(*memory, static_cast<size_t>(capacity));
            charge->Admit(result.pixels->pixels.capacity());
            result.pixels->FinishSvgAllocation();
        }
        catch (...)
        {
            result.metadataRetained = metadata.retained;
            throw;
        }
    }

    bool NativeImageDecodeResult::Ready() const noexcept
    {
        return pixels && failure == NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE && SUCCEEDED(error);
    }

    winrt::Windows::Foundation::IAsyncAction DecodeImageAsync(
        std::shared_ptr<NativeImageRequest> request, std::shared_ptr<ReadingMemory> memory,
        winrt::Windows::Storage::Streams::IRandomAccessStream stream, uint32_t maximumDecodeSide,
        std::shared_ptr<NativeImageDecodeResult> result, std::shared_ptr<ImageLoadTask> svgTask,
        NativeSvgImageTarget svgTarget, std::shared_ptr<NativeImageReuseCache> reuseCache, uint64_t reuseScope)
    {
        ModuleLifetime module;
        StreamOwner owner{std::move(stream)};
        if (!result)
            throw winrt::hresult_invalid_argument();
        *result = NativeImageDecodeResult{};
        if (svgTask)
        {
            std::lock_guard lock(svgTask->mutex);
            svgTask->decodeStarted = true;
        }
        std::shared_ptr<ImagePixels> decoded;
        std::shared_ptr<ImageReadState> reads;
        uint32_t stage = NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR;
        try
        {
            auto cancellation = co_await winrt::get_cancellation_token();
            cancellation.enable_propagation();
            Require(request && memory, NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR, E_INVALIDARG);
            if (!owner.stream)
                throw DecodeFailure{NOTEPADS_MARKDOWN_IMAGE_NOT_PROVIDED, S_OK};
            // Reject before the first switch, so even a non-agile stream is
            // closed on its acquisition apartment by the common close path.
            Require(static_cast<bool>(owner.stream.try_as<IAgileObject>()), NOTEPADS_MARKDOWN_IMAGE_UNSUPPORTED, E_NOINTERFACE);
            Require(maximumDecodeSide > 0 && maximumDecodeSide <= ImageMaximumDecodeSide, NOTEPADS_MARKDOWN_IMAGE_QUOTA, E_INVALIDARG);
            reads = ImageReadState::Create(request, memory, owner.stream);
            cancellation.callback(
                [weak = std::weak_ptr<ImageReadState>{reads}, weakSvg = std::weak_ptr<ImageLoadTask>{svgTask}]
                {
                    if (auto active = weak.lock())
                        active->Cancel();
                    if (auto active = weakSvg.lock())
                        CancelSvgImageTask(*active);
                });
            if (cancellation())
                throw winrt::hresult_canceled();
            CheckActive(request);
            co_await winrt::resume_background();
            CheckActive(request);
            Require(owner.stream.CanRead() && owner.stream.Position() == 0, NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR, E_INVALIDARG);
            auto expected = owner.stream.Size();
            Require(expected <= ImageMaximumEncodedBytes, NOTEPADS_MARKDOWN_IMAGE_QUOTA, HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA));
            {
                // Retained SVG sources keep this allocation after decoding.
                // Fund the advertised capacity, while still checking every
                // read and EOF against Size() before appending any bytes.
                auto capacity = (std::max)(uint64_t{1}, expected);
                auto charge = request->Reserve(NOTEPADS_MARKDOWN_IMAGE_ENCODED, capacity);
                auto encoded = NativeEncodedImage::Create(memory, charge, static_cast<size_t>(capacity));
                for (;;)
                {
                    CheckActive(request);
                    if (cancellation())
                        throw winrt::hresult_canceled();
                    auto remaining = ImageMaximumEncodedBytes - encoded->Bytes().size();
                    auto count = static_cast<uint32_t>(std::min<uint64_t>(ImageReadState::ChunkBytes, remaining + 1));
                    auto chunk = reads->Read(count);
                    CheckActive(request);
                    auto length = chunk.Length();
                    Require(length <= count, NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR, E_UNEXPECTED);
                    if (!length)
                        break;
                    Require(length <= remaining, NOTEPADS_MARKDOWN_IMAGE_QUOTA, HRESULT_FROM_WIN32(ERROR_NOT_ENOUGH_QUOTA));
                    Require(encoded->Bytes().size() <= expected && length <= expected - encoded->Bytes().size(),
                            NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR, HRESULT_FROM_WIN32(ERROR_INVALID_DATA));
                    auto access = chunk.as<::Windows::Storage::Streams::IBufferByteAccess>();
                    BYTE *bytes{};
                    winrt::check_hresult(access->Buffer(&bytes));
                    Require(bytes != nullptr, NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR, E_POINTER);
                    encoded->Append({bytes, length});
                }
                Require(encoded->Bytes().size() == expected, NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR,
                        HRESULT_FROM_WIN32(ERROR_HANDLE_EOF));
                encoded->Freeze();
                stage = NOTEPADS_MARKDOWN_IMAGE_DECODE_ERROR;
                // A custom agile stream may complete on any apartment. Ensure
                // synchronous WIC work cannot inherit that completion thread.
                co_await winrt::resume_background();
                auto decoderClass = DecoderClass(encoded->Bytes());
                NativeImageReuseCache::Candidate cached;
                if (reuseCache && reuseScope &&
                    request->Kind() != NOTEPADS_MARKDOWN_IMAGE_SVG && !::IsEqualGUID(decoderClass, GUID_NULL))
                {
                    auto checkpoint = [&] { CheckActive(request); };
                    result->reuseScope = reuseScope;
                    cached = reuseCache->FindRaster(reuseScope, encoded->Bytes(), *request, checkpoint);
                }
                if (cached)
                {
                    CheckActive(request);
                    request->NoteMetadata(cached.pixels->metadata);
                    decoded = std::move(cached.pixels);
                    result->encoded = std::move(cached.encoded);
                    result->reused = true;
                }
                else if (::IsEqualGUID(decoderClass, GUID_NULL) && svgTask && svgTarget.enabled &&
                    IsXmlImageCandidate(encoded->Bytes(), [&] { CheckActive(request); }))
                {
                    // This scope starts after resume_background. The private
                    // SVG call completes synchronously on this MTA caller,
                    // including worker join/reaping, before returning.
                    DecodeApartment apartment;
                    if (cancellation())
                        CancelSvgImageTask(*svgTask);
                    RenderSvgImageOnBackground(svgTask, memory, std::move(encoded), svgTarget, result);
                    if (result->Ready())
                        result->pixels->Freeze();
                }
                else
                {
                    decoded = Decode(request, memory, encoded->Bytes(), maximumDecodeSide);
                    if (reuseCache && result->reuseScope && decoded)
                        result->encoded = std::move(encoded);
                }
                // A retained raster cache identity or SVG task/result can keep
                // this same original block. A hit's duplicate fresh read ends
                // here and returns only its own actual encoded allocation.
            }
        }
        catch (DecodeFailure const &failure)
        {
            RecordDecodeFailure(*result, failure.kind, failure.code().value);
        }
        catch (NativeFailure const &failure)
        {
            RecordNativeFailure(*result, failure.status);
        }
        catch (winrt::hresult_canceled const &failure)
        {
            RecordDecodeFailure(*result, NOTEPADS_MARKDOWN_IMAGE_CANCELLED, failure.code().value);
        }
        catch (winrt::hresult_error const &failure)
        {
            RecordDecodeFailure(*result, stage, failure.code().value);
        }
        catch (std::bad_alloc const &)
        {
            RecordDecodeFailure(*result, NOTEPADS_MARKDOWN_IMAGE_QUOTA, E_OUTOFMEMORY);
        }
        catch (...)
        {
            RecordDecodeFailure(*result, stage, winrt::to_hresult().value);
        }
        if (reads && reads->Quarantined())
        {
            // Freeze the unique Close authority together with the original IO
            // owners. The common guard must not close an unobserved live read.
            owner.stream = nullptr;
            if (SUCCEEDED(result->closeError))
                result->closeError = reads->RetirementError();
            if (svgTask)
            {
                std::lock_guard lock(svgTask->mutex);
                svgTask->svgQuarantined = true;
                svgTask->svgKeepAlive = svgTask;
            }
        }
        auto close = owner.Close();
        if (SUCCEEDED(result->closeError))
            result->closeError = close;
        if (FAILED(close) && result->failure == NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE)
        {
            result->failure = NOTEPADS_MARKDOWN_IMAGE_PROVIDER_ERROR;
            result->error = close;
        }
        if (result->failure == NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE && request && !request->IsCurrent())
        {
            result->failure = NOTEPADS_MARKDOWN_IMAGE_CANCELLED;
            result->error = HRESULT_FROM_WIN32(ERROR_CANCELLED);
        }
        if (result->failure == NOTEPADS_MARKDOWN_IMAGE_NO_FAILURE && decoded)
            result->pixels = std::move(decoded);
        // Otherwise decoded dies before completion, giving back actual pixels
        // before its lease. Cancellation never publishes a half-result.
        decoded.reset();
        // The actual read/decode/close work is finished. A completed WinRT
        // coroutine can outlive its completion callback; do not let that frame
        // keep a physical IO permit after the caller drops its request owner.
        request.reset();
        reads.reset();
        memory.reset();
        reuseCache.reset();
        result.reset();
        owner.stream = nullptr;
        stream = nullptr;
        if (svgTask)
            svgTask->NotifyDecodeComplete();
        svgTask.reset();
    }
}
