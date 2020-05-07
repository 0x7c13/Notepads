using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.Serialization;
using System.ServiceModel;
using System.Text;

namespace Notepads.DesktopExtension
{
    // NOTE: You can use the "Rename" command on the "Refactor" menu to change the interface name "IExtensionService" in both code and config file together.
    [ServiceContract]
    public interface IExtensionService
    {
        [OperationContract]
        bool ReplaceFile(string newPath, string oldPath);
    }
}
