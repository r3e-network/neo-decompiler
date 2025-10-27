using Neo.SmartContract.Framework;
using Neo.SmartContract.Framework.Attributes;
using Neo.SmartContract.Framework.Services;

namespace Examples
{
    [ManifestExtra("Author", "neo-decompiler")]
    [ManifestExtra("Email", "dev@example.com")]
    [ManifestExtra("Description", "Minimal example contract for neo-decompiler walkthrough.")]
    public class HelloWorld : SmartContract
    {
        public static string Main()
        {
            return "hello, neo!";
        }

        public static void Notify(string message)
        {
            Runtime.Notify(message);
        }
    }
}
