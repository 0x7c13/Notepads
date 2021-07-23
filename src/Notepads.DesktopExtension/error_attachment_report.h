#pragma once
#include "pch.h"
#include "report.h"

struct error_attachment_report : report
{
	explicit error_attachment_report(hstring const& id, std::string const& attachment) noexcept :
		report(), m_errorid(id), m_attachment(winrt::to_hstring(base64_encode(attachment)))
	{
	}

	error_attachment_report(error_attachment_report const& other) noexcept :
		report(other),
		m_errorid(other.m_errorid),
		m_content(other.m_content),
		m_attachment(other.m_attachment)
	{
	}

	error_attachment_report(error_attachment_report&& other) noexcept :
		report(other)
	{
	}

	bool empty() const noexcept
	{
		return m_attachment.empty();
	}

protected:
	virtual hstring type() const noexcept
	{
		return L"errorAttachment";
	}

	virtual void append_additional_data(json_object& json_obj)  const noexcept
	{
		report::append_additional_data(json_obj);

		json_obj.Insert(L"contentType", JsonValue::CreateStringValue(m_content));
		json_obj.Insert(L"data", JsonValue::CreateStringValue(m_attachment));
		json_obj.Insert(L"errorId", JsonValue::CreateStringValue(m_errorid));
	}

	hstring m_errorid;
	hstring m_content = L"text/plain";
	hstring m_attachment;

private:
	//From https://stackoverflow.com/a/34571089/5155484
	static std::string base64_encode(std::string const& in) noexcept
	{
		static const std::string base64_str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
		std::string out;

		int val = 0, valb = -6;
		for (auto c : in) {
			val = (val << 8) + c;
			valb += 8;
			while (valb >= 0) {
				out.push_back(base64_str[(val >> valb) & 0x3F]);
				valb -= 6;
			}
		}
		if (valb > -6) out.push_back(base64_str[((val << 8) >> (valb + 8)) & 0x3F]);
		while (out.size() % 4) out.push_back('=');
		return out;
	}
};