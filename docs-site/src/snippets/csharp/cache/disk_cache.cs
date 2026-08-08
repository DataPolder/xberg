using Xberg;
using System;
using System.Threading.Tasks;

var config = new ExtractionConfig
{
    UseCache = true,
    CacheNamespace = "xberg_cache",
    CacheTtlSecs = 86400 * 7,
};

Console.WriteLine("First extraction (will be cached)...");
var result1 = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), config)).Results[0];
Console.WriteLine($"  - Content length: {result1.Content.Length}");

Console.WriteLine("\nSecond extraction (from cache)...");
var result2 = (await XbergConverter.ExtractAsync(ExtractInput.FromUri("document.pdf"), config)).Results[0];
Console.WriteLine($"  - Content length: {result2.Content.Length}");

Console.WriteLine($"\nResults are identical: {result1.Content == result2.Content}");

// NOTE: The C# binding exposes no public API to clear the on-disk cache or
// read cache statistics (no ClearCacheAsync/ClearAllCacheAsync/GetCacheStatsAsync).
// CacheStats.FromJson exists only to parse a JSON string you already have;
// there is no method that produces one. Manage the cache directory directly
// on disk if you need to clear it.
