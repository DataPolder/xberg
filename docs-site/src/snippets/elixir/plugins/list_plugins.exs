# List all registered plugins
{:ok, extractors} = Xberg.list_document_extractors()
{:ok, ocr_backends} = Xberg.list_ocr_backends()
{:ok, post_processors} = Xberg.list_post_processors()
{:ok, validators} = Xberg.list_validators()

IO.puts("Document extractors:")
Enum.each(extractors, fn name -> IO.puts("  - #{name}") end)

IO.puts("\nOCR backends:")
Enum.each(ocr_backends, fn name -> IO.puts("  - #{name}") end)

IO.puts("\nPost-processors:")
Enum.each(post_processors, fn name -> IO.puts("  - #{name}") end)

IO.puts("\nValidators:")
Enum.each(validators, fn name -> IO.puts("  - #{name}") end)

IO.puts(
  "\nTotal: #{length(extractors)} extractors, #{length(ocr_backends)} OCR backends, " <>
    "#{length(post_processors)} post-processors, #{length(validators)} validators"
)
