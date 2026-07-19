use std::borrow::Cow;
use std::cell::Cell;
use std::collections::BTreeMap;

use html5ever::driver::{ParseOpts, parse_document};
use html5ever::interface::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tokenizer::TokenizerOpts;
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::{Attribute, ExpandedName, QualName};
use lightningcss::stylesheet::{ParserFlags, ParserOptions, StyleAttribute, StyleSheet};
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use oxc_allocator::Allocator;
use oxc_parser::{ParseOptions, Parser};
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use roxmltree::{Document, ParsingOptions};

const MAX_HTML_RECOVERY_WARNING_COUNT: usize = 1_000;
const MAX_XML_NODES: u32 = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StaticSyntaxError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TextKind {
    Css,
    Html,
    JavaScript,
    Json,
    Plain,
    Xml,
}

/// Validates every recognized text file in the resulting site, including
/// unchanged files. HTML5 parse errors are recoverable by definition, so they
/// are returned as a bounded advisory count. All other parser diagnostics are
/// hard failures.
pub(super) fn validate_resulting_site(
    files: &BTreeMap<String, Vec<u8>>,
) -> Result<usize, StaticSyntaxError> {
    let mut html_recovery_warning_count = 0_usize;
    for (path, bytes) in files {
        let Some(kind) = text_kind(path) else {
            continue;
        };
        let text = std::str::from_utf8(bytes).map_err(|_| StaticSyntaxError)?;
        if text.contains('\0') {
            return Err(StaticSyntaxError);
        }
        match kind {
            TextKind::Css => validate_css(path, text)?,
            TextKind::Html => {
                html_recovery_warning_count = html_recovery_warning_count
                    .saturating_add(validate_html(text)?)
                    .min(MAX_HTML_RECOVERY_WARNING_COUNT);
            }
            TextKind::JavaScript => validate_javascript(path, text)?,
            TextKind::Json => {
                serde_json::from_str::<serde_json::Value>(text).map_err(|_| StaticSyntaxError)?;
            }
            TextKind::Plain => {}
            TextKind::Xml => validate_xml(text)?,
        }
    }
    Ok(html_recovery_warning_count)
}

fn validate_html(text: &str) -> Result<usize, StaticSyntaxError> {
    let options = ParseOpts {
        tokenizer: TokenizerOpts {
            exact_errors: true,
            ..TokenizerOpts::default()
        },
        tree_builder: TreeBuilderOpts {
            exact_errors: true,
            ..TreeBuilderOpts::default()
        },
    };
    let parsed = parse_document(HtmlRecoverySink::default(), options).one(text);
    validate_html_node(&parsed.dom)?;
    Ok(parsed.recovery_warning_count)
}

/// Uses RcDom only for HTML5 tree-construction state. Parser diagnostics are
/// reduced to a saturating count at the callback boundary, so neither raw
/// diagnostics nor an attacker-controlled number of error strings is retained.
struct HtmlRecoverySink {
    dom: RcDom,
    recovery_warning_count: Cell<usize>,
}

struct ParsedHtml {
    dom: Handle,
    recovery_warning_count: usize,
}

impl Default for HtmlRecoverySink {
    fn default() -> Self {
        Self {
            dom: RcDom::default(),
            recovery_warning_count: Cell::new(0),
        }
    }
}

impl TreeSink for HtmlRecoverySink {
    type Handle = Handle;
    type Output = ParsedHtml;
    type ElemName<'a>
        = ExpandedName<'a>
    where
        Self: 'a;

    fn finish(self) -> Self::Output {
        ParsedHtml {
            dom: self.dom.document,
            recovery_warning_count: self.recovery_warning_count.get(),
        }
    }

    fn parse_error(&self, _: Cow<'static, str>) {
        self.recovery_warning_count.set(
            self.recovery_warning_count
                .get()
                .saturating_add(1)
                .min(MAX_HTML_RECOVERY_WARNING_COUNT),
        );
    }

    fn get_document(&self) -> Self::Handle {
        self.dom.get_document()
    }

