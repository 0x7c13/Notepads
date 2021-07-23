#pragma once
#include "pch.h"
#include "error.h"
#include "managed_error_report.h"

struct handled_error_report : managed_error_report
{
	explicit handled_error_report(
		winrt_error const& error,
		report::dictionary const& properties,
		std::string const& attachment = ""
	) noexcept :
		managed_error_report(error, attachment), m_properties(properties)
	{
	}

	handled_error_report(handled_error_report const& other) noexcept :
		managed_error_report(other),
		m_properties(other.m_properties)
	{
	}

	handled_error_report(handled_error_report&& other) noexcept :
		managed_error_report(other)
	{
	}

protected:
	virtual hstring type() const noexcept
	{
		return L"handledError";
	}

	virtual void append_additional_data(json_object& json_obj)  const noexcept
	{
		managed_error_report::append_additional_data(json_obj);

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

	report::dictionary m_properties;
};