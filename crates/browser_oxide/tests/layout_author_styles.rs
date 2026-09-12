use browser_oxide::Page;

#[tokio::test]
async fn author_stylesheet_drives_absolute_percentage_geometry() {
    let profile = browser_oxide::stealth::presets::chrome_148_linux();
    let expected_width = profile.inner_width;
    let expected_height = profile.inner_height;
    let html = r#"<!doctype html><html><head><style>
        #viewport {
            width: 100%;
            height: 100%;
            position: absolute;
            left: 0;
            top: 0;
        }
    </style></head><body><div id="viewport"></div></body></html>"#;

    let mut page = Page::from_html_with_url(html, "https://example.test/layout", Some(profile))
        .await
        .expect("page");

    let actual = page
        .evaluate(
            r#"JSON.stringify((() => {
                const element = document.getElementById('viewport');
                const rect = element.getBoundingClientRect();
                return {
                    clientWidth: element.clientWidth,
                    clientHeight: element.clientHeight,
                    rectWidth: rect.width,
                    rectHeight: rect.height,
                };
            })())"#,
        )
        .expect("layout probe");

    let value: serde_json::Value = serde_json::from_str(&actual).expect("probe json");
    assert_eq!(value["clientWidth"], expected_width);
    assert_eq!(value["clientHeight"], expected_height);
    assert_eq!(value["rectWidth"], expected_width);
    assert_eq!(value["rectHeight"], expected_height);
}

#[tokio::test]
async fn author_stylesheet_cascade_controls_layout() {
    let html = r#"<!doctype html><html><head><style>
        .box { width: 111px; height: 22px; }
        #target { width: 250px !important; }
        #hidden { width: 300px; height: 50px; display: none; }
    </style></head><body>
        <div id="target" class="box" style="width: 222px; height: 33px"></div>
        <div id="hidden"></div>
    </body></html>"#;
    let mut page = Page::from_html_with_url(
        html,
        "https://example.test/cascade",
        Some(browser_oxide::stealth::presets::chrome_148_linux()),
    )
    .await
    .expect("page");

    let actual = page
        .evaluate(
            r#"JSON.stringify({
                target: [
                    document.getElementById('target').clientWidth,
                    document.getElementById('target').clientHeight
                ],
                hidden: [
                    document.getElementById('hidden').offsetWidth,
                    document.getElementById('hidden').offsetHeight
                ]
            })"#,
        )
        .expect("cascade layout probe");

    assert_eq!(actual, r#"{"target":[250,33],"hidden":[0,0]}"#);
}