    fn elem_name<'a>(&'a self, target: &'a Self::Handle) -> Self::ElemName<'a> {
        self.dom.elem_name(target)
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> Self::Handle {
        self.dom.create_element(name, attrs, flags)
    }

    fn create_comment(&self, text: StrTendril) -> Self::Handle {
        self.dom.create_comment(text)
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> Self::Handle {
        self.dom.create_pi(target, data)
    }

    fn append(&self, parent: &Self::Handle, child: NodeOrText<Self::Handle>) {
        self.dom.append(parent, child);
    }

    fn append_based_on_parent_node(
        &self,
        element: &Self::Handle,
        prev_element: &Self::Handle,
        child: NodeOrText<Self::Handle>,
    ) {
        self.dom
            .append_based_on_parent_node(element, prev_element, child);
    }

    fn append_doctype_to_document(
        &self,
        name: StrTendril,
        public_id: StrTendril,
        system_id: StrTendril,
    ) {
        self.dom
            .append_doctype_to_document(name, public_id, system_id);
    }

    fn get_template_contents(&self, target: &Self::Handle) -> Self::Handle {
        self.dom.get_template_contents(target)
    }

    fn same_node(&self, x: &Self::Handle, y: &Self::Handle) -> bool {
        self.dom.same_node(x, y)
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.dom.set_quirks_mode(mode);
    }

    fn append_before_sibling(&self, sibling: &Self::Handle, new_node: NodeOrText<Self::Handle>) {
        self.dom.append_before_sibling(sibling, new_node);
    }

    fn add_attrs_if_missing(&self, target: &Self::Handle, attrs: Vec<Attribute>) {
        self.dom.add_attrs_if_missing(target, attrs);
    }

    fn remove_from_parent(&self, target: &Self::Handle) {
        self.dom.remove_from_parent(target);
    }

    fn reparent_children(&self, node: &Self::Handle, new_parent: &Self::Handle) {
        self.dom.reparent_children(node, new_parent);
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &Self::Handle) -> bool {
        self.dom.is_mathml_annotation_xml_integration_point(handle)
    }

    fn maybe_clone_an_option_into_selectedcontent(&self, option: &Self::Handle) {
        self.dom.maybe_clone_an_option_into_selectedcontent(option);
    }
}

fn validate_html_node(node: &Handle) -> Result<(), StaticSyntaxError> {
    if let NodeData::Element { name, attrs, .. } = &node.data {
        let attrs = attrs.borrow();
        if let Some(style) = attribute_value(&attrs, "style") {
            validate_style_attribute(style)?;
        }
        if name.local.as_ref().eq_ignore_ascii_case("style") {
            validate_css("<inline-style>", &text_content(node))?;
        } else if name.local.as_ref().eq_ignore_ascii_case("script")
            && attribute_value(&attrs, "src").is_none()
        {
            match inline_script_kind(&attrs) {
                InlineScriptKind::Classic => {
                    validate_javascript_source(&text_content(node), SourceType::script())?;
                }
                InlineScriptKind::Module => {
                    validate_javascript_source(&text_content(node), SourceType::mjs())?;
                }
                InlineScriptKind::Inert => {}
            }
        }
    }
    for child in node.children.borrow().iter() {
        validate_html_node(child)?;
    }
    Ok(())
}

fn text_content(node: &Handle) -> String {
    let mut text = String::new();
    for child in node.children.borrow().iter() {
        if let NodeData::Text { contents } = &child.data {
            text.push_str(contents.borrow().as_ref());
        }
    }
    text
}

fn attribute_value<'a>(attrs: &'a [Attribute], local_name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|attribute| {
            attribute
                .name
                .local
                .as_ref()
                .eq_ignore_ascii_case(local_name)
        })
        .map(|attribute| attribute.value.as_ref())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InlineScriptKind {
    Classic,
    Module,
    Inert,
}

fn inline_script_kind(attrs: &[Attribute]) -> InlineScriptKind {
    inline_script_kind_from_values(
        attribute_value(attrs, "type"),
        attribute_value(attrs, "language"),
    )
}

fn inline_script_kind_from_values(
    script_type: Option<&str>,
    language: Option<&str>,
) -> InlineScriptKind {
    if let Some(script_type) = script_type {
        let script_type = script_type.trim();
        if script_type.is_empty() || javascript_mime_type(script_type) {
            InlineScriptKind::Classic
        } else if script_type.eq_ignore_ascii_case("module") {
            InlineScriptKind::Module
        } else {
            InlineScriptKind::Inert
        }
    } else if let Some(language) = language {
        let language = language.trim();
        if language.is_empty() || javascript_mime_type(&format!("text/{language}")) {
            InlineScriptKind::Classic
        } else {
            InlineScriptKind::Inert
        }
    } else {
        InlineScriptKind::Classic
    }
}

fn javascript_mime_type(value: &str) -> bool {
    let essence = value.split(';').next().unwrap_or_default().trim();
    [
        "application/ecmascript",
        "application/javascript",
        "application/x-ecmascript",
        "application/x-javascript",
        "text/ecmascript",
        "text/javascript",
        "text/javascript1.0",
        "text/javascript1.1",
        "text/javascript1.2",
        "text/javascript1.3",
        "text/javascript1.4",
        "text/javascript1.5",
        "text/jscript",
        "text/livescript",
        "text/x-ecmascript",
        "text/x-javascript",
    ]
    .iter()
    .any(|mime_type| essence.eq_ignore_ascii_case(mime_type))
}

fn validate_css(path: &str, text: &str) -> Result<(), StaticSyntaxError> {
    if !css_delimiters_are_closed(text) {
        return Err(StaticSyntaxError);
    }
    StyleSheet::parse(
        text,
        ParserOptions {
            filename: path.to_owned(),
            error_recovery: false,
            flags: ParserFlags::NESTING,
            ..ParserOptions::default()
        },
    )
    .map(|_| ())
    .map_err(|_| StaticSyntaxError)
}

fn validate_style_attribute(text: &str) -> Result<(), StaticSyntaxError> {
    if !css_delimiters_are_closed(text) {
        return Err(StaticSyntaxError);
    }
    StyleAttribute::parse(
        text,
        ParserOptions {
            filename: "<style-attribute>".to_owned(),
            error_recovery: false,
            ..ParserOptions::default()
        },
    )
    .map(|_| ())
    .map_err(|_| StaticSyntaxError)
}

fn css_delimiters_are_closed(text: &str) -> bool {
    let mut delimiters = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    let mut in_comment = false;
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if in_comment {
            if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                in_comment = false;
                index += 2;
                continue;
            }
        } else if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
        } else if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            in_comment = true;
            index += 2;
            continue;
        } else if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
        } else if matches!(byte, b'{' | b'[' | b'(') {
            delimiters.push(byte);
        } else if matches!(byte, b'}' | b']' | b')') {
            let expected = match byte {
                b'}' => b'{',
                b']' => b'[',
                b')' => b'(',
                _ => unreachable!(),
            };
            if delimiters.pop() != Some(expected) {
                return false;
            }
        }
        index += 1;
    }
    delimiters.is_empty() && quote.is_none() && !in_comment && !escaped
}

