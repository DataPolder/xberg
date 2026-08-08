```php title="PHP"
<?php declare(strict_types=1);

use Xberg\XbergApi;
use Xberg\ExtractedDocument;
use Xberg\ExtractionConfig;
use PHPUnit\Framework\TestCase;

class CustomPluginTest extends TestCase {
    private WordCountProcessor $plugin;
    private ExtractedDocument $mockResult;
    private ExtractionConfig $mockConfig;

    protected function setUp(): void {
        // Build a real ExtractedDocument/ExtractionConfig — WordCountProcessor's
        // process()/should_process() are typed against these classes, not stdClass.
        $this->mockResult = new ExtractedDocument(
            'Test content with some words',
            'text/plain',
            detectedLanguages: ['eng'],
        );
        $this->mockConfig = ExtractionConfig::default();

        // Initialize plugin
        $this->plugin = new WordCountProcessor();
        $this->plugin->initialize();
    }

    protected function tearDown(): void {
        $this->plugin->shutdown();
    }

    public function testPluginInitialization(): void {
        $this->assertNotNull($this->plugin);
        $this->assertEquals("word-count", $this->plugin->name());
    }

    public function testPluginProcessing(): void {
        // Test that plugin processes results without throwing.
        // NOTE: process() cannot currently attach data back onto $result (see
        // word_count_processor.md) so there is nothing further to assert here.
        $this->plugin->process($this->mockResult, $this->mockConfig);
        $this->addToAssertionCount(1);
    }

    public function testShouldProcess(): void {
        // Test should_process logic
        $this->assertTrue($this->plugin->should_process($this->mockResult, $this->mockConfig));

        // Empty content should not process
        $emptyResult = new ExtractedDocument('', 'text/plain');
        $this->assertFalse($this->plugin->should_process($emptyResult, $this->mockConfig));
    }

    public function testProcessingStage(): void {
        $stage = $this->plugin->processing_stage();
        $this->assertEquals("Early", $stage);
    }

    public function testPriority(): void {
        $priority = $this->plugin->priority();
        $this->assertGreaterThanOrEqual(0, $priority);
        $this->assertLessThanOrEqual(255, $priority);
    }
}
```
