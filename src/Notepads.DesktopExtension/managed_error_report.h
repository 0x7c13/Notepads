#pragma once
#include "pch.h"
#include "error.h"
#include "error_attachment_report.h"

struct managed_error_report : report
{
	explicit managed_error_report(winrt_error const& error, std::string const& attachment = "") noexcept :
		report(), m_error(error), m_attachment(m_id, attachment)
	{
		std::wstring sid = winrt::to_hstring(winrt::Windows::Foundation::GuidHelper::CreateNewGuid()).c_str();
		sid.erase(0, sid.find_first_not_of('{')).erase(sid.find_last_not_of('}') + 1);
		m_sid = sid;
	}

	managed_error_report(managed_error_report const& other) noexcept :
		report(other),
		m_pid(other.m_pid),
		m_thread(other.m_thread),
		m_sid(other.m_sid),
		m_process(other.m_process),
		m_error(other.m_error),
		m_attachment(other.m_attachment)
	{
	}

	managed_error_report(managed_error_report&& other) noexcept :
		report(other),
		m_error(other.m_error),
		m_attachment(other.m_attachment)
	{
	}

	hstring sid() const noexcept
	{
		return m_sid;
	}

	virtual winrt::Windows::Data::Json::IJsonValue to_json() const noexcept
	{
		auto json_obj = json_array();

		json_obj.Append(report::to_json());

		if (!m_attachment.empty())
		{
			json_obj.Append(m_attachment.to_json());
		}

		return json_obj;
	}

protected:
	virtual hstring type() const noexcept
	{
		return L"managedError";
	}

	virtual void append_additional_data(json_object& json_obj)  const noexcept
	{
		report::append_additional_data(json_obj);

		json_obj.Insert(L"sid", JsonValue::CreateStringValue(m_sid));
		json_obj.Insert(L"processId", JsonValue::CreateNumberValue(m_pid));
		json_obj.Insert(L"fatal", JsonValue::CreateBooleanValue(m_error.fatal()));
		json_obj.Insert(L"processName", JsonValue::CreateStringValue(m_process));
		json_obj.Insert(L"errorThreadId", JsonValue::CreateNumberValue(m_thread));
		json_obj.Insert(L"exception", m_error.to_json());
	}

	unsigned m_pid = GetCurrentProcessId();
	unsigned m_thread = GetCurrentThreadId();
	hstring m_sid;
	hstring m_process = L"Notepads32.exe";
	error_attachment_report m_attachment;
	winrt_error m_error;
};