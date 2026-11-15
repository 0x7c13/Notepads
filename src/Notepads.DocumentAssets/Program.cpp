// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

#include <windows.h>
#include <winternl.h>
#include <appmodel.h>
#include <shellapi.h>
#include <winrt/Windows.ApplicationModel.DataTransfer.h>
#include <winrt/Windows.Data.Json.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Storage.h>
#include <algorithm>
#include <array>
#include <filesystem>
#include <fstream>
#include <limits>
#include <optional>
#include <string>
#include <string_view>

namespace
{
    namespace fs = std::filesystem;
    using winrt::Windows::ApplicationModel::DataTransfer::SharedStorageAccessManager;
    using winrt::Windows::Data::Json::JsonObject;
    using winrt::Windows::Storage::ApplicationData;

    constexpr auto Protocol = L"Files.ReadDocumentAsset\n";
    constexpr uint64_t MaximumImageBytes = 16ull * 1024 * 1024;
    constexpr DWORD IdleMilliseconds = 30000;

    bool IsId(std::wstring_view value)
    {
        return value.size() == 32 && std::ranges::all_of(value, [](wchar_t ch)
        {
            return (ch >= L'0' && ch <= L'9') || (ch >= L'a' && ch <= L'f');
        });
    }

    bool IsDirectory(fs::path const& path)
    {
        auto attributes = GetFileAttributesW(path.c_str());
        return attributes != INVALID_FILE_ATTRIBUTES && (attributes & FILE_ATTRIBUTE_DIRECTORY) &&
            !(attributes & FILE_ATTRIBUTE_REPARSE_POINT);
    }

    winrt::handle OwnerProcess(wchar_t const* text)
    {
        wchar_t* end{};
        auto pid = wcstoul(text, &end, 10);
        if (text == end || *end || !pid)
            return {};
        winrt::handle owner{OpenProcess(SYNCHRONIZE | PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid)};
        if (!owner)
            return {};
        std::array<wchar_t, PACKAGE_FAMILY_NAME_MAX_LENGTH + 1> actual{}, expected{};
        auto actualLength = static_cast<UINT32>(actual.size());
        auto expectedLength = static_cast<UINT32>(expected.size());
        if (GetPackageFamilyName(owner.get(), &actualLength, actual.data()) != ERROR_SUCCESS ||
            GetCurrentPackageFamilyName(&expectedLength, expected.data()) != ERROR_SUCCESS ||
            std::wstring_view{actual.data()} != expected.data())
            return {};
        return owner;
    }

    bool IsAbsoluteDocument(std::wstring_view path)
    {
        if (path.starts_with(LR"(\\?\)") || path.starts_with(LR"(\\.\)"))
            return false;
        if (path.starts_with(LR"(\\)"))
            return path.find(L':') == path.npos;
        return path.size() > 3 && path[1] == L':' && path[2] == L'\\' &&
            path.find(L':', 2) == path.npos;
    }

    bool IsChildPath(std::wstring_view path)
    {
        if (path.empty() || path.find_first_of(L":/") != path.npos)
            return false;
        for (;;)
        {
            auto separator = path.find(L'\\');
            auto part = path.substr(0, separator);
            if (part.empty() || part == L"." || part == L".." ||
                part.ends_with(L'.') || part.ends_with(L' '))
                return false;
            if (separator == path.npos)
                return true;
            path.remove_prefix(separator + 1);
        }
    }

    std::wstring FinalPath(HANDLE file)
    {
        auto size = GetFinalPathNameByHandleW(file, nullptr, 0, FILE_NAME_NORMALIZED | VOLUME_NAME_DOS);
        if (!size)
            return {};
        std::wstring result(size, L'\0');
        auto written = GetFinalPathNameByHandleW(file, result.data(), size, FILE_NAME_NORMALIZED | VOLUME_NAME_DOS);
        if (!written || written >= size)
            return {};
        result.resize(written);
        return result;
    }

    winrt::file_handle OpenPath(fs::path const& path, DWORD access, DWORD flags)
    {
        // Explicit extended paths support long names and disable DOS device alias parsing.
        auto name = path.native();
        name = name.starts_with(LR"(\\)") ? LR"(\\?\UNC\)" + name.substr(2) : LR"(\\?\)" + name;
        return winrt::file_handle{CreateFileW(name.c_str(), access,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr, OPEN_EXISTING, flags, nullptr)};
    }

