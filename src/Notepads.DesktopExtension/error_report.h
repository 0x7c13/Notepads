#pragma once
#include "pch.h"
#include "managed_error_report.h"
#include "handled_error_report.h"

struct error_report : report
{
	explicit error_report(
		winrt_error const& error,
		report::dictionary const& properties,
		std::string const& attachment = ""
	) noexcept :
		m_handled_error_report(error, properties, attachment), m_managed_error_report(error, attachment)
	{
	}

	error_report(error_report const& other) noexcept :
		report(other),
		m_managed_error_report(other.m_managed_error_report),
		m_handled_error_report(other.m_handled_error_report)
	{
	}

	error_report(error_report&& other) noexcept :
		report(other),
		m_managed_error_report(other.m_managed_error_report),
		m_handled_error_report(other.m_handled_error_report)
	{
	}

	virtual winrt::Windows::Data::Json::IJsonValue to_json() const noexcept
	{
		auto json_obj = json_array();
		json_obj.Append(m_managed_error_report.to_json());
		json_obj.Append(m_handled_error_report.to_json());
		last_error_report_sid = m_managed_error_report.sid();
		return json_obj;
	}

protected:
	managed_error_report m_managed_error_report;
	handled_error_report m_handled_error_report;
};