using Xberg;
using System;
using System.Collections.Generic;
using System.Security.Cryptography;
using System.Text;
using System.Threading.Tasks;

// NOTE: The C# binding has no ICacheBackend plugin interface. Built-in
// caching is controlled via ExtractionConfig.UseCache/CacheNamespace/
// CacheTtlSecs. This class is a plain application-level memoization wrapper.

class CustomCacheWrapper
{
    private readonly Dictionary<string, (ExtractedDocument result, DateTime timestamp)> _cache;
    private readonly TimeSpan _cacheExpiration;

    public CustomCacheWrapper(TimeSpan? cacheExpiration = null)
    {
        _cache = new Dictionary<string, (ExtractedDocument, DateTime)>();
        _cacheExpiration = cacheExpiration ?? TimeSpan.FromHours(1);
    }

    public ExtractedDocument? Get(string key)
    {
        if (_cache.TryGetValue(key, out var entry))
        {
            if (DateTime.UtcNow - entry.timestamp < _cacheExpiration)
            {
                return entry.result;
            }
            else
            {
                _cache.Remove(key);
            }
        }

        return null;
    }

    public void Set(string key, ExtractedDocument result)
    {
        _cache[key] = (result, DateTime.UtcNow);
    }

    public void Delete(string key)
    {
        _cache.Remove(key);
    }

    public void Clear()
    {
        _cache.Clear();
    }

    public string GenerateKey(string filePath, ExtractionConfig? config)
    {
        var keyData = $"{filePath}:{config?.GetHashCode() ?? 0}";
        using var sha256 = SHA256.Create();
        var hashBytes = sha256.ComputeHash(Encoding.UTF8.GetBytes(keyData));
        return Convert.ToHexString(hashBytes);
    }

    public async Task<ExtractedDocument> GetOrExtractAsync(string filePath, ExtractionConfig? config = null)
    {
        var effectiveConfig = config ?? ExtractionConfig.Default();
        var cacheKey = GenerateKey(filePath, effectiveConfig);

        var cached = Get(cacheKey);
        if (cached != null)
        {
            Console.WriteLine("Retrieved from cache");
            return cached;
        }

        var document = (await XbergConverter.ExtractAsync(ExtractInput.FromUri(filePath), effectiveConfig)).Results[0];
        Set(cacheKey, document);
        Console.WriteLine("Extracted and cached");

        return document;
    }
}

class Program
{
    static async Task Main()
    {
        var cache = new CustomCacheWrapper(cacheExpiration: TimeSpan.FromMinutes(30));

        try
        {
            var config = new ExtractionConfig { UseCache = true };
            var filePath = "document.pdf";

            var document1 = await cache.GetOrExtractAsync(filePath, config);
            Console.WriteLine($"First extraction: {document1.Content.Length} chars");

            var document2 = await cache.GetOrExtractAsync(filePath, config);
            Console.WriteLine($"Second extraction: {document2.Content.Length} chars");

            cache.Clear();
            Console.WriteLine("Cache cleared");
        }
        catch (XbergException ex)
        {
            Console.WriteLine($"Error: {ex.Message}");
        }
    }
}