    winrt::file_handle OpenRelativeAsset(HANDLE root, std::wstring_view child)
    {
        // NT names use byte-sized USHORT lengths. Every open below receives one
        // relative component and the already authenticated parent handle.
        if (!root || !IsChildPath(child) || child.find(L'\0') != child.npos || child.size() > (std::numeric_limits<USHORT>::max)() / sizeof(wchar_t))
            return {};
        winrt::file_handle directory;
        auto parent = root;
        for (;;)
        {
            auto separator = child.find(L'\\');
            auto component = child.substr(0, separator);
            bool final = separator == child.npos;
            auto bytes = static_cast<USHORT>(component.size() * sizeof(wchar_t));
            UNICODE_STRING name{bytes, bytes, const_cast<PWSTR>(component.data())};
            OBJECT_ATTRIBUTES attributes{};
            attributes.Length = sizeof(attributes);
            attributes.RootDirectory = parent;
            attributes.ObjectName = &name;
            attributes.Attributes = OBJ_CASE_INSENSITIVE;
            IO_STATUS_BLOCK status{};
            HANDLE raw{};
            auto access = final ? GENERIC_READ : FILE_READ_ATTRIBUTES | FILE_TRAVERSE;
            auto options = FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT |
                (final ? FILE_NON_DIRECTORY_FILE | FILE_SEQUENTIAL_ONLY : 0);
            auto result = NtCreateFile(&raw, access | SYNCHRONIZE, &attributes, &status, nullptr,
                FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ | FILE_SHARE_DELETE | (final ? FILE_SHARE_WRITE : 0),
                FILE_OPEN, options, nullptr, 0);
            if (result < 0 || !raw || raw == INVALID_HANDLE_VALUE)
                return {};
            winrt::file_handle opened{raw};
            FILE_ATTRIBUTE_TAG_INFO info{};
            if (GetFileType(opened.get()) != FILE_TYPE_DISK ||
                !GetFileInformationByHandleEx(opened.get(), FileAttributeTagInfo, &info, sizeof(info)) ||
                (info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT) ||
                (!final && !(info.FileAttributes & FILE_ATTRIBUTE_DIRECTORY)))
                return {};
            if (final)
                return opened;
            // Deny metadata writes while traversing this checked directory;
            // renames keep the same parent handle and the final path guard.
            directory = std::move(opened);
            parent = directory.get();
            child.remove_prefix(separator + 1);
        }
    }

    bool ReadDocumentAsset(JsonObject const& request, fs::path const& staged)
    {
        auto relative = request.GetNamedString(L"relative");
        auto limit = request.GetNamedNumber(L"maximumBytes");
        std::wstring_view child{relative};
        if (child.find(L'\0') != child.npos || !IsChildPath(child) || !(limit >= 1 && limit <= MaximumImageBytes))
            return false;

        // Only Windows can redeem this grant for a StorageFile already accessible to the caller.
        auto document = SharedStorageAccessManager::RedeemTokenForFileAsync(request.GetNamedString(L"documentToken")).get();
        auto path = document.Path();
        std::wstring_view documentPath{path};
        if (documentPath.find(L'\0') != documentPath.npos || !IsAbsoluteDocument(documentPath))
            return false;

        auto source = fs::path{documentPath}.lexically_normal();
        auto folder = source.parent_path();
        auto original = OpenPath(source, FILE_READ_ATTRIBUTES, FILE_ATTRIBUTE_NORMAL);
        auto root = OpenPath(folder, FILE_READ_ATTRIBUTES | FILE_TRAVERSE, FILE_FLAG_BACKUP_SEMANTICS);
        if (!original || GetFileType(original.get()) != FILE_TYPE_DISK || !root)
            return false;
        auto rootPath = FinalPath(root.get());
        if (rootPath.empty())
            return false;
        if (rootPath.back() != L'\\')
            rootPath += L'\\';

        auto originalPath = FinalPath(original.get());
        if (originalPath.rfind(L'\\') + 1 != rootPath.size() || !originalPath.starts_with(rootPath))
            return false;

        auto image = OpenRelativeAsset(root.get(), child);
        if (!image || GetFileType(image.get()) != FILE_TYPE_DISK)
            return false;
        // Relative components never follow junctions or symlinks. Keep the
        // final containment guard on the exact opened handle that we read.
        auto target = FinalPath(image.get());
        LARGE_INTEGER size{};
        if (!target.starts_with(rootPath) || !GetFileSizeEx(image.get(), &size) ||
            size.QuadPart < 0 || size.QuadPart > static_cast<LONGLONG>(limit))
            return false;

        std::ofstream output(staged, std::ios::binary | std::ios::trunc);
        std::array<char, 64 * 1024> buffer{};
        for (auto left = size.QuadPart; left > 0;)
        {
            DWORD read{};
            auto count = static_cast<DWORD>(std::min<LONGLONG>(left, buffer.size()));
            if (!ReadFile(image.get(), buffer.data(), count, &read, nullptr) || read == 0)
                return false;
            if (!output.write(buffer.data(), read))
                return false;
            left -= read;
        }
        output.close();
        return output.good();
    }

    void PublishReply(fs::path const& requestFile)
    {
        auto staged = requestFile;
        staged.replace_extension(L".partial");
        auto reply = requestFile;
        reply.replace_extension(L".reply");

        bool success = false;
        try
        {
            std::ifstream input(requestFile, std::ios::binary | std::ios::ate);
            auto length = input.tellg();
            if (length > 0 && length <= 64 * 1024)
            {
                std::string payload(static_cast<size_t>(length), '\0');
                input.seekg(0);
                if (input.read(payload.data(), length))
                {
                    auto request = JsonObject::Parse(winrt::to_hstring(payload));
                    if (request.GetNamedString(L"operation") == L"Files.ReadDocumentAsset")
                        success = ReadDocumentAsset(request, staged);
                }
            }
        }
        catch (std::exception const&) {}
        catch (winrt::hresult_error const&) {}

        if (!success)
        {
            std::ofstream{staged, std::ios::binary | std::ios::trunc};
            reply.replace_extension(L".refused");
        }
        std::error_code error;
        if (fs::exists(requestFile, error))
            fs::rename(staged, reply, error);
        // If the request was canceled while copying, only the staged file exists.
        if (!fs::exists(requestFile, error))
            fs::remove(reply, error);
        fs::remove(staged, error);
    }

