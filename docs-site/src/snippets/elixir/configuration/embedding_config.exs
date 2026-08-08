alias Xberg.{ExtractionConfig, ChunkingConfig, EmbeddingConfig}

# Configure embeddings for vector search
# Embeddings are configured on the chunking config, not on ExtractionConfig directly.
config = %ExtractionConfig{
chunking: %ChunkingConfig{
max_characters: 512,
overlap: 50,
embedding: %EmbeddingConfig{
model: "sentence-transformers/all-MiniLM-L6-v2"
}
}
}

{:ok, output} = Xberg.extract(input: %Xberg.ExtractInput{kind: :uri, uri: "document.pdf"}, config: config)

result = List.first(output.results)
IO.puts("Extracted chunks with embeddings: #{length(result.chunks || [])}")
