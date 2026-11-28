// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include "Resources/Svg/NativeSvgFontSession.h"
#include "Resources/Svg/NativeSvgFontMemory.h"
#include <d2d1.h>
#include <dwrite_3.h>
#include <bcrypt.h>
#include <wrl.h>
#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstring>
#include <limits>
#include <new>
#include <utility>
#include <vector>

namespace Notepads::Markdown::Native
{
    using SvgFontMemory::Closed;
    using SvgFontMemory::Quota;
    using SvgFontMemory::Timeout;
    using SvgFontMemory::Geometry;
    using SvgFontMemory::Scratch;
    using SvgFontMemory::Check;
    using SvgFontMemory::Bytes;
    using SvgFontMemory::FontLimit;
    using SvgFontMemory::RecordFault;
    using SvgFontMemory::Free;
    using Microsoft::WRL::ComPtr;
    static bool InRange(uint32_t start, uint32_t length, uint32_t size) noexcept
    {
        return start <= size && length <= size - start;
    }
    static bool Boundary(uint8_t const *data, uint32_t at, uint32_t size) noexcept
    {
        return at <= size && (at == size || (data[at] & 0xC0) != 0x80);
    }
    template <class T> static void ValidatePointer(T const *pointer, uint32_t count)
    {
        uint64_t bytes = Bytes(count, sizeof(T));
        auto address = reinterpret_cast<uintptr_t>(pointer);
        if (count && (!pointer || address % alignof(T) || bytes > UINTPTR_MAX - address))
            throw E_INVALIDARG;
    }
    static bool ValidUtf8(uint8_t const *data, uint32_t size, SvgFontServices const *services = nullptr)
    {
        if (size && !data)
            return false;
        uint32_t checked{};
        for (uint32_t p = 0; p < size;)
        {
            if (services && p - checked >= 65532)
            {
                Check(services->checkpoint(services->context, p - checked));
                checked = p;
            }
            uint32_t c = data[p++], n{}, minimum{};
            if (!c)
                return false;
            if (c < 0x80)
                continue;
            if (c >= 0xC2 && c <= 0xDF)
            {
                n = 1;
                minimum = 0x80;
                c &= 0x1F;
            }
            else if (c >= 0xE0 && c <= 0xEF)
            {
                n = 2;
                minimum = 0x800;
                c &= 0x0F;
            }
            else if (c >= 0xF0 && c <= 0xF4)
            {
                n = 3;
                minimum = 0x10000;
                c &= 7;
            }
            else
                return false;
            if (n > size - p)
                return false;
            while (n--)
            {
                uint32_t t = data[p++];
                if ((t & 0xC0) != 0x80)
                    return false;
                c = (c << 6) | (t & 63);
            }
            if (c < minimum || c > 0x10FFFF || (c >= 0xD800 && c <= 0xDFFF))
                return false;
        }
        if (services)
            Check(services->checkpoint(services->context, size - checked));
        return true;
    }
    static void ValidateString(MsFrDraft const &draft, uint32_t p, uint32_t n, bool nonempty)
    {
        if ((nonempty && !n) || !InRange(p, n, draft.strings_length) ||
            !Boundary(draft.strings, p, draft.strings_length) || !Boundary(draft.strings, p + n, draft.strings_length))
            throw E_INVALIDARG;
    }
    static SvgFontVector<wchar_t> Wide(SvgFontAllocationContext *context, uint8_t const *data, uint32_t size)
    {
        SvgFontVector<wchar_t> result{SvgFontAllocator<wchar_t>(context)};
        int count = size ? MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, reinterpret_cast<char const *>(data),
                                               static_cast<int>(size), nullptr, 0)
                         : 0;
        if (size && !count)
            throw E_INVALIDARG;
        result.resize(static_cast<size_t>(count) + 1);
        if (count && !MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, reinterpret_cast<char const *>(data),
                                          static_cast<int>(size), result.data(), count))
            throw E_INVALIDARG;
        return result;
    }
    // This is the private canonical family field, not a second CSS parser.
    // Quoting already identifies Named versus Generic after the Rust cascade.
    struct SvgFontFamilies
    {
        SvgFontVector<wchar_t> pool;
        SvgFontVector<uint32_t> offsets;
        SvgFontVector<wchar_t const *> names;
        explicit SvgFontFamilies(SvgFontAllocationContext *context)
            : pool(SvgFontAllocator<wchar_t>(context)), offsets(SvgFontAllocator<uint32_t>(context)),
              names(SvgFontAllocator<wchar_t const *>(context))
        {
        }
    };
    struct SvgFontFamilyReader
    {
        SvgFontAllocationContext *context;
        uint8_t const *data;
        uint32_t size, position{}, paid{};
        uint8_t Peek()
        {
            if (position >= size)
                throw E_INVALIDARG;
            if (position >= paid)
            {
                auto count = (std::min)(4096u, size - position);
                Check(context->services.checkpoint(context->services.context, count));
                paid = position + count;
            }
            return data[position];
        }
        uint8_t Take()
        {
            auto value = Peek();
            ++position;
            return value;
        }
    };
    static wchar_t const *GenericFamily(uint8_t const *data, uint32_t size)
    {
        struct Mapping
        {
            char const *token;
            uint32_t length;
            wchar_t const *family;
        };
        static Mapping const mappings[] = {{"serif", 5, L"Times New Roman"},
                                           {"sans-serif", 10, L"Arial"},
                                           {"monospace", 9, L"Courier New"},
                                           {"cursive", 7, L"Comic Sans MS"},
                                           {"fantasy", 7, L"Impact"}};
        for (auto const &mapping : mappings)
            if (size == mapping.length && !std::memcmp(data, mapping.token, size))
                return mapping.family;
        return nullptr;
    }
    static SvgFontFamilies DecodeFamilies(SvgFontAllocationContext *context, uint8_t const *data, uint32_t size)
    {
        SvgFontFamilies result(context);
        if (!size)
            throw E_INVALIDARG;
        SvgFontFamilyReader reader{context, data, size};
        while (reader.position < size)
        {
            Check(context->services.checkpoint(context->services.context, 1));
            if (result.pool.size() > UINT32_MAX)
                throw E_INVALIDARG;
            auto offset = static_cast<uint32_t>(result.pool.size());
            auto start = reader.position;
            if (reader.Peek() == '"')
            {
                reader.Take();
                uint32_t decodedBytes{};
                bool closed{};
                while (reader.position < size)
                {
                    uint32_t scalar = reader.Take();
                    if (scalar == '"')
                    {
                        closed = true;
                        break;
                    }

                    if (scalar == '\\')
                    {
                        scalar = reader.Take();
                        if (scalar != '"' && scalar != '\\')
                        {
                            uint32_t value{};
                            for (uint32_t digit = 0; digit < 6; ++digit)
                            {
                                auto hex = digit ? reader.Take() : scalar;
                                if (hex >= '0' && hex <= '9')
                                    value = (value << 4) | (hex - '0');
                                else if (hex >= 'A' && hex <= 'F')
                                    value = (value << 4) | (hex - 'A' + 10);
                                else
                                    throw E_INVALIDARG;
                            }
                            if (reader.Take() != ' ' || !value || (value >= 32 && value != 127))
                                throw E_INVALIDARG;
                            scalar = value;
                        }
                    }
                    else if (scalar < 32 || scalar == 127)
                        throw E_INVALIDARG;
                    else if (scalar >= 128)
                    {
                        uint32_t continuation{}, minimum{};
                        if (scalar >= 0xC2 && scalar <= 0xDF)
                        {
                            continuation = 1;
                            minimum = 0x80;
                            scalar &= 31;
                        }
                        else if (scalar >= 0xE0 && scalar <= 0xEF)
                        {
                            continuation = 2;
                            minimum = 0x800;
                            scalar &= 15;
                        }
                        else if (scalar >= 0xF0 && scalar <= 0xF4)
                        {
                            continuation = 3;
                            minimum = 0x10000;
                            scalar &= 7;
                        }
                        else
                            throw E_INVALIDARG;
                        while (continuation--)
                        {
                            auto next = reader.Take();
                            if ((next & 0xC0) != 0x80)
                                throw E_INVALIDARG;
                            scalar = (scalar << 6) | (next & 63);
                        }
                        if (scalar < minimum || scalar > 0x10FFFF || (scalar >= 0xD800 && scalar <= 0xDFFF))
                            throw E_INVALIDARG;
                    }
                    decodedBytes += scalar < 128 ? 1 : scalar < 0x800 ? 2 : scalar < 0x10000 ? 3 : 4;
                    if (!scalar || decodedBytes > 4096)
                        throw E_INVALIDARG;
                    if (scalar < 0x10000)
                        result.pool.push_back(static_cast<wchar_t>(scalar));
                    else
                    {
                        scalar -= 0x10000;
                        result.pool.push_back(static_cast<wchar_t>(0xD800 + (scalar >> 10)));
                        result.pool.push_back(static_cast<wchar_t>(0xDC00 + (scalar & 1023)));
                    }
                }
                if (!closed || !decodedBytes)
                    throw E_INVALIDARG;
            }
            else
            {

                while (reader.position < size && reader.Peek() != ',')
                    reader.Take();
                auto length = reader.position - start;
                auto generic = GenericFamily(data + start, length);
                if (!generic)
                    throw E_INVALIDARG;
                for (auto p = generic; *p; ++p)
                    result.pool.push_back(*p);
            }
            result.pool.push_back(0);
            result.offsets.push_back(offset);
            if (reader.position == size)
                break;
            if (reader.Take() != ',' || reader.Take() != ' ' || reader.position == size)
                throw E_INVALIDARG;
        }
        result.names.reserve(result.offsets.size());
        for (size_t i = 0; i < result.offsets.size(); ++i)
        {
            if ((i & 255) == 0)
                Check(context->services.checkpoint(context->services.context,
                                                   (std::min)(size_t{256}, result.offsets.size() - i)));
            result.names.push_back(result.pool.data() + result.offsets[i]);
        }
        Check(context->services.checkpoint(context->services.context, 0));
        return result;
    }
    static uint32_t SwapTag(uint32_t t) noexcept
    {
        return ((t & 255) << 24) | ((t & 0xFF00) << 8) | ((t >> 8) & 0xFF00) | (t >> 24);
    }

    class SvgFontAnalysisSource final : public IDWriteTextAnalysisSource
    {
        ULONG refs_{1};

      public:
        wchar_t const *text{};
        uint32_t length{}, direction{};
        wchar_t const *language{};
        HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **out) override
        {
            if (!out)
                return E_POINTER;
            *out = nullptr;
            if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWriteTextAnalysisSource))
                return E_NOINTERFACE;
            *out = static_cast<IDWriteTextAnalysisSource *>(this);
            AddRef();
            return S_OK;
        }
        ULONG STDMETHODCALLTYPE AddRef() override
        {
            return ++refs_;
        }
        ULONG STDMETHODCALLTYPE Release() override
        {
            return --refs_;
        }
        HRESULT STDMETHODCALLTYPE GetTextAtPosition(UINT32 p, WCHAR const **out, UINT32 *n) override
        {
            if (!out || !n || p > length)
                return E_INVALIDARG;
            *n = length - p;
            *out = *n ? text + p : nullptr;
            return S_OK;
        }
        HRESULT STDMETHODCALLTYPE GetTextBeforePosition(UINT32 p, WCHAR const **out, UINT32 *n) override
        {
            if (!out || !n || p > length)
                return E_INVALIDARG;
            *n = p;
            *out = p ? text : nullptr;
            return S_OK;
        }
        DWRITE_READING_DIRECTION STDMETHODCALLTYPE GetParagraphReadingDirection() override
        {
            return direction ? DWRITE_READING_DIRECTION_RIGHT_TO_LEFT : DWRITE_READING_DIRECTION_LEFT_TO_RIGHT;
        }
        HRESULT STDMETHODCALLTYPE GetLocaleName(UINT32 p, UINT32 *n, WCHAR const **out) override
        {
            if (!out || !n || p > length)
                return E_INVALIDARG;
            *n = length - p;
            *out = language;
            return S_OK;
        }
        HRESULT STDMETHODCALLTYPE GetNumberSubstitution(UINT32 p, UINT32 *n, IDWriteNumberSubstitution **out) override
        {
            if (!out || !n || p > length)
                return E_INVALIDARG;
            *n = length - p;
            *out = nullptr;
            return S_OK;
        }
    };

    struct SvgFontFileOwner
    {
        MsFrFile metadata{};
        ComPtr<IDWriteFontFile> file;
        ComPtr<IDWriteFontFileStream> stream;
        bool filled{};
    };
    struct SvgFontFaceOwner
    {
        MsFrFace metadata{};
        ComPtr<IDWriteFontFace5> face;
        SvgFontVector<MsFrAxis> axes;
        bool metrics_filled[2]{};
        explicit SvgFontFaceOwner(SvgFontAllocationContext *c) : axes(SvgFontAllocator<MsFrAxis>(c))
        {
        }
    };
    struct SvgFontDraftOwner
    {
        uint64_t id{};
        uint32_t passes{};
        uint8_t text_sha[32]{};
        SvgFontVector<uint8_t> element;
        explicit SvgFontDraftOwner(SvgFontAllocationContext *c) : element(SvgFontAllocator<uint8_t>(c))
        {
        }
    };
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
    struct SvgFontQualityForcedOwner
    {
        SvgFontVector<uint8_t> element;
        SvgFontVector<wchar_t> family;
        std::array<uint8_t, 32> sha{};
        uint32_t index{}, simulations{};
        uint64_t draft{};
        bool matched{}, bound{};

        explicit SvgFontQualityForcedOwner(SvgFontAllocationContext *context)
            : element(SvgFontAllocator<uint8_t>(context)), family(SvgFontAllocator<wchar_t>(context))
        {
        }
    };
