namespace Notepads.Services
{
    using System;
    using System.Threading.Tasks;

    public enum NotepadsOperationProtocol
    {
        Unrecognized,
        OpenNewInstance
    }

    public static class NotepadsProtocolService
    {
        private const string NewInstanceProtocolStr = "newinstance";

        public static NotepadsOperationProtocol GetOperationProtocol(Uri uri, out string context)
        {
            context = null;

            var uriStr = uri.ToString().Trim();

            if (string.IsNullOrEmpty(uriStr) || !uriStr.StartsWith("notepads://", StringComparison.OrdinalIgnoreCase))
            {
                return NotepadsOperationProtocol.Unrecognized;
            }

            var operation = uriStr.Substring("notepads://".Length);

            if (operation.EndsWith("/"))
            {
                operation = operation.Remove(operation.Length - 1);
            }

            if (!string.IsNullOrEmpty(operation) && string.Equals(NewInstanceProtocolStr, operation, StringComparison.OrdinalIgnoreCase))
            {
                return NotepadsOperationProtocol.OpenNewInstance;
            }

            return NotepadsOperationProtocol.Unrecognized;
        }

        public static async Task<bool> LaunchProtocolAsync(NotepadsOperationProtocol operation)
        {
            if (operation == NotepadsOperationProtocol.Unrecognized)
            {
                return false;
            }
            else if (operation == NotepadsOperationProtocol.OpenNewInstance)
            {
                var uriToLaunch = $"notepads://{NewInstanceProtocolStr}";
                return await Windows.System.Launcher.LaunchUriAsync(new Uri(uriToLaunch.ToLower()));
            }
            else
            {
                return false;
            }
        }
    }
}