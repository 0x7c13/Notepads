#pragma once
#pragma comment(lib, "dbgeng.lib")
#include "pch.h"
#include "dbgeng.h"

#ifdef  _DEBUG
#include "rapidjson/prettywriter.h"
#else
#include "rapidjson/writer.h"
#endif

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

    template <typename json_writer>
    void serialize(json_writer& writer) const noexcept
    {
        if (m_offset <= 0) return;

        auto full_name = winrt::to_string(name());
        auto file_name = winrt::to_string(file());
        auto line_number = line();

        auto delimiter = full_name.find_first_of('!');
        auto class_name = delimiter == std::string::npos ? full_name : full_name.substr(0, delimiter);
        auto method_name = delimiter == std::string::npos ? "" : full_name.substr(delimiter + 1, full_name.length());

        writer.StartObject();
        writer.String("className");
        writer.String(class_name.c_str(), static_cast<rapidjson::SizeType>(class_name.length()));
        writer.String("methodName");
        writer.String(method_name.c_str(), static_cast<rapidjson::SizeType>(method_name.length()));
        writer.String("fileName");
        writer.String(file_name.c_str(), static_cast<rapidjson::SizeType>(file_name.length()));
        writer.String("lineNumber");
        writer.Uint(line_number);
        writer.EndObject();
    }

private:

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