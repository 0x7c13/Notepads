#pragma once
#include "pch.h"
#include "frame.h"
#include "restrictederrorinfo.h"

#define TRACED_FRAMES_COUNT 5
#define SKIPPED_FRAMES_COUNT 3

struct winrt_error
{
    using from_abi_t = winrt::take_ownership_from_abi_t;
    static constexpr auto from_abi{ winrt::take_ownership_from_abi };

    winrt_error() noexcept = default;
    winrt_error(winrt_error&&) = default;
    winrt_error& operator=(winrt_error&&) = default;

    winrt_error(winrt_error const& other) noexcept :
        m_code(other.m_code),
        m_info(other.m_info),
        m_trace(other.m_trace),
        m_fatal(other.m_fatal)
    {
    }

    winrt_error& operator=(winrt_error const& other) noexcept
    {
        m_code = other.m_code;
        m_info = other.m_info;
        m_trace = other.m_trace;
        m_fatal = other.m_fatal;
        return *this;
    }

    explicit winrt_error(winrt::hresult const code, bool fatal, uint48_t skip = 0) noexcept :
        m_code(verify_error(code)),
        m_fatal(fatal)
    {
        originate(code, nullptr, skip + SKIPPED_FRAMES_COUNT);
    }

    explicit winrt_error(winrt::hresult_error const& error, bool fatal = true, uint48_t skip = 0) noexcept :
        m_code(error.code()),
        m_info(error.try_as<winrt::impl::IRestrictedErrorInfo>()),
        m_fatal(fatal)
    {
        trace_stack(skip + SKIPPED_FRAMES_COUNT);
    }

    winrt_error(winrt::hresult const code, winrt::param::hstring const& message) noexcept : m_code(verify_error(code))
    {
        originate(code, winrt::param::get_abi(message), SKIPPED_FRAMES_COUNT);
    }

    winrt_error(winrt::hresult const code, winrt::take_ownership_from_abi_t) noexcept : m_code(verify_error(code))
    {
        winrt::com_ptr<winrt::impl::IErrorInfo> info;
        WINRT_IMPL_GetErrorInfo(0, info.put_void());

        if ((m_info = info.try_as<winrt::impl::IRestrictedErrorInfo>()))
        {
            WINRT_VERIFY_(0, m_info->GetReference(m_debug_reference.put()));

            if (auto info2 = m_info.try_as<winrt::impl::ILanguageExceptionErrorInfo2>())
            {
                WINRT_VERIFY_(0, info2->CapturePropagationContext(nullptr));
            }
        }
        else
        {
            winrt::impl::bstr_handle legacy;

            if (info)
            {
                info->GetDescription(legacy.put());
            }

            winrt::hstring message;

            if (legacy)
            {
                message = winrt::impl::trim_hresult_message(legacy.get(), WINRT_IMPL_SysStringLen(legacy.get()));
            }

            originate(code, get_abi(message), SKIPPED_FRAMES_COUNT);
        }
    }

    winrt::hresult code() const noexcept
    {
        return m_code;
    }

    winrt::hstring message() const noexcept
    {
        if (m_info)
        {
            int32_t code{};
            winrt::impl::bstr_handle fallback;
            winrt::impl::bstr_handle message;
            winrt::impl::bstr_handle unused;

            if (0 == m_info->GetErrorDetails(fallback.put(), &code, message.put(), unused.put()))
            {
                if (code == m_code)
                {
                    if (message)
                    {
                        return winrt::impl::trim_hresult_message(message.get(), WINRT_IMPL_SysStringLen(message.get()));
                    }
                    else
                    {
                        return winrt::impl::trim_hresult_message(fallback.get(), WINRT_IMPL_SysStringLen(fallback.get()));
                    }
                }
            }
        }

        return winrt::impl::message_from_hresult(m_code);
    }

    winrt::hstring stacktrace() const noexcept
    {
        if (!m_info || m_trace.empty()) return L"";

        std::wstringstream stacktrace{ L"" };
        for (auto& trace : m_trace)
        {
            stacktrace << L"   at " << trace.name().c_str() << L"  in " << trace.file().c_str() << L" :line" << trace.line() << L"\n";
        }

        return stacktrace.str().c_str();
    }

