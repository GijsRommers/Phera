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

#[test]
fn simplexml_mutation_and_lifetime_matrix() {
    let Some(oracle) = Comparison::from_environment("simplexml-mutations") else {
        return;
    };
    let script = oracle.artifacts.join("simplexml_mutations.php");
    std::fs::write(&script, MUTATIONS).expect("write the XML fixture");
    let matched = oracle
        .compare(&script, |_, _| {})
        .unwrap_or_else(|error| panic!("{error}; outputs in {}", oracle.artifacts.display()));
    let count = matched.stdout.split(|byte| *byte == b'\n').count() - 1;
    assert_eq!(count, 52);
    eprintln!("{count} SimpleXML mutation and lifetime cases matched");
}

const MUTATIONS: &str = r#"<?php
foreach (['0', '-4', '12txt', 'abc', '1.2', '1e4', '99999999999999999999', ''] as $text) {
    foreach ([0, 2, -4, 1.2, 'x'] as $other) {
        $x = simplexml_load_string("<r>$text</r>");
        $add = null;
        try { $add = $x + $other; } catch (TypeError $e) { $add = $e->getMessage(); }
        echo json_encode([
            (int) $x, (float) $x, array_sum([$x]), $add, -$x,
            $x == $other, $x == $x[0], $x == simplexml_load_string("<r>$text</r>"),
        ], JSON_PRESERVE_ZERO_FRACTION), "\n";
    }
}
foreach (['<r/>', '<r><item>old</item></r>'] as $source) {
    foreach (['nested', 'attribute', 'append', 'mixed', 'unset'] as $mode) {
        $x = simplexml_load_string($source);
        if ($mode === 'nested') $x->item->child->name = 'a';
        if ($mode === 'attribute') $x->item['id'] = '7';
        if ($mode === 'append') $x->item[]->child = 'b';
        if ($mode === 'mixed') $x->item[]->child->entry[0]->name = 'a';
        if ($mode === 'unset') unset($x->item->missing);
        echo json_encode([$x->asXML(), isset($x->item->missing)]), "\n";
    }
}
$x = simplexml_load_string('<r><i>a</i><i>b</i><i>c</i></r>');
$view = $x->i;
$str = '';
foreach ($view as $node) {
    $str .= (string) $node;
    if ((string) $node === 'a') { unset($x->i[1]); $x->addChild('i', 'd'); }
}
unset($x);
echo json_encode([$str, (string) $view[2], (string) $node]), "\n";
$x = simplexml_load_string('<r xmlns:a="urn:test"/>');
$kid = $x->addChild('other:kid', null, 'urn:test');
$kid->child = 'yes';
echo json_encode([$x->asXML(), (string) $x->children('a', true)->kid->child]), "\n";
"#;

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
