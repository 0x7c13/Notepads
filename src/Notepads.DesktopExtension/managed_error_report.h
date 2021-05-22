#pragma once
#include "pch.h"
#include "error.h"
#include "error_attachment_report.h"

struct managed_error_report : report
{
	explicit managed_error_report(winrt_error const& error, std::string const& attachment = "") noexcept :
		report(), m_error(error), m_attachment(attachment)
	{
		m_sid = winrt::to_string(winrt::to_hstring(winrt::Windows::Foundation::GuidHelper::CreateNewGuid()));
		m_sid.erase(0, m_sid.find_first_not_of('{')).erase(m_sid.find_last_not_of('}') + 1);
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

	std::string sid() const noexcept
	{
		return m_sid;
	}

protected:

	virtual void append_type_data(json_writer& writer)  const noexcept
	{
		writer.String("type");
		writer.String("managedError");
	}

	virtual void append_additional_data(json_writer& writer)  const noexcept
	{
		report::append_additional_data(writer);

		writer.String("sid");
		writer.String(m_sid.c_str(), static_cast<rapidjson::SizeType>(m_sid.length()));
		writer.String("processId");
		writer.Uint(m_pid);
		writer.String("fatal");
		writer.Bool(m_error.fatal());
		writer.String("processName");
		writer.String(m_process.c_str(), static_cast<rapidjson::SizeType>(m_process.length()));
		writer.String("errorThreadId");
		writer.Uint(m_thread);

		// Write exception data
		writer.String("exception");
		m_error.serialize(writer);
	}

	virtual void append_report(json_writer& writer)  const noexcept
	{
		report::append_report(writer);

		if (!m_attachment.empty())
		{
			error_attachment_report(m_id, m_attachment).serialize(writer);
		}
	}

	unsigned m_pid = GetCurrentProcessId();
	unsigned m_thread = GetCurrentThreadId();
	std::string m_sid;
	std::string m_process = "Notepads32.exe";
	std::string m_attachment;
	winrt_error m_error;
};