    winrt_error inner() const noexcept
    {
        winrt::com_ptr<winrt::impl::ILanguageExceptionErrorInfo2> info;
        m_info.try_as<winrt::impl::ILanguageExceptionErrorInfo2>()->GetPreviousLanguageExceptionErrorInfo(info.put());

        winrt_error inner;
        if (inner.m_info = info.try_as<winrt::impl::IRestrictedErrorInfo>())
        {
            inner.trace_stack(0, true);
        }
        return inner;
    }

    bool fatal() const noexcept
    {
        return m_fatal;
    }

    template <typename To>
    auto try_as() const noexcept
    {
        return m_info.try_as<To>();
    }

    winrt::hresult to_abi() const noexcept
    {
        if (m_info)
        {
            WINRT_IMPL_SetErrorInfo(0, m_info.try_as<winrt::impl::IErrorInfo>().get());
        }

        return m_code;
    }

    template <typename json_writer>
    void serialize(json_writer& writer) const noexcept
    {
        auto error = winrt::to_string(fmt::format(L"HResult: {}", code()));
        auto msg = winrt::to_string(message());
        auto st = winrt::to_string(stacktrace());

        writer.StartObject();
        writer.String("type");
        writer.String(error.c_str(), static_cast<rapidjson::SizeType>(error.length()));
        writer.String("message");
        writer.String(msg.c_str(), static_cast<rapidjson::SizeType>(msg.length()));
        /*writer.String("stackTrace");
        writer.String(st.c_str(), static_cast<rapidjson::SizeType>(st.size()));*/
        writer.String("frames");
        writer.StartArray();
        for (auto& frame : m_trace)
        {
            frame.serialize(writer);
        }
        writer.EndArray();
        writer.EndObject();
    }

    static winrt_error get_last_error() noexcept
    {
        return winrt_error(winrt::impl::hresult_from_win32(GetLastError()), false);
    }

private:

    static int32_t __stdcall fallback_RoOriginateLanguageException(int32_t error, void* message, void*) noexcept
    {
        winrt::com_ptr<winrt::impl::IErrorInfo> info(new (std::nothrow) winrt::impl::error_info_fallback(error, message), winrt::take_ownership_from_abi);
        WINRT_VERIFY_(0, WINRT_IMPL_SetErrorInfo(0, info.get()));
        return 1;
    }

    void originate(winrt::hresult const code, void* message, uint48_t skip = 0) noexcept
    {
        static int32_t(__stdcall * handler)(int32_t error, void* message, void* exception) noexcept;
        winrt::impl::load_runtime_function("RoOriginateLanguageException", handler, fallback_RoOriginateLanguageException);
        WINRT_VERIFY(handler(code, message, nullptr));

        winrt::com_ptr<winrt::impl::IErrorInfo> info;
        WINRT_VERIFY_(0, WINRT_IMPL_GetErrorInfo(0, info.put_void()));
        WINRT_VERIFY(info.try_as(m_info));

        trace_stack(skip);
    }

    void trace_stack(uint48_t skip = 0, bool original = false) noexcept
    {
        auto infoWithStackTrace = m_info.try_as<ILanguageExceptionStackBackTrace>();
        uint64_t traces[TRACED_FRAMES_COUNT]{ 0 };
        uint48_t count = TRACED_FRAMES_COUNT;
        if (infoWithStackTrace || original)
        {
            infoWithStackTrace->GetStackBackTrace(TRACED_FRAMES_COUNT, traces, &count);
        }
        else
        {
            CaptureStackBackTrace(skip, TRACED_FRAMES_COUNT, (void**)traces, nullptr);
        }

        if (count <= 0) return;
        frame* frames = new frame[count];
        for (uint48_t i = 0; i < count; ++i)
        {
            frames[i] = traces[i];
        }

        m_trace = winrt::array_view(frames, count);
    }

    static winrt::hresult verify_error(winrt::hresult const code) noexcept
    {
        WINRT_ASSERT(code < 0);
        return code;
    }

#ifdef __clang__
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wunused-private-field"
#endif

    winrt::impl::bstr_handle m_debug_reference;
    uint32_t m_debug_magic{ 0xAABBCCDD };
    winrt::hresult m_code{ winrt::impl::error_fail };
    winrt::com_ptr<winrt::impl::IRestrictedErrorInfo> m_info;
    winrt::array_view<frame> m_trace;
    bool m_fatal = true;

#ifdef __clang__
#pragma clang diagnostic pop
#endif
};