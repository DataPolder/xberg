<?php

declare(strict_types=1);

/**
 * PDF Metadata Extractor Post-Processor
 *
 * Custom post-processor that extracts and enriches PDF metadata
 * during the extraction pipeline.
 */

require_once __DIR__ . '/vendor/autoload.php';

use Xberg\Xberg;
use Xberg\PostProcessor;
use Xberg\ExtractedDocument;
use Xberg\ExtractionConfig;

/**
 * Post-processor for extracting and enriching PDF metadata
 */
class PdfMetadataExtractor implements PostProcessor
{
    private int $processedCount;

    public function __construct()
    {
        $this->processedCount = 0;
    }

    /**
     * Get the name of this post-processor
     */
    public function name(): string
    {
        return 'pdf_metadata_extractor';
    }

    /**
     * Get the version of this post-processor
     */
    public function version(): string
    {
        return '1.0.0';
    }

    /**
     * Get the description of this post-processor
     */
    public function description(): string
    {
        return 'Extracts and enriches PDF metadata';
    }

    /**
     * Get the processing stage (early, normal, or late)
     */
    public function processing_stage(): string
    {
        return 'early';
    }

    /**
     * Determine if this processor should handle the result
     */
    public function should_process(ExtractedDocument $result, ExtractionConfig $config): bool
    {
        return $result->mimeType === 'application/pdf';
    }

    /**
     * Process the extraction result.
     *
     * NOTE: ExtractedDocument has no writable "metadata.custom" bag in the current
     * binding (Metadata exposes only its fixed, typed fields via getters — there is
     * no free-form map to attach arbitrary processor output to). This snippet cannot
     * be made to actually enrich metadata until that capability exists; flagged for a
     * product decision rather than guessed at here.
     */
    public function process(ExtractedDocument $result, ExtractionConfig $config): mixed
    {
        $this->processedCount++;

        return null;
    }

    /**
     * Initialize the post-processor
     */
    public function initialize(): void
    {
        error_log("PDF metadata extractor initialized");
    }

    /**
     * Shutdown the post-processor
     */
    public function shutdown(): void
    {
        error_log("Processed {$this->processedCount} PDFs");
    }

    /**
     * Get the number of processed documents
     */
    public function getProcessedCount(): int
    {
        return $this->processedCount;
    }
}

$processor = new PdfMetadataExtractor();
Xberg::registerPostProcessor($processor);
