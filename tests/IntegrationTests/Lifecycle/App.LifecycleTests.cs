// ---------------------------------------------------------------------------------------------
//  Copyright (c) 2026, Jiaqi (0x7c13) Liu. All rights reserved.
//  See LICENSE.txt in the project root for license information.
// ---------------------------------------------------------------------------------------------

namespace Notepads;

public sealed partial class App
{
    internal bool TestHasWorkspace => _windowContext != null;
    internal bool TestOwnsPrimaryWorkspace => _bootstrap.IsPrimaryInstance;
    internal string TestInstanceKey => _bootstrap.InstanceId.ToString();
    internal static int TestPresentationRequests { get; private set; }
    internal static System.Threading.Tasks.Task TestPresentActivation(System.Threading.Tasks.Task activation) =>
        PresentActivation(activation, false, TestPresentWindow);

    private static void TestPresentWindow(bool rootFrameCreated)
    {
        TestPresentationRequests++;
        PresentWindow(rootFrameCreated);
    }

    // Count the old ordering's late request even when Windows denies foreground.
    internal static async System.Threading.Tasks.Task TestLatePresentation(System.Threading.Tasks.Task activation)
    {
        await activation;
        TestPresentWindow(false);
    }
}
