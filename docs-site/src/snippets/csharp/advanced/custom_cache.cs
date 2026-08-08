using Xberg;
using System;
using System.Collections.Generic;
using System.Threading.Tasks;

// The C# binding has no ICache plugin interface — caching is a native
// feature controlled via ExtractionConfig.UseCache/CacheNamespace/CacheTtlSecs.
// This is a plain application-level memoization wrapper, not a registered plugin.
class CustomCacheBackend
{
    private readonly Dictionary<string, ExtractedDocument> _cache = new();

    public async Task<ExtractedDocument> GetOrExtractAsync(
        string filePath,
        ExtractionConfig config)
    {
        var cacheKey = GenerateCacheKey(filePath, config);

        if (_cache.TryGetValue(cacheKey, out var cachedResult))
        {
            Console.WriteLine("Using cached result");
            return cachedResult;
        }

        var document = (await XbergConverter.ExtractAsync(ExtractInput.FromUri(filePath), config)).Results[0];

        _cache[cacheKey] = document;
        Console.WriteLine("Result cached");

        return document;
    }

    private string GenerateCacheKey(string filePath, ExtractionConfig config)
    {
        var configHash = config.ToString().GetHashCode();
        return $"{filePath}:{configHash}";
    }

    public void ClearCache()
    {
        _cache.Clear();
        Console.WriteLine("Cache cleared");
    }
}

class Program
{
    static async Task Main()
    {
        var cacheBackend = new CustomCacheBackend();
        var config = new ExtractionConfig { UseCache = true };

        try
        {
            var document1 = await cacheBackend.GetOrExtractAsync("document.pdf", config);
            Console.WriteLine($"Result 1: {document1.Content.Length} chars");

            var document2 = await cacheBackend.GetOrExtractAsync("document.pdf", config);
            Console.WriteLine($"Result 2: {document2.Content.Length} chars");

            cacheBackend.ClearCache();
        }
        catch (XbergException ex)
        {
            Console.WriteLine($"Error: {ex.Message}");
        }
    }
}
