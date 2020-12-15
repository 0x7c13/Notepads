#pragma once
#include "pch.h"
#include "models.h"

namespace AppCenter
{
	using namespace rapidjson;
	using namespace std;

	enum LogType
	{
		managedError,
		errorAttachment,
		event
	};

	namespace
	{
		static const string logTypes[] = { "managedError", "errorAttachment", "event" };
		static string launchTimeStamp;
		static Device deviceInfo = Device();
	}

	class Log
	{
	public:

		#pragma region  Constructors for error report

		Log(LogType type, const string& id, const string& sid, bool isFatal, Exception* exception) :
			_type(type), _id(id), _sid(sid), _fatal(isFatal), _exception(exception)
		{
			InitializeLog();
		}

		Log(LogType type, const string& id, const string& sid, bool isFatal, Exception* exception, const vector<pair<const CHAR*, string>>& properties) :
			_type(type), _id(id), _sid(sid), _fatal(isFatal), _exception(exception), _properties(properties)
		{
			InitializeLog();
		}

		#pragma endregion

		#pragma region  Constructor for error attachment

		Log(LogType type, const string& id, const string& errorId, const string& data) :
			_type(type), _id(id), _errorId(errorId), _data(data)
		{
			InitializeLog();
		}

		#pragma endregion

		#pragma region  Constructor for event report

		Log(LogType type, const string& id, const string& sid, const string& name, const vector<pair<const CHAR*, string>>& properties) :
			_type(type), _id(id), _sid(sid), _name(name), _properties(properties)
		{
			InitializeLog();
		}

		#pragma endregion

		Log(const Log& log) :
			_type(log._type), _name(log._name), _timestamp(log._timestamp), _processId(log._processId), _id(log._id), _sid(log._sid), _fatal(log._fatal),
			_processName(log._processName), _errorThreadId(log._errorThreadId), _data(log._data), _errorId(log._errorId),
			_exception(NULL), _properties(log._properties)
		{
			_exception = (log._exception == 0) ? 0 : new Exception(*log._exception);
			InitializeLog();
		}

		~Log()
		{
			delete _exception;
		}

		Log& operator=(const Log& log)
		{
			if (this == &log) return *this;

			delete _exception;
			_type = log._type;
			_timestamp = log._timestamp;
			_id = log._id;
			_sid = log._sid;
			_processId = log._processId;
			_fatal = log._fatal;
			_processName = log._processName;
			_errorThreadId = log._errorThreadId;
			_exception = (log._exception == 0) ? 0 : new Exception(*log._exception);
			_contentType = log._contentType;
			_data = log._data;
			_errorId = log._errorId;
			_name = log._name;
			_properties = log._properties;
			return *this;
		}

		template <typename Writer>
		void Serialize(Writer& writer) const
		{
			writer.StartObject();

			writer.String("type");
			writer.String(logTypes[_type].c_str(), static_cast<SizeType>(logTypes[_type].length()));
			writer.String("timestamp");
			writer.String(_timestamp.c_str(), static_cast<SizeType>(_timestamp.length()));
			writer.String("appLaunchTimestamp");
			writer.String(launchTimeStamp.c_str(), static_cast<SizeType>(launchTimeStamp.length()));
			writer.String("id");
			writer.String(_id.c_str(), static_cast<SizeType>(_id.length()));

			if (!_sid.empty())
			{
				writer.String("sid");
				writer.String(_sid.c_str(), static_cast<SizeType>(_sid.length()));
			}

			switch (_type)
			{
			case LogType::managedError:
				writer.String("processId");
				writer.Uint(_processId);
				writer.String("fatal");
				writer.Bool(_fatal);
				writer.String("processName");
				writer.String(_processName.c_str(), static_cast<SizeType>(_processName.length()));
				writer.String("errorThreadId");
				writer.Uint(_errorThreadId);

				// Write exception data
				writer.String("exception");
				_exception->Serialize(writer);
				break;
			case LogType::errorAttachment:
				writer.String("contentType");
				writer.String(_contentType.c_str(), static_cast<SizeType>(_contentType.length()));
				writer.String("data");
				writer.String(_data.c_str(), static_cast<SizeType>(_data.length()));
				writer.String("errorId");
				writer.String(_errorId.c_str(), static_cast<SizeType>(_errorId.length()));
				break;
			case LogType::event:
				writer.String("name");
				writer.String(_name.c_str(), static_cast<SizeType>(_name.length()));
				break;
			}

			// Write device specific data
			writer.String("device");
			deviceInfo.Serialize(writer);

			// Write custom properties if available
			if (!_properties.empty())
			{
				writer.String("properties");
				writer.StartObject();
				for (vector<pair<const CHAR*, string>>::const_iterator property = _properties.begin(); property != _properties.end(); ++property)
				{
					writer.String(property->first);
					writer.String(property->second.c_str(), static_cast<SizeType>(property->second.length()));
				}
				writer.EndObject();
			}

			writer.EndObject();
		}

	private:
		LogType _type;
		string _contentType = "text/plain";
		string _name;
		string _timestamp = getTimeStamp();
		unsigned _processId;
		string _id;
		string _sid;
		bool _fatal = false;
		string _processName = "Notepads32.exe";
		unsigned _errorThreadId = 0;
		string _data;
		string _errorId;
		Exception* _exception = NULL;
		vector<pair<const CHAR*, string>> _properties;

		void InitializeLog()
		{
			switch (_type)
			{
			case LogType::managedError:
				_processId = GetCurrentProcessId();
				_errorThreadId = GetCurrentThreadId();
				break;
			case LogType::errorAttachment:
				break;
			case LogType::event:
				break;
			}
		}
	};
}