fn validate_javascript(path: &str, text: &str) -> Result<(), StaticSyntaxError> {
    let source_type = match extension(path) {
        Some(extension) if extension.eq_ignore_ascii_case("mjs") => SourceType::mjs(),
        Some(extension) if extension.eq_ignore_ascii_case("cjs") => SourceType::cjs(),
        Some(extension) if extension.eq_ignore_ascii_case("js") => SourceType::unambiguous(),
        _ => return Err(StaticSyntaxError),
    };
    validate_javascript_source(text, source_type)
}

fn validate_javascript_source(
    text: &str,
    source_type: SourceType,
) -> Result<(), StaticSyntaxError> {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, text, source_type)
        .with_options(ParseOptions {
            parse_regular_expression: true,
            ..ParseOptions::default()
        })
        .parse();
    if parsed.panicked || !parsed.diagnostics.is_empty() {
        return Err(StaticSyntaxError);
    }
    let semantic = SemanticBuilder::new_compiler().build(&parsed.program);
    if semantic.diagnostics.is_empty() {
        Ok(())
    } else {
        Err(StaticSyntaxError)
    }
}

fn validate_xml(text: &str) -> Result<(), StaticSyntaxError> {
    let document = Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: MAX_XML_NODES,
            entity_resolver: None,
        },
    )
    .map_err(|_| StaticSyntaxError)?;
    validate_xml_embedded_languages(&document)
}

