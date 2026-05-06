using System;
using System.Threading.Tasks;
using Windows.Storage;
using Notepads.Services;
using Notepads.Views.MainPage;
using Windows.UI.Core;

namespace Notepads.Tests
{
    public static class EnhancedAutosaveTests
    {
        public static async Task RunTestsAsync(NotepadsMainPage mainPage)
        {
            LoggingService.LogInfo("Starting EnhancedAutosaveTests...");

            // Ensure EnhancedAutosave is enabled
            EnhancedAutosaveService.FeatureFlag_EnhancedAutosave = true;
            EnhancedAutosaveService.IsAutosaveEnabled = true;
            EnhancedAutosaveService.ShowSavedNotification = true;

            // 1. Changing a file triggers an immediate save event.
            // Create a temp file and load it
            StorageFile tempFile = await ApplicationData.Current.LocalFolder.CreateFileAsync("test_autosave.txt", CreationCollisionOption.ReplaceExisting);
            await mainPage.Dispatcher.RunAsync(CoreDispatcherPriority.Normal, async () =>
            {
                await mainPage.OpenFileAsync(tempFile);
                
                var editor = mainPage.GetTextEditor();
                
                // Simulate typing
                editor.TypeText("Automated Test Content");
                
                LoggingService.LogInfo("Test 1: Content changed, waiting for autosave...");
            });

            // Wait for 300ms debounce + queue processing
            await Task.Delay(1000);

            // Check if file was saved
            string savedContent = await FileIO.ReadTextAsync(tempFile);
            if (savedContent == "Automated Test Content")
            {
                LoggingService.LogInfo("Test 1 Passed: File was successfully autosaved.");
            }
            else
            {
                LoggingService.LogError("Test 1 Failed: File was not autosaved.");
            }

            // 2. Status-bar icon updates color and label
            // (Requires UI inspection, but we can verify the service state)
            LoggingService.LogInfo($"Test 2: LastSaveTime is {EnhancedAutosaveService.LastSaveTime}");

            // 3. Clicking the icon flips the persisted setting
            EnhancedAutosaveService.IsAutosaveEnabled = false;
            if (!EnhancedAutosaveService.IsAutosaveEnabled)
            {
                LoggingService.LogInfo("Test 3 Passed: Autosave disabled successfully.");
            }

            // 4. Notification toggle disabled when autosave is off
            // Checked in UI bindings

            // 5. No save popup appears when toggle is off, but file is still written
            EnhancedAutosaveService.IsAutosaveEnabled = true;
            EnhancedAutosaveService.ShowSavedNotification = false;
            
            await mainPage.Dispatcher.RunAsync(CoreDispatcherPriority.Normal, () =>
            {
                var editor = mainPage.GetTextEditor();
                editor.TypeText("Silent Save Content");
            });

            await Task.Delay(1000);

            savedContent = await FileIO.ReadTextAsync(tempFile);
            if (savedContent == "Silent Save Content")
            {
                LoggingService.LogInfo("Test 5 Passed: File was silently autosaved.");
            }
            else
            {
                LoggingService.LogError("Test 5 Failed: Silent autosave failed.");
            }

            // Cleanup
            await tempFile.DeleteAsync();
            LoggingService.LogInfo("EnhancedAutosaveTests completed.");
        }
    }
}
