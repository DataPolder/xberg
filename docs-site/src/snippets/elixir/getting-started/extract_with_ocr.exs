# Extract scanned documents with OCR
# Configure Tesseract for OCR processing

ocr_config = %Xberg.OcrConfig{
backend: "tesseract",
language: ["eng"]
}

config = %Xberg.ExtractionConfig{
ocr: ocr_config
}

{:ok, output} = Xberg.extract(input: %Xberg.ExtractInput{kind: :uri, uri: "scanned.pdf"}, config: config)

result = List.first(output.results)
IO.puts("Extracted text from scanned document:")
IO.puts(result.content)
IO.puts("Used OCR backend: tesseract")