#endif
    struct NativeSvgFontSession::Impl
    {
        SvgFontAllocationContext memory;
        SvgCoverageSession coverage;
        notepads_markdown_svg_work_token object_lease{};
        uint64_t ticket{}, generation{}, geometry_generation{};
        uint32_t creator{GetCurrentThreadId()}, total_utf16{};
        bool closed{}, registered{}, geometry_pending{}, geometry_filled{};
        Impl *previous{};
        NativeSvgFontStatistics stats;
        ComPtr<IDWriteFactory6> factory;
        ComPtr<IDWriteFontCollection> collection;
        ComPtr<IDWriteFontFallback> fallback;
        SvgFontVector<SvgFontFileOwner> file_owners;
        SvgFontVector<SvgFontFaceOwner> face_owners;
        SvgFontVector<SvgFontDraftOwner> drafts;
        SvgFontVector<MsFrFile> files;
        SvgFontVector<MsFrFace> faces;
        SvgFontVector<MsFrAxis> axes;
        SvgFontVector<MsFrLogicalRun> runs;
        SvgFontVector<uint8_t> strings;
        SvgFontVector<MsFrGlyphPath> paths;
        SvgFontVector<MsFrCommand> commands;
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
        SvgFontVector<SvgFontQualityForcedOwner> quality_forced;
        SvgFontVector<wchar_t> quality_fallback_family;
        NativeSvgFontQualityTrace *quality_trace{};
#endif
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
        NativeSvgFontRuntimeHook runtime_hook{};
        std::array<uint64_t, static_cast<size_t>(NativeSvgFontRuntimePoint::Count)> runtime_ordinals{};

        bool RuntimeBoundary(NativeSvgFontRuntimePoint point, uint64_t progress, uint64_t capacity)
        {
            auto ordinal = ++runtime_ordinals[static_cast<size_t>(point)];
            bool hit = runtime_hook.point == point && runtime_hook.ordinal == ordinal;
            bool failing = point == NativeSvgFontRuntimePoint::FragmentRead ||
                           point == NativeSvgFontRuntimePoint::SinkCommand ||
                           point == NativeSvgFontRuntimePoint::VisibleOutline;
            HRESULT hr = hit && failing ? runtime_hook.failureHr : S_OK;
            // Fix the shared request's first fault before an observer, sticky
            // sink state, local owner destruction, or refund can run.
            if (hr != S_OK)
                Fault(hr);
            if (runtime_hook.observer)
                runtime_hook.observer(runtime_hook.context, {point, ordinal, progress, capacity, hr, hit});
            if (hr != S_OK)
                throw hr;
            return hit;
        }
#endif

        Impl(SvgFontServices services, notepads_markdown_svg_work_token lease)
            : memory{services}, coverage(&memory), object_lease(lease),
              file_owners(SvgFontAllocator<SvgFontFileOwner>(&memory)),
              face_owners(SvgFontAllocator<SvgFontFaceOwner>(&memory)),
              drafts(SvgFontAllocator<SvgFontDraftOwner>(&memory)), files(SvgFontAllocator<MsFrFile>(&memory)),
              faces(SvgFontAllocator<MsFrFace>(&memory)), axes(SvgFontAllocator<MsFrAxis>(&memory)),
              runs(SvgFontAllocator<MsFrLogicalRun>(&memory)), strings(SvgFontAllocator<uint8_t>(&memory)),
              paths(SvgFontAllocator<MsFrGlyphPath>(&memory, Geometry)),
              commands(SvgFontAllocator<MsFrCommand>(&memory, Geometry))
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
              ,
              quality_forced(SvgFontAllocator<SvgFontQualityForcedOwner>(&memory)),
              quality_fallback_family(SvgFontAllocator<wchar_t>(&memory))
#endif
        {
        }
        void Work(uint64_t n = 0)
        {
            if (closed)
                throw Closed;
            if (stats.firstFault != S_OK)
                throw stats.firstFault;
            HRESULT hr = memory.services.checkpoint(memory.services.context, n);
            if (hr != S_OK)
            {
                if (hr == E_ABORT || hr == Timeout)
                    closed = true;
                throw hr;
            }
        }
        HRESULT Fault(HRESULT hr) noexcept
        {
            RecordFault(memory.services.context, hr);
            if (stats.firstFault == S_OK)
                stats.firstFault = hr;
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
            if (quality_trace && quality_trace->firstError == S_OK)
                quality_trace->firstError = hr;
#endif
            return hr;
        }
        void InvalidateMetadata()
        {
            Free(files);
            Free(faces);
            Free(axes);
            Free(runs);
            Free(strings);
        }
        void Initialize();
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
        void InitializeQuality(NativeSvgFontQualityConfig const &, std::span<uint8_t const>);
        void RecordQualityPlan(MsFrDraft const &, MsFrPlan const &);
#endif
        void DisposeOwners() noexcept;
        uint32_t AddFile(IDWriteFontFile *file);
        uint32_t AddFace(IDWriteFontFace5 *face);
        void Plan(MsFrDraft const &, MsFrPlan &);
        void FillFont(uint32_t, uint8_t *, uint64_t, uint64_t *);
        void FillMetrics(uint32_t, uint32_t, MsFrGlyphMetrics *, uint32_t, uint32_t *);
        void PrepareGeometry(MsFrGlyphKey const *, uint32_t, MsFrGeometryReceipt &);
        void FillGeometry(uint64_t, MsFrGlyphPath *, uint32_t, MsFrCommand *, uint32_t, uint32_t *, uint32_t *);
        void RetireGeometry(uint64_t);
        static Impl *Find(uint64_t, bool cleanup = false) noexcept;
        static int32_t __cdecl PlanCall(uint64_t, MsFrDraft const *, MsFrPlan *) noexcept;
        static int32_t __cdecl FontCall(uint64_t, uint32_t, uint8_t *, uint64_t, uint64_t *) noexcept;
        static int32_t __cdecl MetricsCall(uint64_t, uint32_t, uint32_t, MsFrGlyphMetrics *, uint32_t,
                                           uint32_t *) noexcept;
        static int32_t __cdecl PrepareCall(uint64_t, MsFrGlyphKey const *, uint32_t,
                                           MsFrGeometryReceipt *) noexcept;
        static int32_t __cdecl GeometryCall(uint64_t, uint64_t, MsFrGlyphPath *, uint32_t, MsFrCommand *, uint32_t,
                                            uint32_t *, uint32_t *) noexcept;
        static int32_t __cdecl RetireCall(uint64_t, uint64_t) noexcept;
        static int32_t __cdecl CoverCall(uint64_t, MsSvgCoverageBatch const *, MsSvgCoverageResult *) noexcept;
    };
    static std::atomic<uint64_t> NextSession{};
    static std::atomic<uint64_t> LateCalls{};
    static thread_local NativeSvgFontSession::Impl *Active{};

    NativeSvgFontSession::Impl *NativeSvgFontSession::Impl::Find(uint64_t id, bool cleanup) noexcept
    {
        for (auto current = Active; current; current = current->previous)
        {
            if (id && current->ticket == id)
            {
                if (current->creator != GetCurrentThreadId() || (current->closed && !cleanup))
                {
                    ++current->stats.lateCalls;
                    return nullptr;
                }
                return current;
            }
        }
        ++LateCalls;
        return nullptr;
    }
    template <class F> static HRESULT Guard(NativeSvgFontSession::Impl *self, F &&body) noexcept
    {
        if (!self)
            return Closed;
        try
        {
            body();
            return S_OK;
        }
        catch (HRESULT hr)
        {
            return self->Fault(hr);
        }
        catch (std::bad_alloc const &)
        {
            return self->Fault(E_OUTOFMEMORY);
        }
        catch (...)
        {
            return self->Fault(E_FAIL);
        }
    }

    void NativeSvgFontSession::Impl::Initialize()
    {
        Work();
        Check(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED, __uuidof(IDWriteFactory6),
                                  reinterpret_cast<IUnknown **>(factory.GetAddressOf())));
        Check(factory->GetSystemFontCollection(&collection));
        Check(factory->GetSystemFontFallback(&fallback));
        Work();
    }

    struct Hash final
    {
        BCRYPT_HASH_HANDLE value{};
        Hash()
        {
            auto s = BCryptCreateHash(BCRYPT_SHA256_ALG_HANDLE, &value, nullptr, 0, nullptr, 0, 0);
            if (s < 0)
                throw HRESULT_FROM_NT(s);
        }
        ~Hash()
        {
            if (value)
                BCryptDestroyHash(value);
        }
        void Add(void const *p, uint32_t n)
        {
            auto s = BCryptHashData(value, static_cast<PUCHAR>(const_cast<void *>(p)), n, 0);
            if (s < 0)
                throw HRESULT_FROM_NT(s);
        }
        void Finish(uint8_t *p)
        {
            auto s = BCryptFinishHash(value, p, 32, 0);
            if (s < 0)
                throw HRESULT_FROM_NT(s);
        }
    };
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
    static bool ValidQualityFamily(std::wstring const &value) noexcept
    {
        if (value.empty() || value.size() > 4096)
            return false;
        for (size_t i = 0; i < value.size(); ++i)
        {
            uint32_t unit = value[i];
            if (!unit)
                return false;
            if (unit >= 0xD800 && unit <= 0xDBFF)
            {
                if (++i == value.size() || value[i] < 0xDC00 || value[i] > 0xDFFF)
                    return false;
            }
            else if (unit >= 0xDC00 && unit <= 0xDFFF)
                return false;
        }
        return true;
    }

    void NativeSvgFontSession::Impl::InitializeQuality(NativeSvgFontQualityConfig const &config,
                                                       std::span<uint8_t const> source)
    {
        Work();
        if (!quality_trace || quality_trace->planCount || quality_trace->fileCount || quality_trace->faceCount ||
            quality_trace->axisCount || quality_trace->runCount || quality_trace->byteCount ||
            quality_trace->forcedCount || quality_trace->forcedBound || quality_trace->firstError != S_OK ||
            quality_trace->sourceVerified || quality_trace->completeValidated || source.empty() || !source.data() ||
            config.forced.size() > 32 || !std::isfinite(config.fallbackScale) || config.fallbackScale <= 0 ||
            config.rangeFirst > config.rangeLast || config.rangeLast > 0x10FFFF ||
            (config.fallbackFamily.empty() && (config.rangeFirst || config.rangeLast || config.fallbackScale != 1)))
            throw E_INVALIDARG;
        if (!config.fallbackFamily.empty() && !ValidQualityFamily(config.fallbackFamily))
            throw E_INVALIDARG;
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
        if (config.runtimeHook.point >= NativeSvgFontRuntimePoint::Count ||
            ((config.runtimeHook.point == NativeSvgFontRuntimePoint::None) != (config.runtimeHook.ordinal == 0)) ||
            (config.runtimeHook.point != NativeSvgFontRuntimePoint::None && SUCCEEDED(config.runtimeHook.failureHr)))
            throw E_INVALIDARG;
        runtime_hook = config.runtimeHook;
#endif

        Hash hash;
        for (size_t at = 0; at < source.size();)
        {
            uint32_t count = static_cast<uint32_t>((std::min)(size_t{65536}, source.size() - at));
            Work(count);
            hash.Add(source.data() + at, count);
            at += count;
        }
        hash.Finish(quality_trace->actualSourceSha256.data());
        if (quality_trace->actualSourceSha256 != config.sourceSha256)
            throw E_INVALIDARG;
        quality_trace->sourceVerified = true;
        quality_trace->forcedCount = static_cast<uint32_t>(config.forced.size());

        quality_forced.reserve(config.forced.size());
        for (auto const &input : config.forced)
        {
            Work(1);
            if (input.elementId.empty() || input.elementId.size() > 4096 || input.simulations > 3 ||
                !ValidQualityFamily(input.baseFamily) ||
                !ValidUtf8(reinterpret_cast<uint8_t const *>(input.elementId.data()),
                           static_cast<uint32_t>(input.elementId.size()), &memory.services) ||
                !std::any_of(input.fileSha256.begin(), input.fileSha256.end(), [](uint8_t byte) { return byte != 0; }))
                throw E_INVALIDARG;
            for (auto const &earlier : quality_forced)
                if (earlier.element.size() == input.elementId.size() &&
                    !std::memcmp(earlier.element.data(), input.elementId.data(), input.elementId.size()))
                    throw E_INVALIDARG;
            Work(input.elementId.size() + input.baseFamily.size());
            SvgFontQualityForcedOwner owner(&memory);
            auto element = reinterpret_cast<uint8_t const *>(input.elementId.data());
            owner.element.assign(element, element + input.elementId.size());
            owner.family.assign(input.baseFamily.begin(), input.baseFamily.end());
            owner.family.push_back(0);
            owner.sha = input.fileSha256;
            owner.index = input.faceIndex;
            owner.simulations = input.simulations;
            quality_forced.push_back(std::move(owner));
        }
        if (!config.fallbackFamily.empty())
        {
            Work(config.fallbackFamily.size());
            quality_fallback_family.assign(config.fallbackFamily.begin(), config.fallbackFamily.end());
            quality_fallback_family.push_back(0);
            ComPtr<IDWriteFontFallbackBuilder> builder;
            Check(factory->CreateFontFallbackBuilder(&builder));
            DWRITE_UNICODE_RANGE range{config.rangeFirst, config.rangeLast};
            wchar_t const *family = quality_fallback_family.data();
            Check(builder->AddMapping(&range, 1, &family, 1, collection.Get(), nullptr, nullptr, config.fallbackScale));
            Check(builder->AddMappings(fallback.Get()));
            ComPtr<IDWriteFontFallback> replacement;
            Check(builder->CreateFontFallback(&replacement));
            fallback = std::move(replacement);
        }
        Work();
    }

    void NativeSvgFontSession::Impl::RecordQualityPlan(MsFrDraft const &draft, MsFrPlan const &plan)
    {
        if (!quality_trace)
            return;
        auto &trace = *quality_trace;
        auto fits = [](uint32_t used, uint64_t count, size_t capacity) noexcept
        { return used <= capacity && count <= capacity - used; };
        uint64_t byte_count = static_cast<uint64_t>(draft.text_length) + draft.element_id_length + plan.strings_length;
        if (!trace.sourceVerified || !fits(trace.planCount, 1, trace.plans.size()) ||
            !fits(trace.fileCount, plan.file_count, trace.files.size()) ||
            !fits(trace.faceCount, plan.face_count, trace.faces.size()) ||
            !fits(trace.axisCount, plan.axis_count, trace.axes.size()) ||
            !fits(trace.runCount, plan.run_count, trace.runs.size()) ||
            !fits(trace.byteCount, byte_count, trace.bytes.size()))
            throw Quota;

        NativeSvgFontQualityPlan entry{};
        entry.draftId = draft.draft_id;
        entry.generation = plan.generation;
        entry.text = {trace.byteCount, draft.text_length};
        entry.elementId = {entry.text.start + entry.text.count, draft.element_id_length};
        entry.strings = {entry.elementId.start + entry.elementId.count, plan.strings_length};
        entry.files = {trace.fileCount, plan.file_count};
        entry.faces = {trace.faceCount, plan.face_count};
        entry.axes = {trace.axisCount, plan.axis_count};
        entry.runs = {trace.runCount, plan.run_count};
        auto copy = [&](auto const *from, uint32_t count, auto &to, uint32_t start)
        {
            for (uint32_t at = 0; at < count;)
            {
                uint32_t chunk = (std::min)(256u, count - at);
                Work(Bytes(chunk, sizeof(*from)));
                std::copy_n(from + at, chunk, to.data() + start + at);
                at += chunk;
            }
        };
        copy(draft.text, draft.text_length, trace.bytes, entry.text.start);
        copy(draft.element_id_length ? draft.strings + draft.element_id_start : nullptr, draft.element_id_length,
             trace.bytes, entry.elementId.start);
        copy(plan.strings, plan.strings_length, trace.bytes, entry.strings.start);
        copy(plan.files, plan.file_count, trace.files, entry.files.start);
        copy(plan.faces, plan.face_count, trace.faces, entry.faces.start);
        copy(plan.axes, plan.axis_count, trace.axes, entry.axes.start);
        copy(plan.runs, plan.run_count, trace.runs, entry.runs.start);
        Work();
        trace.plans[trace.planCount] = entry;
        trace.fileCount += plan.file_count;
        trace.faceCount += plan.face_count;
        trace.axisCount += plan.axis_count;
        trace.runCount += plan.run_count;
        trace.byteCount += static_cast<uint32_t>(byte_count);
        ++trace.planCount;
    }