fn validate_xml_embedded_languages(document: &Document<'_>) -> Result<(), StaticSyntaxError> {
    const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
    const XHTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";
    const XLINK_NAMESPACE: &str = "http://www.w3.org/1999/xlink";

    for node in document.descendants().filter(roxmltree::Node::is_element) {
        let namespace = node.tag_name().namespace();
        if !matches!(namespace, Some(SVG_NAMESPACE | XHTML_NAMESPACE)) {
            continue;
        }
        if let Some(style) = xml_attribute_value(node, "style") {
            validate_style_attribute(style)?;
        }
        let local_name = node.tag_name().name();
        if local_name.eq_ignore_ascii_case("style") {
            validate_css("<inline-xml-style>", &xml_text_content(node)?)?;
        } else if local_name.eq_ignore_ascii_case("script") {
            let external = if namespace == Some(SVG_NAMESPACE) {
                xml_attribute_value(node, "href").is_some()
                    || node.attribute((XLINK_NAMESPACE, "href")).is_some()
            } else {
                xml_attribute_value(node, "src").is_some()
            };
            if !external {
                match inline_script_kind_from_values(
                    xml_attribute_value(node, "type"),
                    xml_attribute_value(node, "language"),
                ) {
                    InlineScriptKind::Classic => {
                        validate_javascript_source(&xml_text_content(node)?, SourceType::script())?
                    }
                    InlineScriptKind::Module => {
                        validate_javascript_source(&xml_text_content(node)?, SourceType::mjs())?
                    }
                    InlineScriptKind::Inert => {}
                }
            }
        }
    }
    Ok(())
}

fn xml_attribute_value<'a>(node: roxmltree::Node<'a, '_>, local_name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|attribute| attribute.namespace().is_none() && attribute.name() == local_name)
        .map(|attribute| attribute.value())
}

fn xml_text_content(node: roxmltree::Node<'_, '_>) -> Result<String, StaticSyntaxError> {
    let mut text = String::new();
    for child in node.children() {
        if let Some(value) = child.text() {
            text.push_str(value);
        } else if child.is_element() {
            return Err(StaticSyntaxError);
        }
    }
    Ok(text)
}

fn text_kind(path: &str) -> Option<TextKind> {
    let extension = extension(path)?;
    if matches_ignore_ascii_case(extension, &["html", "htm"]) {
        Some(TextKind::Html)
    } else if extension.eq_ignore_ascii_case("css") {
        Some(TextKind::Css)
    } else if matches_ignore_ascii_case(extension, &["js", "mjs", "cjs"]) {
        Some(TextKind::JavaScript)
    } else if matches_ignore_ascii_case(extension, &["json", "map", "webmanifest"]) {
        Some(TextKind::Json)
    } else if matches_ignore_ascii_case(extension, &["svg", "xml"]) {
        Some(TextKind::Xml)
    } else if matches_ignore_ascii_case(extension, &["txt", "csv"]) {
        Some(TextKind::Plain)
    } else {
        None
    }
}

fn extension(path: &str) -> Option<&str> {
    path.rsplit_once('.').map(|(_, extension)| extension)
}

