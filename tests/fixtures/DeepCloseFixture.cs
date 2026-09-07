using System;
using System.Threading;
using System.Windows.Forms;

internal static class DeepCloseFixture
{
    [STAThread]
    private static void Main()
    {
        Application.EnableVisualStyles();
        using (var window = new Form
        {
            Text = "Still Deep Close Fixture",
            Width = 480,
            Height = 260,
            StartPosition = FormStartPosition.CenterScreen,
        })
        {
            Application.Run(window);
        }

        // Intentionally imitate an application that leaves a background host
        // after its last window closes. Still should stop this process.
        Thread.Sleep(TimeSpan.FromMinutes(1));
    }
}