#endif
    struct Fragment final
    {
        IDWriteFontFileStream *stream;
        void const *data{};
        void *context{};
        NativeSvgFontStatistics *stats;
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
        Fragment(IDWriteFontFileStream *s, uint64_t p, uint64_t n, NativeSvgFontStatistics *counters,
                 NativeSvgFontSession::Impl *session = nullptr)
#else
        Fragment(IDWriteFontFileStream *s, uint64_t p, uint64_t n, NativeSvgFontStatistics *counters)
#endif
            : stream(s), stats(counters)
        {
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
            if (session)
                session->RuntimeBoundary(NativeSvgFontRuntimePoint::FragmentRead, p, n);
#endif
            Check(stream->ReadFileFragment(&data, p, n, &context));
            ++stats->fragmentReads;
        }
        ~Fragment()
        {
            stream->ReleaseFileFragment(context);
            ++stats->fragmentReleases;
        }
    };
    uint32_t NativeSvgFontSession::Impl::AddFile(IDWriteFontFile *file)
    {
        Work();
        void const *key{};
        uint32_t key_size{};
        Check(file->GetReferenceKey(&key, &key_size));
        if (!key_size || key_size > 65536 || !key)
            throw E_INVALIDARG;
        ComPtr<IDWriteFontFileLoader> loader;
        Check(file->GetLoader(&loader));
        for (auto const &earlier : file_owners)
        {
            void const *previous_key{};
            uint32_t previous_size{};
            Check(earlier.file->GetReferenceKey(&previous_key, &previous_size));
            ComPtr<IDWriteFontFileLoader> previous_loader;
            Check(earlier.file->GetLoader(&previous_loader));
            if (previous_loader.Get() == loader.Get() && previous_size == key_size &&
                !std::memcmp(previous_key, key, key_size))
                return earlier.metadata.ticket;
        }
        ComPtr<IDWriteFontFileStream> stream;
        Check(loader->CreateStreamFromKey(key, key_size, &stream));
        uint64_t size{};
        Check(stream->GetFileSize(&size));
        if (size < 12 || size > FontLimit)
            throw Quota;
        SvgFontFileOwner owner;
        owner.file = file;
        owner.stream = stream;
        owner.metadata.byte_length = size;
        Hash hash;
        std::array<uint8_t, 12> header{};
        for (uint64_t p = 0; p < size;)
        {
            Work(1);
            uint32_t n = static_cast<uint32_t>((std::min)(65536ull, size - p));
            {
                Fragment fragment(stream.Get(), p, n, &stats);
                if (!p)
                    std::memcpy(header.data(), fragment.data, header.size());
                hash.Add(fragment.data, n);
            }
            Work();
            p += n;
        }
        hash.Finish(owner.metadata.sha256);
        owner.metadata.collection_faces = 1;
        if (!std::memcmp(header.data(), "ttcf", 4))
            owner.metadata.collection_faces =
                (uint32_t(header[8]) << 24) | (uint32_t(header[9]) << 16) | (uint32_t(header[10]) << 8) | header[11];
        if (!owner.metadata.collection_faces || owner.metadata.collection_faces > 65535)
            throw E_INVALIDARG;
        for (auto const &earlier : file_owners)
            if (earlier.metadata.byte_length == size &&
                !std::memcmp(earlier.metadata.sha256, owner.metadata.sha256, 32))
                return earlier.metadata.ticket;
        uint64_t total = size;
        for (auto const &earlier : file_owners)
            total += earlier.metadata.byte_length;
        if (file_owners.size() >= 16 || total > FontLimit)
            throw Quota;
        owner.metadata.ticket = static_cast<uint32_t>(file_owners.size()) + 1;
        file_owners.push_back(std::move(owner));
        return file_owners.back().metadata.ticket;
    }
    uint32_t NativeSvgFontSession::Impl::AddFace(IDWriteFontFace5 *face)
    {
        Work();
        uint32_t count{};
        Check(face->GetFiles(&count, nullptr));
        // TrueType/OpenType faces have exactly one source payload. Multi-file
        // legacy faces cannot be represented by this already-frozen private ABI.
        if (count != 1)
            throw DWRITE_E_FILEFORMAT;
        ComPtr<IDWriteFontFile> file;
        Check(face->GetFiles(&count, file.GetAddressOf()));
        uint32_t file_ticket = AddFile(file.Get());
        SvgFontFaceOwner owner(&memory);
        owner.face = face;
        owner.metadata.file_ticket = file_ticket;
        owner.metadata.face_index = face->GetIndex();
        owner.metadata.simulations = face->GetSimulations();
        if (owner.metadata.face_index >= file_owners[file_ticket - 1].metadata.collection_faces ||
            owner.metadata.simulations > 3)
            throw E_INVALIDARG;
        uint32_t n = face->GetFontAxisValueCount();
        if (n > 4096)
            throw Quota;
        owner.axes.resize(n);
        SvgFontVector<DWRITE_FONT_AXIS_VALUE> actual{SvgFontAllocator<DWRITE_FONT_AXIS_VALUE>(&memory)};
        actual.resize(n);
        Check(face->GetFontAxisValues(actual.data(), n));
        owner.metadata.axis_count = n;
        for (uint32_t i = 0; i < n; ++i)
        {
            if (!std::isfinite(actual[i].value))
                throw E_INVALIDARG;
            owner.axes[i] = {SwapTag(static_cast<uint32_t>(actual[i].axisTag)), actual[i].value};
        }
        std::sort(owner.axes.begin(), owner.axes.begin() + n,
                  [](auto const &a, auto const &b) { return a.tag_be < b.tag_be; });
        for (uint32_t i = 1; i < n; ++i)
            if (owner.axes[i - 1].tag_be == owner.axes[i].tag_be)
                throw E_INVALIDARG;
        DWRITE_FONT_METRICS1 m{};
        face->GetMetrics(&m);
        owner.metadata.units_per_em = m.designUnitsPerEm;
        owner.metadata.glyph_count = face->GetGlyphCount();
        if (!m.designUnitsPerEm || !owner.metadata.glyph_count || owner.metadata.glyph_count > 65535)
            throw E_INVALIDARG;
        owner.metadata.metrics = {m.designUnitsPerEm,
                                  m.ascent,
                                  m.descent,
                                  m.lineGap,
                                  m.capHeight,
                                  m.xHeight,
                                  m.underlinePosition,
                                  m.underlineThickness,
                                  m.strikethroughPosition,
                                  m.strikethroughThickness};
        for (auto const &earlier : face_owners)
        {
            auto const &a = earlier.metadata;
            auto const &b = owner.metadata;
            if (a.file_ticket == b.file_ticket && a.face_index == b.face_index && a.simulations == b.simulations &&
                a.axis_count == b.axis_count &&
                (!n || !std::memcmp(earlier.axes.data(), owner.axes.data(), n * sizeof(MsFrAxis))))
                return a.ticket;
        }
        if (face_owners.size() >= 32)
            throw Quota;
        owner.metadata.ticket = static_cast<uint32_t>(face_owners.size()) + 1;
        face_owners.push_back(std::move(owner));
        return face_owners.back().metadata.ticket;
    }

    static void ValidateDraft(MsFrDraft const &d, NativeSvgFontSession::Impl &session)
    {
        if (d.size != sizeof(d) || d.version != NOTEPADS_MARKDOWN_FR_ABI_VERSION || d.text_length > 1048576 || d.strings_length > 1048576 ||
            d.style_count > 65535 || d.logical_span_count > 65535 || d.axis_count > 4096 ||
            (d.style_count && !d.styles) || (d.logical_span_count && !d.logical_spans) || (d.axis_count && !d.axes))
            throw E_INVALIDARG;
        ValidatePointer(d.text, d.text_length);
        ValidatePointer(d.strings, d.strings_length);
        if (!ValidUtf8(d.text, d.text_length, &session.memory.services) ||
            !ValidUtf8(d.strings, d.strings_length, &session.memory.services))
            throw E_INVALIDARG;
        ValidatePointer(d.styles, d.style_count);
        ValidatePointer(d.logical_spans, d.logical_span_count);
        ValidatePointer(d.axes, d.axis_count);
        if ((!d.text_length && (d.style_count || d.logical_span_count)) ||
            (d.text_length && (!d.style_count || !d.logical_span_count)))
            throw E_INVALIDARG;
        if (d.element_id_length > 4096 || (!d.element_id_length && d.element_id_start))
            throw E_INVALIDARG;
        ValidateString(d, d.element_id_start, d.element_id_length, false);
        uint32_t end{};
        for (uint32_t i = 0; i < d.style_count; ++i)
        {
            if ((i & 255) == 0)
                session.Work(256);
            auto const &s = d.styles[i];
            if (s.utf8_start != end || !s.utf8_length || !InRange(end, s.utf8_length, d.text_length) ||
                !Boundary(d.text, end + s.utf8_length, d.text_length) || s.weight < 1 || s.weight > 1000 ||
                s.style > 2 || s.stretch < 1 || s.stretch > 9 || s.direction > 1 || s.optical_sizing > 1 ||
                !std::isfinite(s.size) || s.size <= 0 || !InRange(s.axis_start, s.axis_count, d.axis_count))
                throw E_INVALIDARG;
            ValidateString(d, s.family_start, s.family_length, true);
            ValidateString(d, s.language_start, s.language_length, true);
            for (uint32_t j = 0; j < s.axis_count; ++j)
            {
                auto const &axis = d.axes[s.axis_start + j];
                if (!std::isfinite(axis.value))
                    throw E_INVALIDARG;
                for (uint32_t k = 0; k < j; ++k)
                {
                    if ((k & 255) == 0)
                        session.Work(256);
                    if (d.axes[s.axis_start + k].tag_be == axis.tag_be)
                        throw E_INVALIDARG;
                }
            }
            end += s.utf8_length;
        }
        if (end != d.text_length)
            throw E_INVALIDARG;
        end = 0;
        for (uint32_t i = 0; i < d.logical_span_count; ++i)
        {
            if ((i & 255) == 0)
                session.Work(256);
            auto const &s = d.logical_spans[i];
            if (s.utf8_start != end || !s.utf8_length || !InRange(end, s.utf8_length, d.text_length) ||
                !Boundary(d.text, end + s.utf8_length, d.text_length) || s.bidi_level > 125 || s.orientation > 1)
                throw E_INVALIDARG;
            ValidateString(d, s.language_start, s.language_length, true);
            for (uint32_t shift = 0; shift < 32; shift += 8)
            {
                auto c = (s.script_be >> shift) & 255;
                if (!((c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z')))
                    throw E_INVALIDARG;
            }
            end += s.utf8_length;
        }
        if (end != d.text_length)
            throw E_INVALIDARG;
    }

    void NativeSvgFontSession::Impl::Plan(MsFrDraft const &d, MsFrPlan &output)
    {
        Work(1);
        ValidateDraft(d, *this);
        Work(d.text_length + d.strings_length + d.style_count + d.logical_span_count);
        Hash text_hash;
        uint32_t units{};
        for (uint32_t p = 0; p < d.text_length;)
        {
            Work(1);
            uint32_t n = (std::min)(65536u, d.text_length - p);
            text_hash.Add(d.text + p, n);
            for (uint32_t i = 0; i < n; ++i)
            {
                uint8_t c = d.text[p + i];
                if ((c & 0xC0) != 0x80)
                    units += c >= 0xF0 ? 2 : 1;
            }
            p += n;
        }
        uint8_t text_sha[32]{};
        text_hash.Finish(text_sha);
        SvgFontDraftOwner *draft{};
        for (auto &earlier : drafts)
        {
            if (earlier.id == d.draft_id)
                draft = &earlier;
            else if (d.element_id_length && earlier.element.size() == d.element_id_length &&
                     !std::memcmp(earlier.element.data(), d.strings + d.element_id_start, d.element_id_length))
                throw E_INVALIDARG;
        }
        if (draft)
        {
            if (draft->passes >= 2 || draft->element.size() != d.element_id_length ||
                std::memcmp(draft->text_sha, text_sha, 32) ||
                (d.element_id_length &&
                 std::memcmp(draft->element.data(), d.strings + d.element_id_start, d.element_id_length)))
                throw E_INVALIDARG;
        }
        else
        {
            if (drafts.size() >= 256)
                throw Quota;
            if (units > 262144 - total_utf16)
                throw Quota;
            SvgFontDraftOwner next(&memory);
            next.id = d.draft_id;
            std::memcpy(next.text_sha, text_sha, 32);
            if (d.element_id_length)
                next.element.assign(d.strings + d.element_id_start,
                                    d.strings + d.element_id_start + d.element_id_length);
            drafts.push_back(std::move(next));
            draft = &drafts.back();
            total_utf16 += units;
        }
        ++draft->passes;
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
        SvgFontQualityForcedOwner *force{};
        for (auto &candidate : quality_forced)
        {
            if (d.element_id_length && candidate.element.size() == d.element_id_length &&
                !std::memcmp(candidate.element.data(), d.strings + d.element_id_start, d.element_id_length))
            {
                if (candidate.matched && candidate.draft != d.draft_id)
                    throw E_INVALIDARG;
                candidate.matched = true;
                candidate.draft = d.draft_id;
                force = &candidate;
            }
        }
#endif
        uint32_t si{}, li{}, position{};
        while (position < d.text_length)
        {
            Work(1);
            auto const &style = d.styles[si];
            auto const &logical = d.logical_spans[li];
            uint32_t end = (std::min)(style.utf8_start + style.utf8_length, logical.utf8_start + logical.utf8_length);
            auto text = Wide(&memory, d.text + position, end - position);
            auto families = DecodeFamilies(&memory, d.strings + style.family_start, style.family_length);
            auto language = Wide(&memory, d.strings + logical.language_start, logical.language_length);
            ComPtr<IDWriteFontFallback> orderedFallback;
            IDWriteFontFallback *selectedFallback = fallback.Get();
            wchar_t const *baseFamily = families.names.front();
            if (families.names.size() > 1)
            {
                Work();
                ComPtr<IDWriteFontFallbackBuilder> builder;
                Check(factory->CreateFontFallbackBuilder(&builder));
                DWRITE_UNICODE_RANGE range{0, 0x10FFFF};
                Check(builder->AddMapping(&range, 1, families.names.data(),
                                          static_cast<uint32_t>(families.names.size()), collection.Get(),
                                          language.data(), nullptr, 1));
                Work();
                Check(builder->AddMappings(fallback.Get()));
                Check(builder->CreateFontFallback(&orderedFallback));
                Work();
                selectedFallback = orderedFallback.Get();
                baseFamily = nullptr;
            }
            SvgFontAnalysisSource analysis;
            analysis.text = text.data();
            analysis.length = static_cast<uint32_t>(text.size()) - 1;
            analysis.direction = logical.bidi_level & 1;
            analysis.language = language.data();
            SvgFontVector<uint32_t> byte_map{SvgFontAllocator<uint32_t>(&memory)};
            byte_map.resize(text.size(), UINT32_MAX);
            uint32_t bp = position, wp{}, mapped_work{};
            while (bp < end)
            {
                if (++mapped_work == 256)
                {
                    Work(256);
                    mapped_work = 0;
                }
                byte_map[wp] = bp;
                uint8_t c = d.text[bp];
                uint32_t n = c < 0x80 ? 1 : c < 0xE0 ? 2 : c < 0xF0 ? 3 : 4;
                wp += n == 4 ? 2 : 1;
                bp += n;
            }
            byte_map[wp] = end;
            uint32_t tp{};
            while (tp < analysis.length)
            {
                Work(1);
                uint32_t mapped{};
                float scale{};
                ComPtr<IDWriteFont> font;
                HRESULT map_status = selectedFallback->MapCharacters(
                    &analysis, tp, analysis.length - tp, collection.Get(), baseFamily,
                    static_cast<DWRITE_FONT_WEIGHT>(style.weight),
                    style.style == 1   ? DWRITE_FONT_STYLE_OBLIQUE
                    : style.style == 2 ? DWRITE_FONT_STYLE_ITALIC
                                       : DWRITE_FONT_STYLE_NORMAL,
                    static_cast<DWRITE_FONT_STRETCH>(style.stretch), &mapped, &font, &scale);
                Check(map_status);
                Work();
                if (!font || !mapped || mapped > analysis.length - tp || byte_map[tp] == UINT32_MAX ||
                    byte_map[tp + mapped] == UINT32_MAX || !std::isfinite(scale) || scale <= 0)
                    throw DWRITE_E_NOFONT;
                ComPtr<IDWriteFontFace> base;
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
                if (force)
                {
                    uint32_t family_index{};
                    BOOL exists{};
                    Check(collection->FindFamilyName(force->family.data(), &family_index, &exists));
                    if (!exists)
                        throw DWRITE_E_NOFONT;
                    ComPtr<IDWriteFontFamily> forced_family;
                    Check(collection->GetFontFamily(family_index, &forced_family));
                    ComPtr<IDWriteFont> regular;
                    Check(forced_family->GetFirstMatchingFont(DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_STRETCH_NORMAL,
                                                              DWRITE_FONT_STYLE_NORMAL, &regular));
                    Check(regular->CreateFontFace(&base));
                }
                else
#endif
                    Check(font->CreateFontFace(&base));
                ComPtr<IDWriteFontFace5> selected;
                Check(base.As(&selected));
                ComPtr<IDWriteFontResource> resource;
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
                if (force)
                {
                    uint32_t count{};
                    Check(base->GetFiles(&count, nullptr));
                    if (count != 1)
                        throw DWRITE_E_FILEFORMAT;
                    ComPtr<IDWriteFontFile> file;
                    Check(base->GetFiles(&count, file.GetAddressOf()));
                    uint32_t file_id = AddFile(file.Get());
                    auto const &metadata = file_owners[file_id - 1].metadata;
                    if (std::memcmp(metadata.sha256, force->sha.data(), force->sha.size()) ||
                        force->index >= metadata.collection_faces)
                        throw E_INVALIDARG;
                    Check(factory->CreateFontResource(file.Get(), force->index, &resource));
                }
                else
#endif
                    Check(selected->GetFontResource(&resource));
                SvgFontVector<DWRITE_FONT_AXIS_VALUE> requested{SvgFontAllocator<DWRITE_FONT_AXIS_VALUE>(&memory)};
                uint32_t axis_count = selected->GetFontAxisValueCount();
                if (axis_count > 4096)
                    throw Quota;
                requested.resize(axis_count);
                Check(selected->GetFontAxisValues(requested.data(), axis_count));
                auto set_axis = [&](uint32_t tag, float value)
                {
                    auto native_tag = static_cast<DWRITE_FONT_AXIS_TAG>(SwapTag(tag));
                    for (auto &axis : requested)
                        if (axis.axisTag == native_tag)
                        {
                            axis.value = value;
                            return;
                        }
                    if (requested.size() >= 4096)
                        throw Quota;
                    requested.push_back({native_tag, value});
                };
                if (style.optical_sizing)
                    set_axis(0x6F70737Au, style.size);
                for (uint32_t a = 0; a < style.axis_count; ++a)
                {
                    auto const &axis = d.axes[style.axis_start + a];
                    set_axis(axis.tag_be, axis.value);
                }
                ComPtr<IDWriteFontFace5> actual;
                auto simulations = base->GetSimulations();
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
                if (force)
                    simulations = static_cast<DWRITE_FONT_SIMULATIONS>(force->simulations);
#endif
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
                if (RuntimeBoundary(NativeSvgFontRuntimePoint::DefaultAxes, requested.size(), axis_count))
                {
                    auto count = resource->GetFontAxisCount();
                    if (count > 4096)
                        throw Quota;
                    requested.resize(count);
                    Check(resource->GetDefaultFontAxisValues(requested.data(), count));
                }
#endif
                Check(resource->CreateFontFace(simulations, requested.data(), static_cast<uint32_t>(requested.size()),
                                               &actual));
                Work();
                uint32_t face_id = AddFace(actual.Get());
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
                if (force)
                {
                    auto const &metadata = face_owners[face_id - 1].metadata;
                    if (metadata.face_index != force->index || metadata.simulations != force->simulations ||
                        std::memcmp(file_owners[metadata.file_ticket - 1].metadata.sha256, force->sha.data(),
                                    force->sha.size()))
                        throw E_INVALIDARG;
                    if (!force->bound)
                    {
                        force->bound = true;
                        ++quality_trace->forcedBound;
                    }
                }
#endif

                if (runs.size() >= 65535 || logical.language_length > 1048576 - strings.size())
                    throw Quota;
                uint32_t lp = static_cast<uint32_t>(strings.size());
                strings.insert(strings.end(), d.strings + logical.language_start,
                               d.strings + logical.language_start + logical.language_length);
                runs.push_back({byte_map[tp], byte_map[tp + mapped] - byte_map[tp], face_id, logical.script_be, lp,
                                logical.language_length, logical.bidi_level, logical.orientation, scale});
                tp += mapped;
            }
            position = end;
            if (end == style.utf8_start + style.utf8_length)
                ++si;
            if (end == logical.utf8_start + logical.utf8_length)
                ++li;
        }
        for (auto const &f : file_owners)
            files.push_back(f.metadata);
        for (auto const &f : face_owners)
        {
            MsFrFace face = f.metadata;
            if (face.axis_count > 4096 - axes.size())
                throw Quota;
            face.axis_start = static_cast<uint32_t>(axes.size());
            if (face.axis_count)
                axes.insert(axes.end(), f.axes.data(), f.axes.data() + face.axis_count);
            faces.push_back(face);
        }
        Work();
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
        if (RuntimeBoundary(NativeSvgFontRuntimePoint::SplitClusterPlan, runs.size(), d.text_length))
        {
            for (uint32_t byte = 1; byte < d.text_length; ++byte)
                if ((d.text[byte] & 0xC0) == 0x80)
                {
                    for (auto &run : runs)
                        if (byte > run.utf8_start && byte < run.utf8_start + run.utf8_length)
                        {
                            // Break an actual UTF-8/source cluster boundary in
                            // paid storage; the consumer must reject this plan.
                            run.utf8_length = byte - run.utf8_start;
                            break;
                        }
                    break;
                }
        }
        if (RuntimeBoundary(NativeSvgFontRuntimePoint::NonFiniteAxis, axes.size(), runs.size()))
        {
            if (!axes.empty())
                axes.front().value = std::numeric_limits<float>::quiet_NaN();
            else if (!runs.empty())
                runs.front().fallback_scale = 0;
        }
#endif
        if (generation == UINT64_MAX)
            throw Quota;
        ++generation;
        ++stats.plans;
        MsFrPlan prepared = {sizeof(output),
                               NOTEPADS_MARKDOWN_FR_ABI_VERSION,
                               generation,
                               files.data(),
                               static_cast<uint32_t>(files.size()),
                               faces.data(),
                               static_cast<uint32_t>(faces.size()),
                               axes.data(),
                               static_cast<uint32_t>(axes.size()),
                               runs.data(),
                               static_cast<uint32_t>(runs.size()),
                               strings.data(),
                               static_cast<uint32_t>(strings.size())};
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
        RecordQualityPlan(d, prepared);
#endif
        output = prepared;
    }

    void NativeSvgFontSession::Impl::FillFont(uint32_t id, uint8_t *destination, uint64_t capacity, uint64_t *written)
    {
        Work();
        if (!id || id > file_owners.size())
            throw E_INVALIDARG;
        auto &file = file_owners[id - 1];
        if (!destination || capacity != file.metadata.byte_length || capacity > SIZE_MAX || file.filled)
            throw E_INVALIDARG;
        ValidatePointer(destination, static_cast<uint32_t>(capacity));
        file.filled = true; // one attempt; failed owner's private prefix is discarded.
        Hash hash;
        for (uint64_t p = 0; p < capacity;)
        {
            Work(1);
            uint32_t n = static_cast<uint32_t>((std::min)(65536ull, capacity - p));
            {
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
                Fragment fragment(file.stream.Get(), p, n, &stats, this);
#else
                Fragment fragment(file.stream.Get(), p, n, &stats);
#endif
                std::memcpy(destination + static_cast<size_t>(p), fragment.data, n);
                hash.Add(fragment.data, n);
            }
            Work();
            p += n;
        }
        uint8_t digest[32]{};
        hash.Finish(digest);
        if (std::memcmp(digest, file.metadata.sha256, 32))
            throw DWRITE_E_FILEFORMAT;
        Work();
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
        if (RuntimeBoundary(NativeSvgFontRuntimePoint::CorruptOwnedFont, capacity, capacity))
            std::memset(destination, 0, static_cast<size_t>((std::min)(capacity, uint64_t{12})));
#endif
        ++stats.fontFills;
        *written = capacity;
    }
    void NativeSvgFontSession::Impl::FillMetrics(uint32_t id, uint32_t orientation, MsFrGlyphMetrics *destination,
                                                 uint32_t capacity, uint32_t *written)
    {
        Work();
        if (!id || id > face_owners.size() || orientation > 1)
            throw E_INVALIDARG;
        auto &owner = face_owners[id - 1];
        if (!owner.metadata.simulations || !destination || capacity != owner.metadata.glyph_count ||
            owner.metrics_filled[orientation])
            throw E_INVALIDARG;
        ValidatePointer(destination, capacity);
        owner.metrics_filled[orientation] = true;
        std::array<UINT16, 1024> glyphs{};
        std::array<DWRITE_GLYPH_METRICS, 1024> metrics{};
        for (uint32_t p = 0; p < capacity;)
        {
            Work(1);
            uint32_t n = (std::min)(1024u, capacity - p);
            for (uint32_t i = 0; i < n; ++i)
                glyphs[i] = static_cast<UINT16>(p + i);
            Check(owner.face->GetDesignGlyphMetrics(glyphs.data(), n, metrics.data(), orientation != 0));
            Work(n);
            for (uint32_t i = 0; i < n; ++i)
            {
                auto const &m = metrics[i];
                destination[p + i] = {m.leftSideBearing, m.advanceWidth,      m.rightSideBearing, m.topSideBearing,
                                      m.advanceHeight,   m.bottomSideBearing, m.verticalOriginY};
            }
            p += n;
        }
        Work();
        ++stats.metricsFills;
        *written = capacity;
    }

    class SvgFontGeometrySink final : public IDWriteGeometrySink
    {
        NativeSvgFontSession::Impl &session_;
        ULONG refs_{1};
        uint32_t orientation_{};
        uint32_t fill_rule_{};
        bool open_{};
        HRESULT fault_{};
        D2D1_POINT_2F Convert(D2D1_POINT_2F p)
        {
            if (!std::isfinite(p.x) || !std::isfinite(p.y))
                throw E_INVALIDARG;
            // ABI transports DWrite's original design-sized y-down coordinates.
            // Rust applies the frozen orientation matrix exactly once.
            return p;
        }
        template <class F> void Attempt(F &&operation) noexcept
        {
            if (fault_ != S_OK)
                return;
            try
            {
                operation();
            }
            catch (HRESULT hr)
            {
                fault_ = hr;
            }
            catch (std::bad_alloc const &)
            {
                fault_ = E_OUTOFMEMORY;
            }
            catch (...)
            {
                fault_ = E_FAIL;
            }
        }
        void Append(MsFrCommand const &c)
        {
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
            session_.RuntimeBoundary(NativeSvgFontRuntimePoint::SinkCommand, session_.commands.size(),
                                     session_.commands.capacity());
#endif
            if ((session_.commands.size() & 255) == 0)
                session_.Work(256);
            if (session_.commands.size() >= 1048576)
                throw Quota;
            session_.commands.push_back(c);
        }

      public:
        SvgFontGeometrySink(NativeSvgFontSession::Impl &session, uint32_t orientation)
            : session_(session), orientation_(orientation)
        {
        }
        HRESULT Result() const noexcept
        {
            return fault_ != S_OK ? fault_ : open_ ? E_UNEXPECTED : S_OK;
        }
        uint32_t FillRule() const noexcept
        {
            return fill_rule_;
        }
        HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **out) override
        {
            if (!out)
                return E_POINTER;
            *out = nullptr;
            if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWriteGeometrySink))
                return E_NOINTERFACE;
            *out = static_cast<IDWriteGeometrySink *>(this);
            AddRef();
            return S_OK;
        }
        ULONG STDMETHODCALLTYPE AddRef() override
        {
            return ++refs_;
        }
        ULONG STDMETHODCALLTYPE Release() override
        {
            return --refs_;
        }
        void STDMETHODCALLTYPE SetFillMode(D2D1_FILL_MODE mode) override
        {
            Attempt(
                [&]
                {
                    if (mode != D2D1_FILL_MODE_ALTERNATE && mode != D2D1_FILL_MODE_WINDING)
                        throw E_INVALIDARG;
                    fill_rule_ = mode == D2D1_FILL_MODE_WINDING ? 1u : 0u;
                });
        }
        void STDMETHODCALLTYPE SetSegmentFlags(D2D1_PATH_SEGMENT flags) override
        {
            Attempt(
                [&]
                {
                    if (flags != D2D1_PATH_SEGMENT_NONE)
                        throw E_INVALIDARG;
                });
        }
        void STDMETHODCALLTYPE BeginFigure(D2D1_POINT_2F point, D2D1_FIGURE_BEGIN begin) override
        {
            Attempt(
                [&]
                {
                    if (open_ || begin != D2D1_FIGURE_BEGIN_FILLED)
                        throw E_INVALIDARG;
                    auto p = Convert(point);
                    Append({0, p.x, p.y, 0, 0, 0, 0});
                    open_ = true;
                });
        }
        void STDMETHODCALLTYPE AddLines(D2D1_POINT_2F const *points, UINT32 count) override
        {
            Attempt(
                [&]
                {
                    if (!open_ || (count && !points))
                        throw E_INVALIDARG;
                    for (uint32_t i = 0; i < count; ++i)
                    {
                        auto p = Convert(points[i]);
                        Append({1, p.x, p.y, 0, 0, 0, 0});
                    }
                });
        }
        void STDMETHODCALLTYPE AddBeziers(D2D1_BEZIER_SEGMENT const *curves, UINT32 count) override
        {
            Attempt(
                [&]
                {
                    if (!open_ || (count && !curves))
                        throw E_INVALIDARG;
                    for (uint32_t i = 0; i < count; ++i)
                    {
                        auto a = Convert(curves[i].point1), b = Convert(curves[i].point2),
                             c = Convert(curves[i].point3);
                        Append({2, a.x, a.y, b.x, b.y, c.x, c.y});
                    }
                });
        }
        void STDMETHODCALLTYPE EndFigure(D2D1_FIGURE_END end) override
        {
            Attempt(
                [&]
                {
                    if (!open_ || end != D2D1_FIGURE_END_CLOSED)
                        throw E_INVALIDARG;
                    Append({3, 0, 0, 0, 0, 0, 0});
                    open_ = false;
                });
        }
        HRESULT STDMETHODCALLTYPE Close() override
        {
            return Result();
        }
    };

    void NativeSvgFontSession::Impl::PrepareGeometry(MsFrGlyphKey const *keys, uint32_t count,
                                                     MsFrGeometryReceipt &receipt)
    {
        Work();
        if (geometry_pending || count > 4096 || (count && !keys))
            throw E_INVALIDARG;
        ValidatePointer(keys, count);
        if (geometry_generation == UINT64_MAX)
            throw Quota;
        geometry_pending = true;
        geometry_filled = false;
        ++geometry_generation;
        // A failing prepare has no receipt; explicitly destroy actual buffers now.
        try
        {
            paths.reserve(count);
            for (uint32_t i = 0; i < count; ++i)
            {
                Work(1);
                auto const &key = keys[i];
                if (!key.face_ticket || key.face_ticket > face_owners.size() || key.orientation > 1 || key.reserved)
                    throw E_INVALIDARG;
                auto &face = face_owners[key.face_ticket - 1];
                if (!face.metadata.simulations || key.glyph_id >= face.metadata.glyph_count)
                    throw E_INVALIDARG;
                for (uint32_t j = 0; j < i; ++j)
                {
                    if ((j & 255) == 0)
                        Work(256);
                    if (keys[j].face_ticket == key.face_ticket && keys[j].glyph_id == key.glyph_id &&
                        keys[j].orientation == key.orientation)
                        throw E_INVALIDARG;
                }
                uint32_t start = static_cast<uint32_t>(commands.size());
                SvgFontGeometrySink sink(*this, key.orientation);
                UINT16 gid = static_cast<UINT16>(key.glyph_id);
                FLOAT advance = 0;
                DWRITE_GLYPH_OFFSET offset{};
                HRESULT hr = face.face->GetGlyphRunOutline(static_cast<FLOAT>(face.metadata.units_per_em), &gid,
                                                           &advance, &offset, 1, key.orientation != 0, FALSE, &sink);
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
                RuntimeBoundary(NativeSvgFontRuntimePoint::VisibleOutline, commands.size() - start,
                                commands.capacity());
#endif
                Check(sink.Result());
                Check(hr);
                Work();
                paths.push_back({i, start, static_cast<uint32_t>(commands.size()) - start, sink.FillRule()});
            }
            Work();
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
            RuntimeBoundary(NativeSvgFontRuntimePoint::HeldGeometry, commands.size(), memory.live[Geometry]);
            Work();
#endif
            ++stats.geometryPrepares;
            receipt = {
                sizeof(receipt),      NOTEPADS_MARKDOWN_FR_ABI_VERSION, geometry_generation, count, static_cast<uint32_t>(commands.size()),
                memory.live[Geometry]};
        }
        catch (...)
        {
            Free(paths);
            Free(commands);
            geometry_pending = false;
            throw;
        }
    }
    void NativeSvgFontSession::Impl::FillGeometry(uint64_t id, MsFrGlyphPath *p, uint32_t np, MsFrCommand *c,
                                                  uint32_t nc, uint32_t *wp, uint32_t *wc)
    {
        Work();
        if (!geometry_pending || id != geometry_generation)
        {
            ++stats.wrongGenerations;
            throw E_INVALIDARG;
        }
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
        if (RuntimeBoundary(NativeSvgFontRuntimePoint::CapacityShort, paths.size(), commands.size()))
        {
            if (nc)
                --nc;
            else if (np)
                --np;
        }
#endif
        if (geometry_filled || np != paths.size() || nc != commands.size() || (np && !p) || (nc && !c))
            throw E_INVALIDARG;
        ValidatePointer(p, np);
        ValidatePointer(c, nc);
        uintptr_t p_begin = reinterpret_cast<uintptr_t>(p), c_begin = reinterpret_cast<uintptr_t>(c);
        if (np && nc && p_begin < c_begin + Bytes(nc, sizeof(*c)) && c_begin < p_begin + Bytes(np, sizeof(*p)))
            throw E_INVALIDARG;
        geometry_filled = true;
#if defined(NOTEPADS_MARKDOWN_SVG_RUNTIME_TESTS)
        if (RuntimeBoundary(NativeSvgFontRuntimePoint::MirrorGeometryX, commands.size(), commands.capacity()))
            for (auto &command : commands)
            {
                if (command.kind < 3)
                    command.x1 = -command.x1;
                if (command.kind == 2)
                {
                    command.x2 = -command.x2;
                    command.x3 = -command.x3;
                }
            }
#endif
        for (uint32_t i = 0; i < np;)
        {
            Work(1);
            uint32_t n = (std::min)(256u, np - i);
            std::copy_n(paths.data() + i, n, p + i);
            i += n;
        }
        for (uint32_t i = 0; i < nc;)
        {
            Work(1);
            uint32_t n = (std::min)(256u, nc - i);
            std::copy_n(commands.data() + i, n, c + i);
            i += n;
        }
        Work();
        ++stats.geometryFills;
        *wp = np;
        *wc = nc;
    }
    void NativeSvgFontSession::Impl::RetireGeometry(uint64_t id)
    {
        if (!geometry_pending || id != geometry_generation)
        {
            ++stats.wrongGenerations;
            throw E_INVALIDARG;
        }
        Free(paths);
        Free(commands);
        geometry_pending = false;
        geometry_filled = false;
        ++stats.geometryRetirements;
    }

    int32_t __cdecl NativeSvgFontSession::Impl::PlanCall(uint64_t id, MsFrDraft const *d, MsFrPlan *out) noexcept
    {
        if (out)
            *out = {};
        auto self = Find(id);
        return Guard(self,
                     [&]
                     {
                         self->InvalidateMetadata();
                         if (!d || !out)
                             throw E_POINTER;
                         self->Plan(*d, *out);
                     });
    }
    int32_t __cdecl NativeSvgFontSession::Impl::FontCall(uint64_t id, uint32_t file, uint8_t *p, uint64_t n,
                                                         uint64_t *written) noexcept
    {
        if (written)
            *written = 0;
        auto self = Find(id);
        return Guard(self,
                     [&]
                     {
                         if (!written)
                             throw E_POINTER;
                         self->FillFont(file, p, n, written);
                     });
    }
    int32_t __cdecl NativeSvgFontSession::Impl::MetricsCall(uint64_t id, uint32_t face, uint32_t orientation,
                                                            MsFrGlyphMetrics *p, uint32_t n,
                                                            uint32_t *written) noexcept
    {
        if (written)
            *written = 0;
        auto self = Find(id);
        return Guard(self,
                     [&]
                     {
                         if (!written)
                             throw E_POINTER;
                         self->FillMetrics(face, orientation, p, n, written);
                     });
    }
    int32_t __cdecl NativeSvgFontSession::Impl::PrepareCall(uint64_t id, MsFrGlyphKey const *k, uint32_t n,
                                                            MsFrGeometryReceipt *out) noexcept
    {
        if (out)
            *out = {};
        auto self = Find(id);
        return Guard(self,
                     [&]
                     {
                         if (!out)
                             throw E_POINTER;
                         self->PrepareGeometry(k, n, *out);
                     });
    }
    int32_t __cdecl NativeSvgFontSession::Impl::GeometryCall(uint64_t id, uint64_t generation, MsFrGlyphPath *p,
                                                             uint32_t np, MsFrCommand *c, uint32_t nc, uint32_t *wp,
                                                             uint32_t *wc) noexcept
    {
        if (wp)
            *wp = 0;
        if (wc)
            *wc = 0;
        auto self = Find(id);
        return Guard(self,
                     [&]
                     {
                         if (!wp || !wc)
                             throw E_POINTER;
                         self->FillGeometry(generation, p, np, c, nc, wp, wc);
                     });
    }

    int32_t __cdecl NativeSvgFontSession::Impl::RetireCall(uint64_t id, uint64_t generation) noexcept
    {
        auto self = Find(id, true);
        return Guard(self, [&] { self->RetireGeometry(generation); });
    }

    int32_t __cdecl NativeSvgFontSession::Impl::CoverCall(uint64_t id, MsSvgCoverageBatch const *input,
                                                          MsSvgCoverageResult *output) noexcept
    {
        auto self = Find(id);
        auto status = Guard(self, [&] { self->Work(0); });
        if (status != S_OK)
            return status;
        status = self->coverage.CoverBatch(input, output);
        auto work = self->coverage.FirstWorkFault();
        if (work != S_OK)
            self->Fault(work);
        else if (status != S_OK && self->coverage.CleanupFault() == S_OK)
            self->Fault(status);
        if (self->coverage.IsQuarantined())
            self->closed = true;
        return status;
    }

    void NativeSvgFontSession::Impl::DisposeOwners() noexcept
    {
        InvalidateMetadata();
        Free(paths);
        Free(commands);
        Free(drafts);
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
        Free(quality_forced);
        Free(quality_fallback_family);
        quality_trace = nullptr;
#endif
        Free(face_owners);
        Free(file_owners);
        fallback.Reset();
        collection.Reset();
        factory.Reset();
    }

    HRESULT NativeSvgFontSession::Create(notepads_markdown_svg_resources *resources, NativeSvgFontSession *output) noexcept
    {
        return CreateInternal(resources, output);
    }

