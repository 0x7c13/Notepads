namespace Notepads.DesktopExtension.Services
{
    using System.Runtime.Serialization;
    using System.ServiceModel;
    using System.Threading.Tasks;

    // NOTE: You can use the "Rename" command on the "Refactor" menu to change the interface name "IAdminService" in both code and config file together.
    [ServiceContract]
    public interface IAdminService
    {
        [OperationContract]
        Task<bool> SaveFile(string memoryMapName, string filePath, int dataArrayLength);
    }
}
