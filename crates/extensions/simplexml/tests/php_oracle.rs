//! Compare SimpleXML and shared DOM identity with PHP 8.5.

use phera_extension_api::testing::scripts::Comparison;

#[test]
fn simplexml_matrix() {
    let Some(oracle) = Comparison::from_environment("simplexml") else {
        return;
    };
    let script = oracle.artifacts.join("simplexml_matrix.php");
    std::fs::write(&script, MATRIX).expect("write the XML fixture");
    let matched = oracle
        .compare(&script, |_, _| {})
        .unwrap_or_else(|error| panic!("{error}; outputs in {}", oracle.artifacts.display()));
    let count = matched.stdout.split(|byte| *byte == b'\n').count() - 1;
    assert_eq!(count, 217);
    eprintln!("{count} SimpleXML cases matched");
}

const MATRIX: &str = r#"<?php
$contents = ['', '0', '123', '-2.50', 'text', '&lt;value&gt;', '<![CDATA[cdata]]>', 'a<part/>b'];
foreach (['x', 'title', 'article'] as $name) {
    foreach ($contents as $content) {
        foreach ([0, LIBXML_NOCDATA, LIBXML_NOBLANKS] as $flags) {
            foreach (['', 'hello', '42'] as $attr) {
                $source = '<root xmlns:p="urn:phera"><' . $name . ' id="' . $attr . '">'
                    . $content . '</' . $name . '><' . $name . '/><p:other>ns</p:other></root>';
                $xml = simplexml_load_string($source, options: $flags);
                $view = $xml->$name;
                $first = $view[0];
                $only = new SimpleXMLElement('<single>' . $content . '</single>', $flags);
                $values = [];
                foreach ($view as $itemName => $item) {
                    $values[] = [$itemName, $item->getName(), (string) $item];
                }
                $namespace = $xml->children('urn:phera');
                echo json_encode([
                    $view->getName(), (string) $first, (string) $first['id'], $view->count(),
                    $values, $namespace->getName(), (string) $namespace[0], (bool) $only,
                    (int) $only, (float) $only, (array) $first,
                    $first->getDocNamespaces(), $first->asXML(), $xml->missing[0],
                ]), "\n";
            }
        }
    }
}
$xml = simplexml_load_string('<root xmlns:p="urn:test"><p:item id="a"/></root>');
$dom = dom_import_simplexml($xml);
$dom->setAttribute('updated', 'yes');
$xml->registerXPathNamespace('alternate', 'urn:test');
$selected = $xml->xpath('//alternate:item');
$item = $selected[0];
$converted = dom_import_simplexml($item);
$item['id'] = 'changed';
$item->value = 'value';
$copy = simplexml_import_dom($dom);
echo json_encode([
    (string) $xml['updated'], $converted->getAttribute('id'),
    dom_import_simplexml($copy) === $dom, $copy->asXML(),
]), "\n";
"#;