#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
    HRESULT NativeSvgFontSession::CreateForQuality(notepads_markdown_svg_resources *resources,
                                                   NativeSvgFontQualityConfig const &config,
                                                   std::span<uint8_t const> actualSource,
                                                   NativeSvgFontQualityTrace *trace,
                                                   NativeSvgFontSession *output) noexcept
    {
        return CreateInternal(resources, output, &config, actualSource, trace);
    }

    HRESULT NativeSvgFontSession::ValidateQualityComplete() noexcept
    {
        if (!_impl || _impl->creator != GetCurrentThreadId())
        {
            ++LateCalls;
            return Closed;
        }
        auto self = _impl;
        return Guard(self,
                     [&]
                     {
                         self->Work();
                         auto trace = self->quality_trace;
                         if (!trace || !trace->sourceVerified || trace->completeValidated)
                             throw E_INVALIDARG;
                         for (auto const &entry : self->quality_forced)
                         {
                             self->Work(1);
                             if (!entry.bound)
                                 throw E_INVALIDARG;
                         }
                         if (trace->forcedBound != self->quality_forced.size())
                             throw E_UNEXPECTED;
                         trace->completeValidated = true;
                     });
    }
#endif

    HRESULT NativeSvgFontSession::CreateInternal(notepads_markdown_svg_resources *resources, NativeSvgFontSession *output
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
                                                 ,
                                                 NativeSvgFontQualityConfig const *config,
                                                 std::span<uint8_t const> actualSource, NativeSvgFontQualityTrace *trace
#endif
                                                 ) noexcept
    {
        if (!resources || !output || output->_impl || output->_attempted)
            return E_INVALIDARG;
        output->_attempted = true;
        NativeSvgFontRetirement receipt{};
        receipt.workerThread = GetCurrentThreadId();
        auto failed = [&](HRESULT work) noexcept
        {
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
            if (trace && trace->firstError == S_OK)
                trace->firstError = work;
#endif
            receipt.firstWorkFault = work;
            receipt.retired = receipt.cleanupError == S_OK;
            output->_creationFailure = receipt;
            output->_creationFailed = true;
            output->_finalStatistics.firstFault = work;
            output->_finalStatistics.failedReleases = receipt.failedReleases;
            return work;
        };
        SvgFontServices services{resources};
        notepads_markdown_svg_work_token lease{};
        HRESULT hr = services.reserve(resources, Scratch, sizeof(Impl), &lease);
        if (hr != S_OK)
            return failed(RecordFault(resources, hr));
        void *storage = ::operator new(sizeof(Impl), std::nothrow);
        if (!storage)
        {
            RecordFault(resources, E_OUTOFMEMORY);
            HRESULT released = services.release(resources, lease);
            if (released != S_OK)
            {
                receipt.cleanupError = released;
                receipt.failedReleases = 1;
                receipt.unreleasedObject = lease;
            }
            return failed(E_OUTOFMEMORY);
        }
        Impl *p{};
        try
        {
            p = new (storage) Impl(services, lease);
            hr = services.admit(resources, lease, sizeof(Impl));
            Check(hr);
            p->Initialize();
#if defined(NOTEPADS_MARKDOWN_SVG_QUALITY_TESTS)
            p->quality_trace = trace;
            if (config)
                p->InitializeQuality(*config, actualSource);
            else if (trace || !actualSource.empty())
                throw E_INVALIDARG;
#endif
            uint64_t expected = NextSession.load(std::memory_order_relaxed);
            do
            {
                if (expected == UINT64_MAX)
                    throw Quota;
            } while (!NextSession.compare_exchange_weak(expected, expected + 1, std::memory_order_relaxed));
            p->ticket = expected + 1;
            p->previous = Active;
            Active = p;
            p->registered = true;
            output->_impl = p;
            return S_OK;
        }
        catch (HRESULT error)
        {
            hr = error;
        }
        catch (std::bad_alloc const &)
        {
            hr = E_OUTOFMEMORY;
        }
        catch (...)
        {
            hr = E_FAIL;
        }
        RecordFault(resources, hr);
        if (p)
        {
            p->closed = true;
            p->DisposeOwners();
            receipt.completedStages |= 1;
            for (auto live : p->memory.live)
                receipt.remainingNativeBytes += live;
            receipt.failedReleases = p->memory.failed_releases;
            receipt.cleanupError = p->memory.cleanup_error;
            if (receipt.remainingNativeBytes && receipt.cleanupError == S_OK)
                receipt.cleanupError = E_UNEXPECTED;
            receipt.fragmentReads = p->stats.fragmentReads;
            receipt.fragmentReleases = p->stats.fragmentReleases;
            p->~Impl();
        }
        ::operator delete(storage);
        receipt.completedStages |= 4;
        HRESULT released = services.release(resources, lease);
        if (released == S_OK)
            receipt.completedStages |= 8;
        else
        {
            ++receipt.failedReleases;
            receipt.unreleasedObject = lease;
            if (receipt.cleanupError == S_OK)
                receipt.cleanupError = released;
            RecordFault(resources, released);
        }
        return failed(hr);
    }

    NativeSvgFontSession::~NativeSvgFontSession()
    {
        if (!_impl)
            return;
        NativeSvgFontRetirement ignored{};
        (void)FinishOnWorker(&ignored);
    }
    MsSvgFontCallbacks NativeSvgFontSession::Callbacks() const noexcept
    {
        if (!_impl || _impl->creator != GetCurrentThreadId() || _impl->closed)
            return {};
        return {sizeof(MsSvgFontCallbacks),
                NOTEPADS_MARKDOWN_SVG_FONT_CALLBACKS_VERSION,
                _impl->ticket,
                Impl::PlanCall,
                Impl::FontCall,
                Impl::MetricsCall,
                Impl::PrepareCall,
                Impl::GeometryCall,
                Impl::RetireCall};
    }
    MsSvgCoverageCallbacks NativeSvgFontSession::CoverageCallbacks() const noexcept
    {
        if (!_impl || _finished || _impl->creator != GetCurrentThreadId() || _impl->closed)
            return {};
        MsSvgCoverageCallbacks result{};
        result.size = sizeof(result);
        result.version = NOTEPADS_MARKDOWN_SVG_COVERAGE_ABI;
        result.session = _impl->ticket;
        result.cover_batch = Impl::CoverCall;
        return result;
    }
    SvgCoverageStatistics NativeSvgFontSession::CoverageStatistics() const noexcept
    {
        return _impl && _impl->creator == GetCurrentThreadId() ? _impl->coverage.Statistics()
                                                               : _finalCoverageStatistics;
    }
