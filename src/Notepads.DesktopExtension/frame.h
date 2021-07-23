#pragma once
#pragma comment(lib, "dbgeng.lib")
#include "pch.h"
#include "dbgeng.h"

__declspec(selectany) winrt::com_ptr<IDebugSymbols> symbols;

struct frame
{
    using from_abi_t = winrt::take_ownership_from_abi_t;
    static constexpr auto from_abi{ winrt::take_ownership_from_abi };

    frame() noexcept = default;
    frame(frame&&) = default;
    frame& operator=(frame&&) = default;

    frame(frame const& other) noexcept :
        m_offset(other.m_offset)
    {
    }

    frame& operator=(frame const& other) noexcept
    {
        m_offset = other.m_offset;
        return *this;
    }

    frame& operator=(uint64_t addr) noexcept
    {
        m_offset = addr;
        return *this;
    }

    explicit frame(uint64_t addr) noexcept
        : m_offset(addr)
    {
    }

    template <class T>
    explicit frame(T* addr) noexcept
        : m_offset(reinterpret_cast<uint64_t>(addr))
    {
    }

    winrt::hstring name() const noexcept
    {
        originate();

        if (!symbols) return winrt::to_hstring(m_offset);

        auto name = std::string(256, '\0');
        auto size = 0UL;
        auto result = SUCCEEDED(
            symbols->GetNameByOffset(
                m_offset,
                &name[0],
                static_cast<ULONG>(name.size()),
                &size,
                nullptr
            )
        );

        if (!result && size > name.size())
        {
            name.resize(size);
            result = SUCCEEDED(
                symbols->GetNameByOffset(
                    m_offset,
                    &name[0],
                    static_cast<ULONG>(name.size()),
                    &size,
                    nullptr
                )
            );
        }

        return winrt::to_hstring(name.data());
    }

    winrt::hstring file() const noexcept
    {
        originate();

        if (!symbols) return L"";

        auto file = std::string(256, '\0');
        auto size = 0UL;
        auto result = SUCCEEDED(
            symbols->GetLineByOffset(
                m_offset,
                nullptr,
                &file[0],
                static_cast<ULONG>(file.size()),
                &size,
                nullptr
            )
        );

        if (!result && size > file.size())
        {
            file.resize(size);
            result = SUCCEEDED(
                symbols->GetLineByOffset(
                    m_offset,
                    nullptr,
                    &file[0],
                    static_cast<ULONG>(file.size()),
                    &size,
                    nullptr
                )
            );
        }

        return winrt::to_hstring(file.data());
    }

    uint48_t line() const noexcept
    {
        originate();

        if (!symbols) return 0;

        auto line = 0UL;
        auto result = SUCCEEDED(
            symbols->GetLineByOffset(m_offset, &line, 0, 0, 0, 0)
        );

        return (result ? line : 0);
    }

    winrt::Windows::Data::Json::IJsonValue to_json() const noexcept
    {
        std::wstring full_name = name().c_str();
        auto delimiter = full_name.find_first_of('!');
        auto class_name = delimiter == std::wstring::npos ? full_name : full_name.substr(0, delimiter);
        auto method_name = delimiter == std::wstring::npos ? L"" : full_name.substr(delimiter + 1, full_name.length());

        auto json_obj = winrt::Windows::Data::Json::JsonObject();
        json_obj.Insert(L"className", JsonValue::CreateStringValue(class_name));
        json_obj.Insert(L"methodName", JsonValue::CreateStringValue(method_name));
        json_obj.Insert(L"fileName", JsonValue::CreateStringValue(file()));
        json_obj.Insert(L"lineNumber", JsonValue::CreateNumberValue(line()));
        return json_obj;
    }

private:
    using JsonValue = winrt::Windows::Data::Json::JsonValue;

    static void originate() noexcept
    {
        if (!symbols)
        {
            winrt::com_ptr<IDebugClient> client = nullptr;
            DebugCreate(winrt::guid_of<IDebugClient>(), client.put_void());
            client->AttachProcess(0, GetCurrentProcessId(), DEBUG_ATTACH_NONINVASIVE | DEBUG_ATTACH_NONINVASIVE_NO_SUSPEND);
            client.as<IDebugControl>()->WaitForEvent(DEBUG_WAIT_DEFAULT, INFINITE);
            client.as(symbols);
        }
    }

    uint64_t m_offset{ 0 };
};