fn matches_ignore_ascii_case(value: &str, choices: &[&str]) -> bool {
    choices
        .iter()
        .any(|choice| value.eq_ignore_ascii_case(choice))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_index() -> Vec<u8> {
        br#"<!doctype html><html lang="en"><head><meta charset="utf-8"><title>LP</title><style>main { color: rebeccapurple; }</style></head><body><main style="text-wrap: balance">Ready</main><script>globalThis.ready = true;</script><script type="module">export const ready = true;</script></body></html>"#.to_vec()
    }

    #[test]
    fn accepts_valid_modern_static_site_syntax() {
        let files = BTreeMap::from([
            ("index.html".to_owned(), valid_index()),
            (
                "styles.css".to_owned(),
                b"@layer base { .card { color: color-mix(in srgb, red 40%, blue); & > strong { text-wrap: balance; } } }"
                    .to_vec(),
            ),
            (
                "app.mjs".to_owned(),
                br#"class View { title = "ready"; render = () => this.title?.toUpperCase() ?? ""; } const route = /^(home|about)$/u; await Promise.resolve(route.test(new View().render())); export { View };"#
                    .to_vec(),
            ),
            (
                "worker.cjs".to_owned(),
                br#"const value = { nested: { ready: true } }; module.exports = value.nested?.ready ?? false;"#
                    .to_vec(),
            ),
            (
                "icon.svg".to_owned(),
                br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><path d="M0 0h10v10z"/></svg>"#
                    .to_vec(),
            ),
            (
                "feed.xml".to_owned(),
                br#"<?xml version="1.0"?><feed xmlns="urn:example"><title>Ready</title></feed>"#
                    .to_vec(),
            ),
            ("data.json".to_owned(), br#"{"ready":true}"#.to_vec()),
        ]);

        assert_eq!(validate_resulting_site(&files), Ok(0));
    }

    #[test]
    fn html5_recovery_is_a_bounded_advisory() {
        let files = BTreeMap::from([(
            "index.htm".to_owned(),
            b"<main><strong>recover me</main></strong>".to_vec(),
        )]);

        let warning_count = validate_resulting_site(&files).unwrap();
        assert!(warning_count > 0);
        assert!(warning_count <= MAX_HTML_RECOVERY_WARNING_COUNT);
    }

    #[test]
    fn invalid_inline_classic_and_module_javascript_are_hard_failures() {
        for script in [
            "<script>function {</script>",
            "<script type=\"module\">export function {</script>",
        ] {
            let html = format!(
                "<!doctype html><html lang=\"en\"><head><title>LP</title></head><body>{script}</body></html>"
            );
            let files = BTreeMap::from([("index.html".to_owned(), html.into_bytes())]);
            assert_eq!(validate_resulting_site(&files), Err(StaticSyntaxError));
        }
    }

    #[test]
    fn inert_inline_json_script_types_are_not_misparsed_as_javascript() {
        let files = BTreeMap::from([(
            "index.html".to_owned(),
            br#"<!doctype html><html lang="en"><head><title>LP</title><script type="application/json">{"source":"function {"}</script><script type="application/ld+json">{"@context":"https://schema.org"}</script><script type="importmap">{"imports":{}}</script></head><body></body></html>"#
                .to_vec(),
        )]);

        assert_eq!(validate_resulting_site(&files), Ok(0));
    }

    #[test]
    fn invalid_inline_style_body_and_attribute_are_hard_failures() {
        for html in [
            r#"<!doctype html><html lang="en"><head><title>LP</title><style>.card { color red; }</style></head><body></body></html>"#,
            r#"<!doctype html><html lang="en"><head><title>LP</title></head><body><main style="color red">LP</main></body></html>"#,
        ] {
            let files = BTreeMap::from([("index.html".to_owned(), html.as_bytes().to_vec())]);
            assert_eq!(validate_resulting_site(&files), Err(StaticSyntaxError));
        }
    }

    #[test]
    fn javascript_parser_and_semantic_diagnostics_are_hard_failures() {
        for path in ["broken.js", "broken.mjs", "broken.cjs"] {
            let files = BTreeMap::from([
                ("index.html".to_owned(), valid_index()),
                (path.to_owned(), b"function {".to_vec()),
            ]);
            assert_eq!(validate_resulting_site(&files), Err(StaticSyntaxError));
        }

        let early_error = BTreeMap::from([
            ("index.html".to_owned(), valid_index()),
            (
                "early.js".to_owned(),
                b"const duplicate = 1; const duplicate = 2;".to_vec(),
            ),
        ]);
        assert_eq!(
            validate_resulting_site(&early_error),
            Err(StaticSyntaxError)
        );
    }

    #[test]
    fn balanced_but_invalid_css_is_a_hard_failure() {
        let files = BTreeMap::from([
            ("index.html".to_owned(), valid_index()),
            ("styles.css".to_owned(), b".card { color red; }".to_vec()),
        ]);

        assert_eq!(validate_resulting_site(&files), Err(StaticSyntaxError));
    }

    #[test]
    fn xml_and_svg_are_strictly_parsed() {
        for (path, content) in [
            ("mismatch.xml", "<root><child></root>"),
            (
                "unknown-prefix.svg",
                "<svg xmlns=\"http://www.w3.org/2000/svg\"><x:path/></svg>",
            ),
        ] {
            let files = BTreeMap::from([
                ("index.html".to_owned(), valid_index()),
                (path.to_owned(), content.as_bytes().to_vec()),
            ]);
            assert_eq!(validate_resulting_site(&files), Err(StaticSyntaxError));
        }
    }

    #[test]
    fn svg_and_namespaced_xml_embedded_languages_are_hard_validated() {
        for (path, content) in [
            (
                "script.svg",
                r#"<svg xmlns="http://www.w3.org/2000/svg"><script>function {</script></svg>"#,
            ),
            (
                "style.svg",
                r#"<svg xmlns="http://www.w3.org/2000/svg"><style>.card { color red; }</style></svg>"#,
            ),
            (
                "foreign-href.svg",
                r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:f="urn:foreign"><script f:href="app.js">function {</script></svg>"#,
            ),
            (
                "attribute.svg",
                r#"<svg xmlns="http://www.w3.org/2000/svg"><path style="color red"/></svg>"#,
            ),
            (
                "embedded-svg.xml",
                r#"<root xmlns:s="http://www.w3.org/2000/svg"><s:svg><s:script type="module">export function {</s:script></s:svg></root>"#,
            ),
            (
                "xhtml.xml",
                r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><script>function {</script></body></html>"#,
            ),
        ] {
            let files = BTreeMap::from([
                ("index.html".to_owned(), valid_index()),
                (path.to_owned(), content.as_bytes().to_vec()),
            ]);
            assert_eq!(validate_resulting_site(&files), Err(StaticSyntaxError));
        }
    }

    #[test]
    fn generic_xml_namespaces_do_not_activate_by_local_name() {
        let files = BTreeMap::from([
            ("index.html".to_owned(), valid_index()),
            (
                "generic.xml".to_owned(),
                br#"<root xmlns="urn:example" style="color red"><script>function {</script><style>.card { color red; }</style></root>"#
                    .to_vec(),
            ),
        ]);

        assert_eq!(validate_resulting_site(&files), Ok(0));
    }

    #[test]
    fn unchanged_recognized_text_is_part_of_the_resulting_site_gate() {
        let files = BTreeMap::from([
            ("index.html".to_owned(), valid_index()),
            ("unchanged.js".to_owned(), b"function {".to_vec()),
        ]);

        assert_eq!(validate_resulting_site(&files), Err(StaticSyntaxError));
    }

    #[test]
    fn json_utf8_and_nul_guards_remain_hard_failures() {
        for files in [
            BTreeMap::from([
                ("index.html".to_owned(), valid_index()),
                ("bad.json".to_owned(), b"{]".to_vec()),
            ]),
            BTreeMap::from([
                ("index.html".to_owned(), valid_index()),
                ("bad.txt".to_owned(), b"before\0after".to_vec()),
            ]),
            BTreeMap::from([
                ("index.html".to_owned(), valid_index()),
                ("bad.csv".to_owned(), vec![0xff]),
            ]),
        ] {
            assert_eq!(validate_resulting_site(&files), Err(StaticSyntaxError));
        }
    }
}
