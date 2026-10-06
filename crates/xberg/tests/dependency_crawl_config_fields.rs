#![cfg(feature = "url-config-types")]

use serde_json::json;
use xberg::{BrowserConfig, CrawlConfig};

#[test]
fn new_crawl_fields_roundtrip_through_public_config_types() {
    let crawl: CrawlConfig = serde_json::from_value(json!({
        "path_patterns_match_url": true,
        "browser": {
            "chrome_path": "/opt/chromium",
            "chrome_args": ["--disable-gpu"]
        },
        "ssrf": {
            "denylist": [{"type": "cidr", "value": "203.0.113.0/24"}]
        }
    }))
    .expect("new crawl fields must deserialize through Xberg's public config surface");

    assert!(crawl.path_patterns_match_url);
    assert_eq!(
        crawl.browser.chrome_path.as_deref(),
        Some(std::path::Path::new("/opt/chromium"))
    );
    assert_eq!(crawl.browser.chrome_args, vec!["--disable-gpu"]);

    let crawl_json = serde_json::to_value(&crawl).expect("crawl config must serialize");
    assert_eq!(
        crawl_json["ssrf"]["denylist"],
        json!([{"type": "cidr", "value": "203.0.113.0/24"}])
    );

    let browser_json = serde_json::to_value(BrowserConfig {
        chrome_path: Some("/usr/bin/chromium".into()),
        chrome_args: vec!["--lang=fr".to_string()],
        ..BrowserConfig::default()
    })
    .expect("browser config must serialize");
    assert_eq!(browser_json["chrome_path"], "/usr/bin/chromium");
    assert_eq!(browser_json["chrome_args"], json!(["--lang=fr"]));
}
