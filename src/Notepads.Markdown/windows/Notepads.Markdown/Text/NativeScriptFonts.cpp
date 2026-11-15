// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE file in the project root for license information.
// ---------------------------------------------------------------------------------------------
#include "pch.h"
#include "Text/NativeScriptFonts.h"
#include "Rendering/NativeDocumentRenderer.h"
#include <dwrite_3.h>
#include <cwchar>
#include <cstring>
#include <map>
#include <string_view>
#include <type_traits>
#include <utility>

namespace Notepads::Markdown::Native
{
    namespace
    {
        template <typename F> HRESULT FontBoundary(F &&body) noexcept
        {
            try
            {
                return body();
            }
            catch (std::bad_alloc const &)
            {
                return E_OUTOFMEMORY;
            }
            catch (PreviewFailure const &failure)
            {
                return failure.code();
            }
            catch (winrt::hresult_error const &failure)
            {
                return failure.code();
            }
            catch (...)
            {
                return E_FAIL;
            }
        }

        template <typename T> winrt::com_ptr<T> Retain(T *value)
        {
            winrt::com_ptr<T> result;
            result.copy_from(value);
            return result;
        }

        void RequireFonts(bool condition)
        {
            if (!condition)
                throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_PROVENANCE, L"script-fonts"};
        }

        void CheckFontEpoch(IDWriteFontCollection *collection)
        {
            auto three = Retain(collection).try_as<IDWriteFontCollection3>();
            if (!three)
                return;
            auto event = three->GetExpirationEvent();
            if (!event)
                return;
            auto status = ::WaitForSingleObjectEx(event, 0, FALSE);
            if (status == WAIT_OBJECT_0)
                winrt::throw_hresult(E_CHANGED_STATE);
            if (status != WAIT_TIMEOUT)
                winrt::throw_hresult(HRESULT_FROM_WIN32(status == WAIT_FAILED ? ::GetLastError() : ERROR_INVALID_STATE));
        }

        struct FontComMemory
        {
            static void *operator new(size_t bytes, notepads_markdown_preview_layout *owner) { return AllocatePreview(owner, bytes); }
            static void operator delete(void *value) noexcept { FreePreview(value); }
            static void operator delete(void *value, notepads_markdown_preview_layout *) noexcept { FreePreview(value); }
        };

#define SCRIPT_FONT_REFS \
      private: std::atomic<ULONG> _references{1}; \
      public: \
        ULONG STDMETHODCALLTYPE AddRef() noexcept override { return ++_references; } \
        ULONG STDMETHODCALLTYPE Release() noexcept override \
        { auto count = --_references; if (!count) delete this; return count; } \
      private:

        struct __declspec(uuid("017ed01e-134c-4cbd-a35c-c957c294c5c0")) ScriptFontToken : IUnknown
        {
            virtual HRESULT STDMETHODCALLTYPE Original(IDWriteFontCollection **collection, UINT32 *variant,
                void const **provider) noexcept = 0;
        };
        struct ProviderIdentity {};

        class CollectionProxy final : public IDWriteFontCollection3, public ScriptFontToken, public FontComMemory
        {
            SCRIPT_FONT_REFS
            winrt::com_ptr<IDWriteFontCollection> _original;
            winrt::com_ptr<IDWriteFontCollection1> _one;
            winrt::com_ptr<IDWriteFontCollection2> _two;
            winrt::com_ptr<IDWriteFontCollection3> _three;
            UINT32 _variant;
            std::shared_ptr<ProviderIdentity> _provider;

