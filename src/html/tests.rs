use super::*;
use scraper::Html;
use serde_json::json;

#[test]
fn extraction_refuses_missing_or_ambiguous_required_fields() {
    let extraction: Extraction = serde_json::from_value(json!({"items":"article","fields":{"title":{"selector":"h2","required":true},"url":{"selector":"a","attribute":"href","absolute_url":true}}})).unwrap();
    assert!(
        extraction
            .extract(
                "<article><h2>One</h2><h2>Two</h2></article>",
                "https://example.test/list"
            )
            .is_none()
    );
    let value = extraction
        .extract(
            "<article><h2>A\n title</h2><a href='javascript:alert(1)'>Link</a></article>",
            "https://example.test/list",
        )
        .unwrap();
    assert_eq!(value["items"][0]["title"], "A title");
    assert!(value["items"][0]["url"].is_null());
    let evidence = structure(
        "<article class='book'><a href='/item'>Read</a><input type='hidden' value='private-value'><script>secret_token='private-value'</script></article>",
    );
    assert!(evidence.to_string().contains("article.book"));
    assert!(!evidence.to_string().contains("private-value"));
    let evidence =
        structure("<article id='12:item' class='sm:wide book.item'><h2>Title</h2></article>");
    for node in evidence.as_array().unwrap() {
        assert!(
            node["matches"].as_u64().unwrap() > 0,
            "{}",
            node["selector"]
        );
    }
    let value = extraction.extract("<article><h2>A\n title<script>private-value</script><span hidden>hidden-value</span><style>private-style</style></h2></article>", "https://example.test/list").unwrap();
    assert_eq!(value["items"][0]["title"], "A title");
    let identity = Field {
        selector: "h2".into(),
        attribute: String::new(),
    };
    assert_eq!(
        identity.extract(&Html::parse_document("<h2>Mina\n Park</h2>")),
        Some("Mina Park".into())
    );
    let long_page = Html::parse_document(&format!("<h2>{}</h2>", "Book title ".repeat(100)));
    assert!(identity.extract(&long_page).is_none());
    assert!(identity.is_present(&long_page));
    assert!(!identity.is_present(&Html::parse_document("<h2>One</h2><h2>Two</h2>")));
}

#[test]
fn fields_keep_values_private_and_refuse_ambiguous_identity() {
    let html = r#"<meta name="current-person" content="person-private"><meta name="csrf-token" content="secret-private"><input name="email" value="private@example.test"><span data-account-id="17"></span>"#;
    let evidence = fields(html).to_string();
    assert!(evidence.contains("current-person"));
    assert!(evidence.contains("data-account-id"));
    for value in [
        "person-private",
        "private@example.test",
        "secret-private",
        "csrf-token",
    ] {
        assert!(!evidence.contains(value));
    }
    let field = Field {
        selector: "meta[name=\"current-person\"]".into(),
        attribute: "content".into(),
    };
    assert_eq!(
        field.extract(&Html::parse_document(html)).as_deref(),
        Some("person-private")
    );
    assert!(
        field
            .extract(&Html::parse_document(&format!("{html}{html}")))
            .is_none()
    );
    assert!(
        field
            .extract(&Html::parse_document(
                "<meta name='current-person' content=''>"
            ))
            .is_none()
    );
}

#[test]
fn ssr_extraction_supports_remix_context_nextjs_flight_and_merged_ld_json() {
    let html = r#"
        <script type="application/ld+json">{"@context":"https://schema.org","@type":"WebSite","potentialAction":{"target":{"urlTemplate":"https://example.com/search?q={q}"}}}</script>
        <script type="application/ld+json">{"@context":"https://schema.org","@type":"ItemList","itemListElement":[{"name":"MacBook Pro"}]}</script>
        <script>window.__remixContext = {"state":{"loaderData":{"root":{"lang":"ko"},"routes/kr.search.buy-sell._index":{"buySellArticles":[{"title":"M3 Max; 16인치"}]}}}}; console.log("done");</script>
        <script>self.__next_f.push([1,"6:[\"$\",\"$L10\",null,{\"children\":[\"$\",\"$L13\",null,{\"data\":[{\"id\":\"p1\",\"title\":\"Hello Velog\"}]}]}]\n"])</script>
    "#;
    let map = extract_ssr_map(html);
    assert!(map.contains_key("__remixContext"));
    assert!(map.contains_key("__NEXT_F__"));
    assert!(map.contains_key("ld+json"));
    assert_eq!(
        extract_ssr_json(html, "__remixContext", "/routeData/buySellArticles/0/title"),
        Some(json!("M3 Max; 16인치"))
    );
    assert_eq!(
        extract_ssr_json(
            html,
            "__remixContext",
            "/state/loaderData/routes/kr.search.buy-sell._index/buySellArticles/0/title"
        ),
        Some(json!("M3 Max; 16인치"))
    );
    assert_eq!(
        extract_ssr_json(html, "__NEXT_F__", "/data/0/title"),
        Some(json!("Hello Velog"))
    );
    assert_eq!(
        extract_ssr_json(html, "ld+json", "/itemListElement/0/name"),
        Some(json!("MacBook Pro"))
    );
}

#[test]
fn infer_html_extraction_and_apollo_state_work() {
    let html = r#"
        <header><nav><a href="/">Home</a><a href="/new">New</a><a href="/show">Show</a><a href="/login">Login</a></nav></header>
        <main>
            <div class='topic_row' data-topic-state-id='101'>
                <div class='topictitle'><a href='https://example.org/a' class='topic-title-link'><h2>First Topic</h2></a></div>
                <div class='topicdesc'><a href='topic?id=101'>Summary one</a></div>
                <div class='topicinfo'><span>10 points</span><time datetime='2026-10-18T00:00:00Z'>1h</time></div>
            </div>
            <div class='topic_row' data-topic-state-id='102'>
                <div class='topictitle'><a href='https://example.org/b' class='topic-title-link'><h2>Second Topic</h2></a></div>
                <div class='topicdesc'><a href='topic?id=102'>Summary two</a></div>
                <div class='topicinfo'><span>20 points</span><time datetime='2026-10-18T01:00:00Z'>2h</time></div>
            </div>
            <div class='topic_row' data-topic-state-id='103'>
                <div class='topictitle'><a href='https://example.org/c' class='topic-title-link'><h2>Third Topic</h2></a></div>
                <div class='topicdesc'><a href='topic?id=103'>Summary three</a></div>
                <div class='topicinfo'><span>30 points</span><time datetime='2026-10-18T02:00:00Z'>3h</time></div>
            </div>
        </main>
        <script>window.__APOLLO_STATE__ = {"ROOT_QUERY":{"searchPosts":{"count":1}},"Post:abc-123":{"id":"abc-123","title":"Rust Guide","url_slug":"rust-guide"}};</script>
    "#;
    let ext = infer_html_extraction(html, "https://news.hada.io/").expect("inferred");
    assert_eq!(ext.items, "div.topic_row");
    let extracted = ext
        .extract(html, "https://news.hada.io/")
        .expect("extracted");
    assert_eq!(extracted["items"][0]["id"], "101");
    assert_eq!(extracted["items"][0]["title"], "First Topic");
    assert_eq!(extracted["items"][0]["url"], "https://example.org/a");
    assert_eq!(extracted["items"][0]["summary"], "Summary one");
    let nav = nav_list_routes(html, "https://news.hada.io/");
    assert_eq!(nav, vec!["/new".to_string(), "/show".to_string()]);
    assert_eq!(
        extract_ssr_json(html, "__ NEXT_F__", "/items/0/title"),
        Some(json!("Rust Guide"))
    );
}