#if defined(NOTEPADS_MARKDOWN_SVG_COVERAGE_TESTS)
    void NativeSvgFontSession::SetCoverageTestHooks(SvgCoverageTestHooks const &hooks) noexcept
    {
        if (_impl && !_finished && _impl->creator == GetCurrentThreadId() && !_impl->closed)
            _impl->coverage.SetTestHooks(hooks);
    }
    IUnknown *NativeSvgFontSession::TakeRetainedCoverageTestOwner() noexcept
    {
        return _impl && _impl->creator == GetCurrentThreadId() ? _impl->coverage.TakeRetainedTestOwner() : nullptr;
    }
#endif
    NativeSvgFontStatistics NativeSvgFontSession::Statistics() const noexcept
    {
        return _impl && _impl->creator == GetCurrentThreadId() ? _impl->stats : _finalStatistics;
    }
    uint64_t NativeSvgFontSession::ModuleLateCalls() noexcept
    {
        return LateCalls.load(std::memory_order_relaxed);
    }
    HRESULT NativeSvgFontSession::FinishOnWorker(NativeSvgFontRetirement *output) noexcept
    {
        if (!output || _finished || !_attempted)
            return E_INVALIDARG;
        if (_creationFailed)
        {
            if (_creationFailure.workerThread != GetCurrentThreadId())
            {
                ++LateCalls;
                return Closed;
            }
            _finished = true;
            *output = _creationFailure;
            return output->cleanupError;
        }
        if (!_impl)
            return E_INVALIDARG;
        auto p = _impl;
        if (p->creator != GetCurrentThreadId())
        {
            ++LateCalls;
            return Closed;
        }
        if (p->coverage.IsWorking())
            return Closed;
        NativeSvgFontRetirement receipt{};
        receipt.session = p->ticket;
        receipt.workerThread = p->creator;
        receipt.firstWorkFault = p->stats.firstFault;
        receipt.cleanupError = p->coverage.CleanupFault();
        p->closed = true;
        if ((p->geometry_pending || p->stats.fragmentReads != p->stats.fragmentReleases) &&
            receipt.cleanupError == S_OK)
            receipt.cleanupError = E_UNEXPECTED;
        p->DisposeOwners();
        receipt.completedStages |= 1;
        for (auto live : p->memory.live)
            receipt.remainingNativeBytes += live;
        receipt.failedReleases = p->memory.failed_releases;
        if (receipt.cleanupError == S_OK)
            receipt.cleanupError = p->memory.cleanup_error;
        if (receipt.remainingNativeBytes && receipt.cleanupError == S_OK)
            receipt.cleanupError = E_UNEXPECTED;
        receipt.fragmentReads = p->stats.fragmentReads;
        receipt.fragmentReleases = p->stats.fragmentReleases;
        _finalStatistics = p->stats;
        _finalCoverageStatistics = p->coverage.Statistics();
        _finalStatistics.failedReleases = receipt.failedReleases;
        Impl **link = &Active;
        while (*link && *link != p)
            link = &(*link)->previous;
        if (*link == p)
        {
            *link = p->previous;
            receipt.completedStages |= 2;
        }
        else if (receipt.cleanupError == S_OK)
            receipt.cleanupError = E_UNEXPECTED;
        p->registered = false;
        auto services = p->memory.services;
        auto lease = p->object_lease;
        if (p->coverage.HasContextDependency() || p->coverage.IsQuarantined())
        {
            // The existing canonical task keeps this wrapper, account and module
            // pinned when fontRetired=false. Preserve its Impl/context and paid
            // sentinel; a late external Release must never reach a dead ledger.
            receipt.remainingNativeBytes += sizeof(Impl);
            receipt.unreleasedObject = lease;
            if (receipt.cleanupError == S_OK)
                receipt.cleanupError = p->coverage.CleanupFault();
            if (receipt.cleanupError == S_OK)
                receipt.cleanupError = E_UNEXPECTED;
            _finished = true;
            RecordFault(services.context, receipt.cleanupError);
            receipt.retired = false;
            *output = receipt;
            return receipt.cleanupError;
        }
        _impl = nullptr;
        _finished = true;
        p->~Impl();
        ::operator delete(p);
        receipt.completedStages |= 4;
        HRESULT released = services.release(services.context, lease);
        if (released == S_OK)
            receipt.completedStages |= 8;
        else
        {
            ++receipt.failedReleases;
            receipt.unreleasedObject = lease;
            if (receipt.cleanupError == S_OK)
                receipt.cleanupError = released;
        }
        if (receipt.cleanupError != S_OK)
            RecordFault(services.context, receipt.cleanupError);
        receipt.retired = receipt.cleanupError == S_OK;
        *output = receipt;
        return receipt.cleanupError;
    }
}