          public:
            CollectionProxy(IDWriteFontCollection *original, UINT32 variant, std::shared_ptr<ProviderIdentity> provider)
                : _variant(variant), _provider(std::move(provider))
            {
                _original.copy_from(original);
                _one = _original.try_as<IDWriteFontCollection1>();
                _two = _original.try_as<IDWriteFontCollection2>();
                _three = _original.try_as<IDWriteFontCollection3>();
            }
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = nullptr;
                if (iid == __uuidof(IUnknown) || iid == __uuidof(IDWriteFontCollection))
                    *value = static_cast<IDWriteFontCollection *>(this);
                else if (iid == __uuidof(IDWriteFontCollection1) && _one)
                    *value = static_cast<IDWriteFontCollection1 *>(this);
                else if (iid == __uuidof(IDWriteFontCollection2) && _two)
                    *value = static_cast<IDWriteFontCollection2 *>(this);
                else if (iid == __uuidof(IDWriteFontCollection3) && _three)
                    *value = static_cast<IDWriteFontCollection3 *>(this);
                else if (iid == __uuidof(ScriptFontToken))
                    *value = static_cast<ScriptFontToken *>(this);
                else return E_NOINTERFACE;
                AddRef();
                return S_OK;
            }
            HRESULT STDMETHODCALLTYPE Original(IDWriteFontCollection **value, UINT32 *variant,
                void const **provider) noexcept override
            {
                if (!value || !variant || !provider) return E_POINTER;
                *value = nullptr;
                *variant = _variant;
                *provider = _provider.get();
                _original.copy_to(value);
                return S_OK;
            }
            UINT32 STDMETHODCALLTYPE GetFontFamilyCount() noexcept override { return _original->GetFontFamilyCount(); }
            HRESULT STDMETHODCALLTYPE GetFontFamily(UINT32 index, IDWriteFontFamily **value) noexcept override
            { if (!value) return E_POINTER; *value = nullptr; return FontBoundary([&] { return _original->GetFontFamily(index, value); }); }
            HRESULT STDMETHODCALLTYPE FindFamilyName(WCHAR const *name, UINT32 *index, BOOL *exists) noexcept override
            { if (!index || !exists) return E_POINTER; *index = UINT32_MAX; *exists = FALSE; return FontBoundary([&] { return _original->FindFamilyName(name, index, exists); }); }
            HRESULT STDMETHODCALLTYPE GetFontFromFontFace(IDWriteFontFace *face, IDWriteFont **value) noexcept override
            { if (!value) return E_POINTER; *value = nullptr; return FontBoundary([&] { return _original->GetFontFromFontFace(face, value); }); }
            HRESULT STDMETHODCALLTYPE GetFontSet(IDWriteFontSet **value) noexcept override
            { if (!value) return E_POINTER; *value = nullptr; return FontBoundary([&] { return _one ? _one->GetFontSet(value) : E_NOINTERFACE; }); }
            HRESULT STDMETHODCALLTYPE GetFontFamily(UINT32 index, IDWriteFontFamily1 **value) noexcept override
            { if (!value) return E_POINTER; *value = nullptr; return FontBoundary([&] { return _one ? _one->GetFontFamily(index, value) : E_NOINTERFACE; }); }
            HRESULT STDMETHODCALLTYPE GetFontFamily(UINT32 index, IDWriteFontFamily2 **value) noexcept override
            { if (!value) return E_POINTER; *value = nullptr; return FontBoundary([&] { return _two ? _two->GetFontFamily(index, value) : E_NOINTERFACE; }); }
            HRESULT STDMETHODCALLTYPE GetMatchingFonts(WCHAR const *name, DWRITE_FONT_AXIS_VALUE const *axes,
                UINT32 count, IDWriteFontList2 **value) noexcept override
            { if (!value) return E_POINTER; *value = nullptr; return FontBoundary([&] { return _two ? _two->GetMatchingFonts(name, axes, count, value) : E_NOINTERFACE; }); }
            DWRITE_FONT_FAMILY_MODEL STDMETHODCALLTYPE GetFontFamilyModel() noexcept override
            { return _two ? _two->GetFontFamilyModel() : DWRITE_FONT_FAMILY_MODEL_WEIGHT_STRETCH_STYLE; }
            HRESULT STDMETHODCALLTYPE GetFontSet(IDWriteFontSet1 **value) noexcept override
            { if (!value) return E_POINTER; *value = nullptr; return FontBoundary([&] { return _two ? _two->GetFontSet(value) : E_NOINTERFACE; }); }
            HANDLE STDMETHODCALLTYPE GetExpirationEvent() noexcept override
            { return _three ? _three->GetExpirationEvent() : nullptr; }
        };

        constexpr uint64_t FileKeyMagic = 0x31534650444d534eull;
        constexpr uint64_t CollectionKeyMagic = 0x31435350444d534eull;
        struct FileKey { uint64_t magic, index; uint32_t variant, reserved; };
        struct CollectionKey { uint64_t magic, index; };
        struct OriginalFile
        {
            winrt::com_ptr<IDWriteFontFileLoader> loader;
            winrt::com_ptr<IUnknown> identity;
            PreviewVector<uint8_t> key;
            explicit OriginalFile(notepads_markdown_preview_layout *owner) : key(PreviewAllocator<uint8_t>{owner}) {}
        };
        static_assert(std::is_nothrow_move_constructible_v<OriginalFile>);
        struct CollectionFiles
        {
            PreviewVector<uint64_t> files;
            uint32_t variant{};
            explicit CollectionFiles(notepads_markdown_preview_layout *owner) : files(PreviewAllocator<uint64_t>{owner}) {}
        };
        struct FileLedger
        {
            notepads_markdown_preview_layout *owner;
            std::mutex mutex;
            PreviewVector<OriginalFile> files;
            PreviewVector<std::shared_ptr<CollectionFiles>> collections;
            explicit FileLedger(notepads_markdown_preview_layout *value) : owner(value),
                files(PreviewAllocator<OriginalFile>{value}),
                collections(PreviewAllocator<std::shared_ptr<CollectionFiles>>{value}) {}

            uint64_t Add(IDWriteFontFile *file)
            {
                winrt::com_ptr<IDWriteFontFileLoader> loader;
                winrt::check_hresult(file->GetLoader(loader.put()));
                auto identity = loader.as<IUnknown>();
                void const *key{};
                UINT32 bytes{};
                winrt::check_hresult(file->GetReferenceKey(&key, &bytes));
                RequireFonts(bytes && key);
                std::scoped_lock lock{mutex};
                for (size_t index = 0; index < files.size(); ++index)
                {
                    auto const &candidate = files[index];
                    if (candidate.identity.get() == identity.get() && candidate.key.size() == bytes &&
                        std::memcmp(candidate.key.data(), key, bytes) == 0)
                        return index;
                }
                OriginalFile entry{owner};
                entry.loader = std::move(loader);
                entry.identity = std::move(identity);
                entry.key.assign(static_cast<uint8_t const *>(key), static_cast<uint8_t const *>(key) + bytes);
                files.push_back(std::move(entry));
                return files.size() - 1;
            }
            uint64_t AddCollection(std::span<uint64_t const> ids, uint32_t variant)
            {
                auto entry = std::allocate_shared<CollectionFiles>(PreviewAllocator<CollectionFiles>{owner}, owner);
                entry->files.assign(ids.begin(), ids.end());
                entry->variant = variant;
                std::scoped_lock lock{mutex};
                collections.push_back(std::move(entry));
                return collections.size() - 1;
            }
        };

        class FileLoader final : public IDWriteFontFileLoader, public FontComMemory
        {
            SCRIPT_FONT_REFS
            std::shared_ptr<FileLedger> _ledger;
          public:
            explicit FileLoader(std::shared_ptr<FileLedger> ledger) : _ledger(std::move(ledger)) {}
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWriteFontFileLoader)) return E_NOINTERFACE;
                *value = static_cast<IDWriteFontFileLoader *>(this);
                AddRef(); return S_OK;
            }
            HRESULT STDMETHODCALLTYPE CreateStreamFromKey(void const *key, UINT32 bytes,
                IDWriteFontFileStream **value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = nullptr;
                return FontBoundary([&]() -> HRESULT
                {
                    if (!key || bytes != sizeof(FileKey)) return E_INVALIDARG;
                    FileKey parsed{};
                    std::memcpy(&parsed, key, sizeof(parsed));
                    if (parsed.magic != FileKeyMagic || parsed.variant > 1 || parsed.reserved) return E_INVALIDARG;
                    winrt::com_ptr<IDWriteFontFileLoader> original;
                    void const *originalKey{};
                    UINT32 originalBytes{};
                    {
                        std::scoped_lock lock{_ledger->mutex};
                        if (parsed.index >= _ledger->files.size()) return E_INVALIDARG;
                        auto const &entry = _ledger->files[static_cast<size_t>(parsed.index)];
                        original = entry.loader;
                        originalKey = entry.key.data();
                        originalBytes = static_cast<UINT32>(entry.key.size());
                    }
                    // Immutable key buffers outlive this call through _ledger.
                    // No lock is held while invoking an arbitrary native loader.
                    return original->CreateStreamFromKey(originalKey, originalBytes, value);
                });
            }
        };

        class FileEnumerator final : public IDWriteFontFileEnumerator, public FontComMemory
        {
            SCRIPT_FONT_REFS
            winrt::com_ptr<IDWriteFactory> _factory;
            winrt::com_ptr<IDWriteFontFileLoader> _loader;
            std::shared_ptr<CollectionFiles> _files;
            size_t _position{SIZE_MAX};
          public:
            FileEnumerator(IDWriteFactory *factory, IDWriteFontFileLoader *loader,
                std::shared_ptr<CollectionFiles> files) : _files(std::move(files))
            { _factory.copy_from(factory); _loader.copy_from(loader); }
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWriteFontFileEnumerator)) return E_NOINTERFACE;
                *value = static_cast<IDWriteFontFileEnumerator *>(this); AddRef(); return S_OK;
            }
            HRESULT STDMETHODCALLTYPE MoveNext(BOOL *current) noexcept override
            {
                if (!current) return E_POINTER;
                if (_position == SIZE_MAX) _position = 0;
                else if (_position < _files->files.size()) ++_position;
                *current = _position < _files->files.size(); return S_OK;
            }
            HRESULT STDMETHODCALLTYPE GetCurrentFontFile(IDWriteFontFile **value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = nullptr;
                return FontBoundary([&]() -> HRESULT
                {
                    if (_position >= _files->files.size()) return E_FAIL;
                    FileKey key{FileKeyMagic, _files->files[_position], _files->variant, 0};
                    return _factory->CreateCustomFontFileReference(&key, sizeof(key), _loader.get(), value);
                });
            }
        };

        class CollectionLoader final : public IDWriteFontCollectionLoader, public FontComMemory
        {
            SCRIPT_FONT_REFS
            std::shared_ptr<FileLedger> _ledger;
            winrt::com_ptr<IDWriteFontFileLoader> _loader;
          public:
            CollectionLoader(std::shared_ptr<FileLedger> ledger, IDWriteFontFileLoader *loader)
                : _ledger(std::move(ledger)) { _loader.copy_from(loader); }
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = nullptr;
                if (iid != __uuidof(IUnknown) && iid != __uuidof(IDWriteFontCollectionLoader)) return E_NOINTERFACE;
                *value = static_cast<IDWriteFontCollectionLoader *>(this); AddRef(); return S_OK;
            }
            HRESULT STDMETHODCALLTYPE CreateEnumeratorFromKey(IDWriteFactory *factory, void const *key,
                UINT32 bytes, IDWriteFontFileEnumerator **value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = nullptr;
                return FontBoundary([&]() -> HRESULT
                {
                    if (!factory || !key || bytes != sizeof(CollectionKey)) return E_INVALIDARG;
                    CollectionKey parsed{};
                    std::memcpy(&parsed, key, sizeof(parsed));
                    if (parsed.magic != CollectionKeyMagic) return E_INVALIDARG;
                    std::shared_ptr<CollectionFiles> files;
                    {
                        std::scoped_lock lock{_ledger->mutex};
                        if (parsed.index >= _ledger->collections.size()) return E_INVALIDARG;
                        files = _ledger->collections[static_cast<size_t>(parsed.index)];
                    }
                    *value = new (_ledger->owner) FileEnumerator(factory, _loader.get(), std::move(files));
                    return S_OK;
                });
            }
        };

        struct CloneVariant
        {
            winrt::com_ptr<IDWriteFontFace5> face;
            winrt::com_ptr<IDWriteFontCollection> collection;
            winrt::com_ptr<IDWriteFont> font;
        };
        struct FontRecord
        {
            winrt::com_ptr<IDWriteFontFace5> original;
            PreviewVector<uint64_t> files;
            PreviewVector<DWRITE_FONT_AXIS_VALUE> axes;
            CloneVariant variants[2];
            explicit FontRecord(notepads_markdown_preview_layout *owner) : files(PreviewAllocator<uint64_t>{owner}),
                axes(PreviewAllocator<DWRITE_FONT_AXIS_VALUE>{owner}) {}
        };
        struct TokenRecord
        {
            winrt::com_ptr<IUnknown> original;
            winrt::com_ptr<IDWriteFontCollection> variants[2];
        };
    }

    struct NativeScriptFontState
    {
        notepads_markdown_preview_layout *owner;
        winrt::com_ptr<IDWriteFactory6> factory;
        winrt::com_ptr<IDWriteFontFallback> systemFallback;
        std::shared_ptr<ProviderIdentity> providerIdentity;
        std::shared_ptr<FileLedger> ledger;
        winrt::com_ptr<FileLoader> fileLoader;
        winrt::com_ptr<CollectionLoader> collectionLoader;
        bool fileRegistered{}, collectionRegistered{};
        std::mutex mutex;
        PreviewVector<std::shared_ptr<FontRecord>> records;
        PreviewVector<TokenRecord> tokens;

        NativeScriptFontState(notepads_markdown_preview_layout *value, IDWriteFactory *native) : owner(value),
            providerIdentity(std::allocate_shared<ProviderIdentity>(PreviewAllocator<ProviderIdentity>{value})),
            ledger(std::allocate_shared<FileLedger>(PreviewAllocator<FileLedger>{value}, value)),
            records(PreviewAllocator<std::shared_ptr<FontRecord>>{value}),
            tokens(PreviewAllocator<TokenRecord>{value})
        { factory = Retain(native).as<IDWriteFactory6>(); }

        void Initialize()
        {
            winrt::check_hresult(factory->GetSystemFontFallback(systemFallback.put()));
            fileLoader.attach(new (owner) FileLoader(ledger));
            collectionLoader.attach(new (owner) CollectionLoader(ledger, fileLoader.get()));
            winrt::check_hresult(factory->RegisterFontFileLoader(fileLoader.get()));
            fileRegistered = true;
            winrt::check_hresult(factory->RegisterFontCollectionLoader(collectionLoader.get()));
            collectionRegistered = true;
        }
        ~NativeScriptFontState()
        {
            // The state never owns its fallback or a text layout. Native caches
            // can retain only the independent ledger, never this cache/state.
            records.clear();
            tokens.clear();
            if (collectionRegistered) (void)factory->UnregisterFontCollectionLoader(collectionLoader.get());
            if (fileRegistered) (void)factory->UnregisterFontFileLoader(fileLoader.get());
            collectionLoader = nullptr;
            fileLoader = nullptr;
            systemFallback = nullptr;
            factory = nullptr;
            ledger.reset();
        }
        winrt::com_ptr<IDWriteFontCollection> Token(IDWriteFontCollection *original, UINT32 variant)
        {
            // Bind only fresh layouts. Never wrap our own collection/fallback
            // again, including tokens from another retained rendering epoch.
            if (Retain(original).try_as<ScriptFontToken>())
                winrt::throw_hresult(E_INVALIDARG);
            CheckFontEpoch(original);
            auto identity = Retain(original).as<IUnknown>();
            std::scoped_lock lock{mutex};
            for (auto const &entry : tokens)
                if (entry.original.get() == identity.get()) return entry.variants[variant];
            TokenRecord entry;
            entry.original = std::move(identity);
            for (UINT32 index = 0; index < 2; ++index)
                entry.variants[index].attach(new (owner) CollectionProxy(original, index, providerIdentity));
            tokens.push_back(std::move(entry));
            return tokens.back().variants[variant];
        }
        std::shared_ptr<FontRecord> Record(IDWriteFontFace5 *face)
        {
            for (auto const &entry : records)
                if (entry->original->Equals(face)) return entry;
            auto entry = std::allocate_shared<FontRecord>(PreviewAllocator<FontRecord>{owner}, owner);
            entry->original.copy_from(face);
            auto count = face->GetFontAxisValueCount();
            entry->axes.resize(count);
            if (count) winrt::check_hresult(face->GetFontAxisValues(entry->axes.data(), count));
            UINT32 fileCount{};
            winrt::check_hresult(face->GetFiles(&fileCount, nullptr));
            RequireFonts(fileCount != 0);
            PreviewVector<IDWriteFontFile *> files{PreviewAllocator<IDWriteFontFile *>{owner}};
            files.resize(fileCount, nullptr);
            struct ReleaseFiles
            {
                PreviewVector<IDWriteFontFile *> &files;
                ~ReleaseFiles() { for (auto *file : files) if (file) file->Release(); }
            } release{files};
            auto actual = fileCount;
            winrt::check_hresult(face->GetFiles(&actual, files.data()));
            RequireFonts(actual == fileCount);
            entry->files.reserve(fileCount);
            for (auto *file : files) { RequireFonts(file != nullptr); entry->files.push_back(ledger->Add(file)); }
            records.push_back(entry);
            return entry;
        }
        void Complete(std::shared_ptr<FontRecord> const &entry, UINT32 variant, bool needFont)
        {
            auto &clone = entry->variants[variant];
            if (!clone.face)
            {
                PreviewVector<winrt::com_ptr<IDWriteFontFile>> files{PreviewAllocator<winrt::com_ptr<IDWriteFontFile>>{owner}};
                files.resize(entry->files.size());
                for (size_t index = 0; index < files.size(); ++index)
                {
                    FileKey key{FileKeyMagic, entry->files[index], variant, 0};
                    winrt::check_hresult(factory->CreateCustomFontFileReference(&key, sizeof(key), fileLoader.get(), files[index].put()));
                }
                if (files.size() == 1)
                {
                    winrt::com_ptr<IDWriteFontFaceReference1> reference;
                    winrt::check_hresult(factory->CreateFontFaceReference(files[0].get(), entry->original->GetIndex(),
                        entry->original->GetSimulations(), entry->axes.data(), static_cast<UINT32>(entry->axes.size()), reference.put()));
                    winrt::check_hresult(reference->CreateFontFace(clone.face.put()));
                }
                else
                {
                    PreviewVector<IDWriteFontFile *> raw{PreviewAllocator<IDWriteFontFile *>{owner}};
                    raw.reserve(files.size());
                    for (auto const &file : files) raw.push_back(file.get());
                    winrt::com_ptr<IDWriteFontFace> base;
                    winrt::check_hresult(factory->CreateFontFace(entry->original->GetType(), static_cast<UINT32>(raw.size()),
                        raw.data(), entry->original->GetIndex(), entry->original->GetSimulations(), base.put()));
                    clone.face = base.as<IDWriteFontFace5>();
                }
                RequireFonts(clone.face->GetType() == entry->original->GetType() &&
                    clone.face->GetIndex() == entry->original->GetIndex() &&
                    clone.face->GetSimulations() == entry->original->GetSimulations() &&
                    clone.face->GetFontAxisValueCount() == entry->axes.size());
                PreviewVector<DWRITE_FONT_AXIS_VALUE> actual{PreviewAllocator<DWRITE_FONT_AXIS_VALUE>{owner}};
                actual.resize(entry->axes.size());
                if (!actual.empty()) winrt::check_hresult(clone.face->GetFontAxisValues(actual.data(), static_cast<UINT32>(actual.size())));
                for (size_t index = 0; index < actual.size(); ++index)
                    RequireFonts(actual[index].axisTag == entry->axes[index].axisTag && actual[index].value == entry->axes[index].value);
            }
            if (needFont && !clone.font)
            {
                auto id = ledger->AddCollection(entry->files, variant);
                CollectionKey key{CollectionKeyMagic, id};
                winrt::check_hresult(factory->CreateCustomFontCollection(collectionLoader.get(), &key, sizeof(key), clone.collection.put()));
                winrt::check_hresult(clone.collection->GetFontFromFontFace(clone.face.get(), clone.font.put()));
                winrt::com_ptr<IDWriteFontFace> actual;
                winrt::check_hresult(clone.font->CreateFontFace(actual.put()));
                RequireFonts(clone.face->Equals(actual.get()));
            }
        }
    };

    namespace
    {
        struct SourceProperty
        {
            UINT32 end{};
            PreviewString locale;
            bool nullLocale{};
            winrt::com_ptr<IUnknown> number;
            explicit SourceProperty(notepads_markdown_preview_layout *owner) : locale(PreviewAllocator<wchar_t>{owner}) {}
        };
        struct SourceFingerprint
        {
            uint64_t hash{1469598103934665603ull};
            UINT32 length{};
            DWRITE_READING_DIRECTION direction{};
        };
        void HashBytes(SourceFingerprint &fingerprint, void const *bytes, size_t length) noexcept
        {
            auto *data = static_cast<uint8_t const *>(bytes);
            for (size_t index = 0; index < length; ++index)
            { fingerprint.hash ^= data[index]; fingerprint.hash *= 1099511628211ull; }
        }
        SourceFingerprint Fingerprint(IDWriteTextAnalysisSource *source)
        {
            RequireFonts(source != nullptr);
            SourceFingerprint result;
            result.direction = source->GetParagraphReadingDirection();
            HashBytes(result, &result.direction, sizeof(result.direction));
            for (;;)
            {
                WCHAR const *text{}; UINT32 count{};
                winrt::check_hresult(source->GetTextAtPosition(result.length, &text, &count));
                if (!text || !count) break;
                RequireFonts(count <= UINT32_MAX - result.length);
                HashBytes(result, text, static_cast<size_t>(count) * sizeof(WCHAR));
                result.length += count;
            }
            for (UINT32 position = 0; position < result.length;)
            {
                UINT32 localeCount{}, numberCount{};
                WCHAR const *locale{};
                winrt::com_ptr<IDWriteNumberSubstitution> number;
                winrt::check_hresult(source->GetLocaleName(position, &localeCount, &locale));
                winrt::check_hresult(source->GetNumberSubstitution(position, &numberCount, number.put()));
                RequireFonts(localeCount && numberCount);
                auto count = std::min({localeCount, numberCount, result.length - position});
                auto identity = number ? number.as<IUnknown>() : winrt::com_ptr<IUnknown>{};
                auto address = reinterpret_cast<uintptr_t>(identity.get());
                bool present = locale != nullptr;
                HashBytes(result, &count, sizeof(count));
                HashBytes(result, &present, sizeof(present));
                if (locale) HashBytes(result, locale, (std::wcslen(locale) + 1) * sizeof(WCHAR));
                HashBytes(result, &address, sizeof(address));
                position += count;
            }
            return result;
        }
        struct SourceSnapshot
        {
            SourceFingerprint fingerprint;
            PreviewString text;
            PreviewVector<SourceProperty> properties;
            explicit SourceSnapshot(notepads_markdown_preview_layout *owner) : text(PreviewAllocator<wchar_t>{owner}),
                properties(PreviewAllocator<SourceProperty>{owner}) {}
            bool Matches(IDWriteTextAnalysisSource *source) const
            {
                if (source->GetParagraphReadingDirection() != fingerprint.direction) return false;
                UINT32 position{};
                while (position < text.size())
                {
                    WCHAR const *value{}; UINT32 count{};
                    winrt::check_hresult(source->GetTextAtPosition(position, &value, &count));
                    if (!value || !count || count > text.size() - position ||
                        std::memcmp(value, text.data() + position, static_cast<size_t>(count) * sizeof(WCHAR))) return false;
                    position += count;
                }
                WCHAR const *tail{}; UINT32 remaining{};
                winrt::check_hresult(source->GetTextAtPosition(position, &tail, &remaining));
                if (tail && remaining) return false;
                position = 0;
                for (auto const &property : properties)
                {
                    WCHAR const *locale{}; UINT32 localeCount{}, numberCount{};
                    winrt::com_ptr<IDWriteNumberSubstitution> number;
                    winrt::check_hresult(source->GetLocaleName(position, &localeCount, &locale));
                    winrt::check_hresult(source->GetNumberSubstitution(position, &numberCount, number.put()));
                    if (!localeCount || !numberCount ||
                        std::min({localeCount, numberCount, fingerprint.length - position}) != property.end - position ||
                        (locale == nullptr) != property.nullLocale) return false;
                    if (locale && property.locale != locale) return false;
                    auto identity = number ? number.as<IUnknown>() : winrt::com_ptr<IUnknown>{};
                    if (identity.get() != property.number.get()) return false;
                    position = property.end;
                }
                return position == fingerprint.length;
            }
            void Capture(notepads_markdown_preview_layout *owner, IDWriteTextAnalysisSource *source, SourceFingerprint value)
            {
                fingerprint = value;
                text.reserve(value.length);
                for (UINT32 position = 0; position < value.length;)
                {
                    WCHAR const *chunk{}; UINT32 count{};
                    winrt::check_hresult(source->GetTextAtPosition(position, &chunk, &count));
                    RequireFonts(chunk && count && count <= value.length - position);
                    text.append(chunk, count); position += count;
                }
                for (UINT32 position = 0; position < value.length;)
                {
                    SourceProperty property{owner};
                    WCHAR const *locale{}; UINT32 localeCount{}, numberCount{};
                    winrt::com_ptr<IDWriteNumberSubstitution> number;
                    winrt::check_hresult(source->GetLocaleName(position, &localeCount, &locale));
                    winrt::check_hresult(source->GetNumberSubstitution(position, &numberCount, number.put()));
                    RequireFonts(localeCount && numberCount);
                    property.end = position + std::min({localeCount, numberCount, value.length - position});
                    property.nullLocale = locale == nullptr;
                    if (locale) property.locale.assign(locale);
                    // Native number-substitution objects are standalone value
                    // objects, never retain this analysis source or a layout.
                    if (number) property.number = number.as<IUnknown>();
                    position = property.end;
                    properties.push_back(std::move(property));
                }
                RequireFonts(Matches(source));
            }
        };
        struct MappingRequest
        {
            std::shared_ptr<SourceSnapshot> source;
            winrt::com_ptr<IUnknown> collection;
            PreviewString family;
            PreviewVector<DWRITE_FONT_AXIS_VALUE> axes;
            bool nullFamily{}, axisMode{};
            UINT32 position{}, length{}, variant{}, mappedLength{};
            DWRITE_FONT_WEIGHT weight{};
            DWRITE_FONT_STYLE style{};
            DWRITE_FONT_STRETCH stretch{};
            FLOAT scale{1};
            std::shared_ptr<FontRecord> record;
            explicit MappingRequest(notepads_markdown_preview_layout *owner) : family(PreviewAllocator<wchar_t>{owner}),
                axes(PreviewAllocator<DWRITE_FONT_AXIS_VALUE>{owner}) {}
        };

        template <typename T> int Ordered(T const &left, T const &right) noexcept
        {
            return left < right ? -1 : right < left ? 1 : 0;
        }
        template <typename T> int OrderedPointer(T *left, T *right) noexcept
        {
            auto less = std::less<T *>{};
            return less(left, right) ? -1 : less(right, left) ? 1 : 0;
        }
        int OrderedFingerprint(SourceFingerprint const &left, SourceFingerprint const &right) noexcept
        {
            if (auto order = Ordered(left.hash, right.hash)) return order;
            if (auto order = Ordered(left.length, right.length)) return order;
            return Ordered(left.direction, right.direction);
        }
        int OrderedProperties(SourceProperty const &left, SourceProperty const &right)
        {
            if (auto order = Ordered(left.end, right.end)) return order;
            if (auto order = Ordered(left.nullLocale, right.nullLocale)) return order;
            if (auto order = left.locale.compare(right.locale)) return order;
            return OrderedPointer(left.number.get(), right.number.get());
        }
        int OrderedSource(SourceSnapshot const &left, SourceSnapshot const &right)
        {
            if (auto order = OrderedFingerprint(left.fingerprint, right.fingerprint)) return order;
            if (auto order = left.text.compare(right.text)) return order;
            auto count = std::min(left.properties.size(), right.properties.size());
            for (size_t index = 0; index < count; ++index)
                if (auto order = OrderedProperties(left.properties[index], right.properties[index])) return order;
            return Ordered(left.properties.size(), right.properties.size());
        }
        // Full collision ordering is independent of the source's text chunking.
        // Equal hashes are never accepted without text/locale/number comparison.
        int OrderedSource(SourceSnapshot const &saved, IDWriteTextAnalysisSource *source)
        {
            UINT32 position{};
            while (position < saved.text.size())
            {
                WCHAR const *chunk{}; UINT32 count{};
                winrt::check_hresult(source->GetTextAtPosition(position, &chunk, &count));
                RequireFonts(chunk && count && count <= saved.text.size() - position);
                if (auto order = std::char_traits<WCHAR>::compare(saved.text.data() + position, chunk, count)) return order;
                position += count;
            }
            WCHAR const *tail{}; UINT32 remaining{};
            winrt::check_hresult(source->GetTextAtPosition(position, &tail, &remaining));
            RequireFonts(!tail || !remaining);
            position = 0;
            for (auto const &property : saved.properties)
            {
                UINT32 localeCount{}, numberCount{}; WCHAR const *locale{};
                winrt::com_ptr<IDWriteNumberSubstitution> number;
                winrt::check_hresult(source->GetLocaleName(position, &localeCount, &locale));
                winrt::check_hresult(source->GetNumberSubstitution(position, &numberCount, number.put()));
                RequireFonts(localeCount && numberCount);
                auto end = position + std::min({localeCount, numberCount, saved.fingerprint.length - position});
                if (auto order = Ordered(property.end, end)) return order;
                if (auto order = Ordered(property.nullLocale, locale == nullptr)) return order;
                if (locale)
                    if (auto order = std::wstring_view{property.locale.data(), property.locale.size()}.compare(locale)) return order;
                auto identity = number ? number.as<IUnknown>() : winrt::com_ptr<IUnknown>{};
                if (auto order = OrderedPointer(property.number.get(), identity.get())) return order;
                position = end;
            }
            RequireFonts(position == saved.fingerprint.length);
            return 0;
        }
        struct SourceIndexKey { SourceSnapshot const *snapshot; };
        struct SourceLookup { SourceFingerprint fingerprint; IDWriteTextAnalysisSource *source; };
        struct SourceLess
        {
            using is_transparent = void;
            bool operator()(SourceIndexKey left, SourceIndexKey right) const
            { return OrderedSource(*left.snapshot, *right.snapshot) < 0; }
            bool operator()(SourceIndexKey saved, SourceLookup lookup) const
            {
                auto order = OrderedFingerprint(saved.snapshot->fingerprint, lookup.fingerprint);
                return order ? order < 0 : OrderedSource(*saved.snapshot, lookup.source) < 0;
            }
            bool operator()(SourceLookup lookup, SourceIndexKey saved) const
            {
                auto order = OrderedFingerprint(lookup.fingerprint, saved.snapshot->fingerprint);
                return order ? order < 0 : OrderedSource(*saved.snapshot, lookup.source) > 0;
            }
        };
        struct RequestLookup
        {
            SourceSnapshot const *source;
            IUnknown *collection;
            UINT32 variant, position, length;
            bool axisMode, nullFamily;
            std::wstring_view family;
            DWRITE_FONT_WEIGHT weight;
            DWRITE_FONT_STYLE style;
            DWRITE_FONT_STRETCH stretch;
            std::span<DWRITE_FONT_AXIS_VALUE const> axes;
        };
        RequestLookup RequestView(MappingRequest const &entry) noexcept
        {
            return {entry.source.get(), entry.collection.get(), entry.variant, entry.position, entry.length,
                entry.axisMode, entry.nullFamily, {entry.family.data(), entry.family.size()}, entry.weight, entry.style,
                entry.stretch, entry.axes};
        }
        int OrderedRequest(RequestLookup const &left, RequestLookup const &right)
        {
            if (auto order = OrderedPointer(left.source, right.source)) return order;
            if (auto order = OrderedPointer(left.collection, right.collection)) return order;
            if (auto order = Ordered(left.variant, right.variant)) return order;
            if (auto order = Ordered(left.position, right.position)) return order;
            if (auto order = Ordered(left.length, right.length)) return order;
            if (auto order = Ordered(left.axisMode, right.axisMode)) return order;
            if (auto order = Ordered(left.nullFamily, right.nullFamily)) return order;
            if (auto order = left.family.compare(right.family)) return order;
            if (left.axisMode)
            {
                if (auto order = Ordered(left.axes.size(), right.axes.size())) return order;
                return left.axes.empty() ? 0 : std::memcmp(left.axes.data(), right.axes.data(), left.axes.size_bytes());
            }
            if (auto order = Ordered(left.weight, right.weight)) return order;
            if (auto order = Ordered(left.style, right.style)) return order;
            return Ordered(left.stretch, right.stretch);
        }
        struct RequestLess
        {
            using is_transparent = void;
            PreviewVector<MappingRequest> const *rows;
            bool operator()(size_t left, size_t right) const
            { return OrderedRequest(RequestView((*rows)[left]), RequestView((*rows)[right])) < 0; }
            bool operator()(size_t saved, RequestLookup const &lookup) const
            { return OrderedRequest(RequestView((*rows)[saved]), lookup) < 0; }
            bool operator()(RequestLookup const &lookup, size_t saved) const
            { return OrderedRequest(lookup, RequestView((*rows)[saved])) < 0; }
        };
        using SourceIndexPair = std::pair<SourceIndexKey const, size_t>;
        using SourceIndex = std::map<SourceIndexKey, size_t, SourceLess, PreviewAllocator<SourceIndexPair>>;
        using RequestIndexPair = std::pair<size_t const, size_t>;
        using RequestIndex = std::map<size_t, size_t, RequestLess, PreviewAllocator<RequestIndexPair>>;
    }

    struct NativeScriptFontBindingState
    {
        std::shared_ptr<NativeScriptFontState> fonts;
        winrt::com_ptr<IDWriteFontFallback> original;
        winrt::com_ptr<IDWriteFontFallback1> one;
        std::mutex mutex;
        PreviewVector<std::shared_ptr<SourceSnapshot>> sources;
        PreviewVector<MappingRequest> requests;
        SourceIndex sourceIndex;
        RequestIndex requestIndex;
        PreviewVector<winrt::com_ptr<IDWriteFontCollection>> epochCollections;
        std::atomic<bool> frozen{};
        std::atomic<HRESULT> failure{S_OK};
        explicit NativeScriptFontBindingState(std::shared_ptr<NativeScriptFontState> value) : fonts(std::move(value)),
            sources(PreviewAllocator<std::shared_ptr<SourceSnapshot>>{fonts->owner}),
            requests(PreviewAllocator<MappingRequest>{fonts->owner}),
            sourceIndex(SourceLess{}, PreviewAllocator<SourceIndexPair>{fonts->owner}),
            requestIndex(RequestLess{&requests}, PreviewAllocator<RequestIndexPair>{fonts->owner}),
            epochCollections(PreviewAllocator<winrt::com_ptr<IDWriteFontCollection>>{fonts->owner}) {}

        UINT32 OriginalCollection(IDWriteFontCollection *token, winrt::com_ptr<IDWriteFontCollection> &collection)
        {
            if (!token) winrt::throw_hresult(E_INVALIDARG);
            auto marker = Retain(token).as<ScriptFontToken>();
            UINT32 variant{};
            void const *provider{};
            winrt::check_hresult(marker->Original(collection.put(), &variant, &provider));
            RequireFonts(variant < 2 && provider == fonts->providerIdentity.get() && collection);
            if (!frozen.load())
                CheckFontEpoch(collection.get());
            return variant;
        }
        std::shared_ptr<SourceSnapshot> Snapshot(IDWriteTextAnalysisSource *source, SourceFingerprint value)
        {
            auto found = sourceIndex.find(SourceLookup{value, source});
            if (found != sourceIndex.end())
                return sources[found->second];
            if (frozen.load()) winrt::throw_hresult(E_UNEXPECTED);
            auto snapshot = std::allocate_shared<SourceSnapshot>(PreviewAllocator<SourceSnapshot>{fonts->owner}, fonts->owner);
            snapshot->Capture(fonts->owner, source, value);
            sources.push_back(snapshot);
            // Publish only an index of an already successful vector append.
            // Insertion failure latches the callback error; cleanup stays valid.
            auto inserted = sourceIndex.emplace(SourceIndexKey{snapshot.get()}, sources.size() - 1);
            RequireFonts(inserted.second);
            return snapshot;
        }
        std::shared_ptr<FontRecord> Map(IDWriteFontFace5 *face, UINT32 variant, bool nominal)
        {
            std::scoped_lock resourceLock{fonts->mutex};
            auto record = fonts->Record(face);
            fonts->Complete(record, variant, nominal);
            return record;
        }
        MappingRequest *Find(std::shared_ptr<SourceSnapshot> const &source, IUnknown *collection, UINT32 variant,
            UINT32 position, UINT32 length, WCHAR const *family, bool axisMode, DWRITE_FONT_WEIGHT weight,
            DWRITE_FONT_STYLE style, DWRITE_FONT_STRETCH stretch, DWRITE_FONT_AXIS_VALUE const *axes, UINT32 axisCount)
        {
            RequestLookup lookup{source.get(), collection, variant, position, length, axisMode, family == nullptr,
                family ? std::wstring_view{family} : std::wstring_view{}, weight, style, stretch, {axes, axisCount}};
            auto found = requestIndex.find(lookup);
            return found == requestIndex.end() ? nullptr : &requests[found->second];
        }
        void Save(MappingRequest request)
        {
            auto index = requests.size();
            requests.push_back(std::move(request));
            auto inserted = requestIndex.emplace(index, index);
            RequireFonts(inserted.second);
        }
        MappingRequest NewRequest(std::shared_ptr<SourceSnapshot> source, winrt::com_ptr<IUnknown> collection,
            UINT32 variant, UINT32 position, UINT32 length, WCHAR const *family, bool axisMode)
        {
            if (frozen.load()) winrt::throw_hresult(E_UNEXPECTED);
            MappingRequest entry{fonts->owner};
            entry.source = std::move(source); entry.collection = std::move(collection);
            entry.variant = variant; entry.position = position; entry.length = length; entry.axisMode = axisMode;
            entry.nullFamily = family == nullptr;
            if (family) entry.family.assign(family);
            return entry;
        }
        HRESULT Remember(HRESULT status) noexcept
        {
            if (FAILED(status)) { HRESULT expected = S_OK; failure.compare_exchange_strong(expected, status); }
            return status;
        }
    };

    namespace
    {
        class Fallback final : public IDWriteFontFallback1, public FontComMemory
        {
            SCRIPT_FONT_REFS
            std::shared_ptr<NativeScriptFontBindingState> _state;
          public:
            explicit Fallback(std::shared_ptr<NativeScriptFontBindingState> state) : _state(std::move(state)) {}
            HRESULT STDMETHODCALLTYPE QueryInterface(REFIID iid, void **value) noexcept override
            {
                if (!value) return E_POINTER;
                *value = nullptr;
                if (iid == __uuidof(IUnknown) || iid == __uuidof(IDWriteFontFallback))
                    *value = static_cast<IDWriteFontFallback *>(this);
                else if (iid == __uuidof(IDWriteFontFallback1) && _state->one)
                    *value = static_cast<IDWriteFontFallback1 *>(this);
                else return E_NOINTERFACE;
                AddRef(); return S_OK;
            }
            HRESULT STDMETHODCALLTYPE MapCharacters(IDWriteTextAnalysisSource *source, UINT32 position, UINT32 length,
                IDWriteFontCollection *collection, WCHAR const *family, DWRITE_FONT_WEIGHT weight,
                DWRITE_FONT_STYLE style, DWRITE_FONT_STRETCH stretch, UINT32 *mappedLength,
                IDWriteFont **mappedFont, FLOAT *scale) noexcept override
            {
                if (!mappedLength || !mappedFont || !scale) return E_POINTER;
                *mappedLength = 0; *mappedFont = nullptr; *scale = 1;
                return _state->Remember(FontBoundary([&]() -> HRESULT
                {
                    std::scoped_lock lock{_state->mutex};
                    winrt::com_ptr<IDWriteFontCollection> originalCollection;
                    auto variant = _state->OriginalCollection(collection, originalCollection);
                    auto identity = originalCollection.as<IUnknown>();
                    auto snapshot = _state->Snapshot(source, Fingerprint(source));
                    if (auto *cached = _state->Find(snapshot, identity.get(), variant, position, length, family, false,
                        weight, style, stretch, nullptr, 0))
                    {
                        *mappedLength = cached->mappedLength; *scale = cached->scale;
                        if (cached->record) cached->record->variants[variant].font.copy_to(mappedFont);
                        return S_OK;
                    }
                    auto request = _state->NewRequest(std::move(snapshot), std::move(identity), variant, position, length, family, false);
                    request.weight = weight; request.style = style; request.stretch = stretch;
                    winrt::com_ptr<IDWriteFont> original;
                    winrt::check_hresult(_state->original->MapCharacters(source, position, length, originalCollection.get(),
                        family, weight, style, stretch, &request.mappedLength, original.put(), &request.scale));
                    RequireFonts(request.mappedLength <= length && (!length || request.mappedLength));
                    if (original)
                    {
                        winrt::com_ptr<IDWriteFontFace> face;
                        winrt::check_hresult(original->CreateFontFace(face.put()));
                        auto actual = face.as<IDWriteFontFace5>();
                        request.record = _state->Map(actual.get(), variant, true);
                    }
                    _state->Save(std::move(request));
                    auto const &saved = _state->requests.back();
                    *mappedLength = saved.mappedLength; *scale = saved.scale;
                    if (saved.record) saved.record->variants[variant].font.copy_to(mappedFont);
                    return S_OK;
                }));
            }
            HRESULT STDMETHODCALLTYPE MapCharacters(IDWriteTextAnalysisSource *source, UINT32 position, UINT32 length,
                IDWriteFontCollection *collection, WCHAR const *family, DWRITE_FONT_AXIS_VALUE const *axes,
                UINT32 axisCount, UINT32 *mappedLength, FLOAT *scale, IDWriteFontFace5 **mappedFace) noexcept override
            {
                if (!mappedLength || !mappedFace || !scale) return E_POINTER;
                *mappedLength = 0; *mappedFace = nullptr; *scale = 1;
                return _state->Remember(FontBoundary([&]() -> HRESULT
                {
                    if (!_state->one) return E_NOINTERFACE;
                    if (axisCount && !axes) return E_INVALIDARG;
                    std::scoped_lock lock{_state->mutex};
                    winrt::com_ptr<IDWriteFontCollection> originalCollection;
                    auto variant = _state->OriginalCollection(collection, originalCollection);
                    auto identity = originalCollection.as<IUnknown>();
                    auto snapshot = _state->Snapshot(source, Fingerprint(source));
                    if (auto *cached = _state->Find(snapshot, identity.get(), variant, position, length, family, true,
                        {}, {}, {}, axes, axisCount))
                    {
                        *mappedLength = cached->mappedLength; *scale = cached->scale;
                        if (cached->record) cached->record->variants[variant].face.copy_to(mappedFace);
                        return S_OK;
                    }
                    auto request = _state->NewRequest(std::move(snapshot), std::move(identity), variant, position, length, family, true);
                    if (axisCount) request.axes.assign(axes, axes + axisCount);
                    winrt::com_ptr<IDWriteFontFace5> original;
                    winrt::check_hresult(_state->one->MapCharacters(source, position, length, originalCollection.get(), family,
                        axes, axisCount, &request.mappedLength, &request.scale, original.put()));
                    RequireFonts(request.mappedLength <= length && (!length || request.mappedLength));
                    if (original) request.record = _state->Map(original.get(), variant, false);
                    _state->Save(std::move(request));
                    auto const &saved = _state->requests.back();
                    *mappedLength = saved.mappedLength; *scale = saved.scale;
                    if (saved.record) saved.record->variants[variant].face.copy_to(mappedFace);
                    return S_OK;
                }));
            }
        };
    }

    std::shared_ptr<NativeScriptFonts> NativeScriptFonts::Create(notepads_markdown_preview_layout *owner, IDWriteFactory *factory)
    {
        if (!owner || !factory) throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT, L"script-fonts"};
        return std::allocate_shared<NativeScriptFonts>(PreviewAllocator<NativeScriptFonts>{owner}, ConstructionKey{}, owner, factory);
    }
    NativeScriptFonts::NativeScriptFonts(ConstructionKey, notepads_markdown_preview_layout *owner, IDWriteFactory *factory)
        : _state(std::allocate_shared<NativeScriptFontState>(PreviewAllocator<NativeScriptFontState>{owner}, owner, factory))
    { _state->Initialize(); }
    NativeScriptFonts::~NativeScriptFonts() = default;

    std::shared_ptr<NativeScriptFontBinding> NativeScriptFonts::Bind(IDWriteTextLayout *layout,
        std::span<notepads_markdown_preview_run const> runs, uint32_t length, std::span<uint32_t const> barriers)
    {
        if (!layout) throw PreviewFailure{NOTEPADS_MARKDOWN_CORE_INVALID_ARGUMENT, L"script-fonts"};
        auto state = std::allocate_shared<NativeScriptFontBindingState>(PreviewAllocator<NativeScriptFontBindingState>{_state->owner}, _state);
        auto layout2 = Retain(layout).as<IDWriteTextLayout2>();
        winrt::check_hresult(layout2->GetFontFallback(state->original.put()));
        if (!state->original) state->original = _state->systemFallback;
        state->one = state->original.try_as<IDWriteFontFallback1>();
        struct Assignment { DWRITE_TEXT_RANGE range; winrt::com_ptr<IDWriteFontCollection> collection; };
        PreviewVector<Assignment> assignments{PreviewAllocator<Assignment>{_state->owner}};
        auto append = [&](uint32_t start, uint32_t end, uint32_t variant)
        {
            while (start < end)
            {
                winrt::com_ptr<IDWriteFontCollection> original;
                DWRITE_TEXT_RANGE property{};
                winrt::check_hresult(layout->GetFontCollection(start, original.put(), &property));
                RequireFonts(original && property.startPosition <= start);
                bool remembered = false;
                for (auto const &entry : state->epochCollections)
                    remembered |= entry.get() == original.get();
                if (!remembered)
                    state->epochCollections.push_back(original);
                auto finish = static_cast<uint32_t>(std::min(uint64_t{end}, uint64_t{property.startPosition} + property.length));
                RequireFonts(finish > start);
                assignments.push_back({{start, finish - start}, _state->Token(original.get(), variant)});
                start = finish;
            }
        };
        uint32_t previousBarrier{};
        for (auto boundary : barriers)
        {
            RequireFonts(boundary > previousBarrier && boundary < length);
            previousBarrier = boundary;
        }
        uint32_t position{}, variant{}, lastToggle{};
        size_t nextBarrier{};
        auto toggle = [&](uint32_t boundary)
        {
            if (boundary != lastToggle) { variant ^= 1; lastToggle = boundary; }
        };
        auto segment = [&](uint32_t start, uint32_t end)
        {
            while (nextBarrier < barriers.size() && barriers[nextBarrier] < end)
            {
                auto boundary = barriers[nextBarrier++];
                RequireFonts(boundary >= start);
                append(start, boundary, variant);
                toggle(boundary);
                start = boundary;
            }
            append(start, end, variant);
        };
        double size = 1, em = 0, dip = 0;
        bool first = true;
        if (runs.empty()) segment(0, length);
        for (auto const &run : runs)
        {
            RequireFonts(run.start == position && run.start < run.end && run.end <= length &&
                std::isfinite(run.size_scale) && run.size_scale > 0 && std::isfinite(run.relative_em) && std::isfinite(run.absolute_dip));
            if (!first && (size != run.size_scale || em != run.relative_em || dip != run.absolute_dip)) toggle(run.start);
            segment(run.start, run.end);
            size = run.size_scale; em = run.relative_em; dip = run.absolute_dip;
            position = run.end; first = false;
        }
        RequireFonts(runs.empty() || position == length);
        winrt::com_ptr<Fallback> fallback;
        fallback.attach(new (_state->owner) Fallback(state));
        for (auto const &assignment : assignments)
            winrt::check_hresult(layout->SetFontCollection(assignment.collection.get(), assignment.range));
        winrt::check_hresult(layout2->SetFontFallback(fallback.get()));
        return std::allocate_shared<NativeScriptFontBinding>(PreviewAllocator<NativeScriptFontBinding>{_state->owner},
            NativeScriptFontBinding::ConstructionKey{}, std::move(state));
    }

    NativeScriptFontBinding::NativeScriptFontBinding(ConstructionKey, std::shared_ptr<NativeScriptFontBindingState> state)
        : _state(std::move(state)) {}
    NativeScriptFontBinding::~NativeScriptFontBinding() = default;
    void NativeScriptFontBinding::Freeze()
    {
        std::scoped_lock lock{_state->mutex};
        winrt::check_hresult(_state->failure.load());
        if (!_state->frozen.load())
            for (auto const &collection : _state->epochCollections)
                CheckFontEpoch(collection.get());
        _state->frozen.store(true);
    }
    bool NativeScriptFontBinding::Frozen() const noexcept { return _state->frozen.load(); }
#undef SCRIPT_FONT_REFS
}
