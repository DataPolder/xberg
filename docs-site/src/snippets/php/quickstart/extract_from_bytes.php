<?php

declare(strict_types=1);

/**
 * Extracting from Bytes
 *
 * Extract content from file data in memory instead of from disk.
 * Useful for processing uploaded files or data from remote sources.
 */

require_once __DIR__ . '/vendor/autoload.php';

use Xberg\ExtractionConfig;

$fileData = file_get_contents('document.pdf');
$mimeType = 'application/pdf';

$output = \Xberg\XbergApi::extract(
    \Xberg\ExtractInput::fromBytes($fileData, $mimeType),
    \Xberg\ExtractionConfig::default()
);
$result = $output->getResults()[0];
echo "Extracted from bytes:\n";
echo substr($result->content, 0, 200) . "...\n\n";

$uploadedFile = [
    'tmp_name' => '/tmp/uploaded_document.pdf',
    'type' => 'application/pdf',
    'size' => 1024000,
];

if (file_exists($uploadedFile['tmp_name'])) {
    $data = file_get_contents($uploadedFile['tmp_name']);
    $output = \Xberg\XbergApi::extract(
        \Xberg\ExtractInput::fromBytes($data, $uploadedFile['type']),
        \Xberg\ExtractionConfig::default()
    );
    $result = $output->getResults()[0];

    echo "Uploaded file processed:\n";
    echo "Size: " . strlen($data) . " bytes\n";
    echo "Content length: " . strlen($result->content) . " characters\n";
}