    void Serve(fs::path const& directory, HANDLE owner)
    {
        auto notification = FindFirstChangeNotificationW(directory.c_str(), FALSE, FILE_NOTIFY_CHANGE_FILE_NAME);
        if (notification == INVALID_HANDLE_VALUE)
            return;
        struct CloseNotification
        {
            HANDLE value;
            ~CloseNotification() { FindCloseChangeNotification(value); }
        } closeNotification{notification};

        auto ready = directory / L"ready";
        auto stagedReady = directory / L"ready.partial";
        {
            std::ofstream output(stagedReady, std::ios::binary | std::ios::trunc);
            output << winrt::to_string(Protocol);
            output.close();
            if (!output)
                return;
        }
        fs::rename(stagedReady, ready);
        HANDLE signals[]{owner, notification};
        auto lastWork = GetTickCount64();
        for (;;)
        {
            std::error_code scanError;
            if (WaitForSingleObject(owner, 0) != WAIT_TIMEOUT || fs::exists(directory / L"close", scanError) || scanError)
                break;
            for (auto entry = fs::directory_iterator(directory, scanError);
                 !scanError && entry != fs::directory_iterator(); entry.increment(scanError))
            {
                auto path = entry->path();
                if (path.extension() != L".request" || !IsId(path.stem().native()))
                    continue;
                auto reply = path;
                reply.replace_extension(L".reply");
                auto refused = path;
                refused.replace_extension(L".refused");
                std::error_code replyError;
                if (!fs::exists(reply, replyError) && !replyError && !fs::exists(refused, replyError) && !replyError)
                {
                    PublishReply(path);
                    lastWork = GetTickCount64();
                }
            }
            if (scanError)
                break;
            auto elapsed = GetTickCount64() - lastWork;
            if (elapsed >= IdleMilliseconds)
                break;
            auto wait = WaitForMultipleObjects(2, signals, FALSE, IdleMilliseconds - static_cast<DWORD>(elapsed));
            if (wait != WAIT_OBJECT_0 + 1 || !FindNextChangeNotification(notification))
                break;
        }
    }

    void Retire(fs::path const& directory) noexcept
    {
        // Only remove this instance's files. Never traverse a directory or reparse point.
        std::error_code error;
        for (auto entry = fs::directory_iterator(directory, error); !error && entry != fs::directory_iterator();
             entry.increment(error))
        {
            auto path = entry->path();
            auto name = path.filename().native();
            auto suffix = path.extension();
            if (name == L"ready" || name == L"ready.partial" || name == L"close" ||
                (IsId(path.stem().native()) && (suffix == L".partial" || suffix == L".request" ||
                    suffix == L".reply" || suffix == L".refused")))
            {
                auto attributes = GetFileAttributesW(path.c_str());
                if (attributes != INVALID_FILE_ATTRIBUTES && !(attributes & FILE_ATTRIBUTE_DIRECTORY))
                    DeleteFileW(path.c_str());
            }
        }
        // Keep the released lease until payload cleanup finishes so the
        // host can identify and reclaim this session if retirement is interrupted.
        if (!error)
            DeleteFileW((directory / L"lease").c_str());
        RemoveDirectoryW(directory.c_str());
    }
}

int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int)
{
    try
    {
        winrt::init_apartment();
        int count{};
        auto arguments = CommandLineToArgvW(GetCommandLineW(), &count);
        struct ReleaseArguments
        {
            LPWSTR* value;
            ~ReleaseArguments() { LocalFree(value); }
        } release{arguments};
        // Windows can prepend launcher metadata such as /InvokerPRAID.
        if (!arguments || count < 4 || std::wstring_view{arguments[count - 3]} != L"--notepads-desktop-bridge" ||
            !IsId(arguments[count - 2]))
            return 1;
        auto owner = OwnerProcess(arguments[count - 1]);
        if (!owner)
            return 1;
        auto base = fs::path{std::wstring_view{ApplicationData::Current().LocalFolder().Path()}} / L"DocumentAssets";
        auto directory = base / arguments[count - 2];
        if (!IsDirectory(base) || !IsDirectory(directory))
            return 1;
        int result = 0;
        {
            // Keep a timed-out worker from being replaced while its file I/O is still running.
            winrt::file_handle lease{CreateFileW((directory / L"lease").c_str(), GENERIC_READ, 0, nullptr,
                OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OPEN_REPARSE_POINT, nullptr)};
            if (!lease)
                return 1;
            try { Serve(directory, owner.get()); }
            catch (...) { result = 1; }
        }
        Retire(directory);
        return result;
    }
    catch (...) { return 1; }
}
