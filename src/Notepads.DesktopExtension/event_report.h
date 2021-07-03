#pragma once
#include "pch.h"
#include "error.h"
#include "report.h"

struct event_report : report
{
	explicit event_report(hstring const& name, report::dictionary const& properties) noexcept :
		report(), m_sid(last_error_report_sid), m_name(name), m_properties(properties)
	{
		last_error_report_sid = L"";
	}

	event_report(event_report const& other) noexcept :
		report(other),
		m_sid(other.m_sid),
		m_name(other.m_name),
		m_properties(other.m_properties)
	{
	}

	event_report(event_report&& other) noexcept :
		report(other)
	{
	}

protected:

	virtual hstring type() const noexcept
	{
		return L"event";
	}

	virtual void append_additional_data(json_object& json_obj)  const noexcept
	{
		report::append_additional_data(json_obj);

		json_obj.Insert(L"sid", JsonValue::CreateStringValue(m_sid));
		json_obj.Insert(L"name", JsonValue::CreateStringValue(m_name));

		// Write custom properties if available
		if (!m_properties.empty())
		{
			auto properties = json_object();
			for (auto& property : m_properties)
			{
				properties.Insert(property.first, JsonValue::CreateStringValue(property.second));
			}

			json_obj.Insert(L"properties", properties);
		}
	}

	hstring m_sid;
	hstring m_name;
	report::dictionary m_properties;
};