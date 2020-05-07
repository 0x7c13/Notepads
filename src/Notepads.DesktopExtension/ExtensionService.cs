using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Runtime.Serialization;
using System.ServiceModel;
using System.Text;

namespace Notepads.DesktopExtension
{
    // NOTE: You can use the "Rename" command on the "Refactor" menu to change the class name "ExtensionService" in both code and config file together.
    public class ExtensionService : IExtensionService
    {
        public bool ReplaceFile(string newPath, string oldPath)
        {
            try
            {
                if (File.Exists(oldPath)) File.Delete(oldPath);
                File.Move(newPath, oldPath);
                if (File.Exists(newPath)) File.Delete(newPath);
            }
            catch (Exception)
            {
                return false;
            }
            return true;
        }
    }
}
