//! Compare the modern DOM tree, selectors and live class lists against PHP.

use phera_extension_api::testing::scripts::Comparison;

#[test]
fn modern_dom_matches_php() {
    let Some(oracle) = Comparison::from_environment("dom-modern") else {
        return;
    };
    let script = oracle.artifacts.join("modern.php");
    std::fs::write(&script, MATRIX).expect("write modern DOM oracle script");
    let output = oracle
        .compare(&script, |_, _| {})
        .unwrap_or_else(|error| panic!("{error}; outputs in {}", oracle.artifacts.display()));
    let rows = output.stdout.iter().filter(|&&byte| byte == b'\n').count();
    assert_eq!(rows, 310);
    eprintln!("{rows} modern DOM rows matched");
}

const MATRIX: &str = r#"<?php
function description($element) {
    return $element === null ? null : [
        $element->localName, $element->id, $element->className,
        $element->namespaceURI, get_class($element),
    ];
}

$documents = [
    '<main><section id="a"><item class="x z" id="i">A</item><item class="x" id="j"/>'
        . '<item id="k"><label class="z">B</label></item></section><item id="after"/></main>',
    '<main><section id="a" class="x"><item id="i" data-note="alpha"/>'
        . '<item id="j" class="z" data-note="ALPHA"/><item id="k">C</item></section></main>',
    '<main><section id="a"><item id="i" class="x x z">first</item>'
        . '<other id="j"/><item id="k" lang="en-US"><label>last</label></item></section></main>',
];
$queries = [
    '*', 'section', 'item, label', '#a > item', 'section .x', '#i + item',
    '#i ~ item', 'item:first-child', 'item:last-of-type', 'item:nth-child(2n+1)',
    'item:empty', '.x:not(.z)', ':is(item, label).z', ':where(main, section) > .x',
    'item:has(label)', 'item[id^="i"]', '[data-note="alpha" i]', '.missing',
];
foreach ([false, true] as $html) {
    foreach ($documents as $source) {
        if ($html) {
            $source = '<!doctype html>' . str_replace('/>', '></item>', $source);
            $source = str_replace('<other id="j"></item>', '<other id="j"></other>', $source);
            $document = Dom\HTMLDocument::createFromString($source, LIBXML_NOERROR);
        } else {
            $document = Dom\XMLDocument::createFromString($source);
        }
        $context = $document->querySelector('section');
        foreach ([$document, $context] as $root) {
            foreach ($queries as $query) {
                $all = [];
                foreach ($root->querySelectorAll($query) as $node) {
                    $all[] = description($node);
                }
                echo json_encode([$query, description($root->querySelector($query)), $all]), "\n";
            }
        }
        $tokens = $context->classList;
        foreach (['x', 'ready', 'ready'] as $token) {
            $tokens->add($token);
            echo json_encode([$tokens->value, count($tokens), $tokens->item(0),
                $tokens->contains('ready'), iterator_to_array($tokens), $tokens === $context->classList]),
                "\n";
            echo json_encode([$tokens->toggle($token), $tokens->value]), "\n";
        }
        $tokens->value = 'alpha beta alpha';
        echo json_encode([$tokens->replace('alpha', 'gamma'), $tokens->value]), "\n";
        $tokens->remove('beta');
        echo json_encode([$context->className, $tokens->value]), "\n";
        foreach (['<sample id="changed">A &amp; B</sample>', ' bare <b>text</b>'] as $fragment) {
            $context->innerHTML = $fragment;
            echo json_encode([$context->innerHTML, description($context->firstElementChild)]), "\n";
        }
        echo json_encode([$context->matches('section'), description($context->closest('main'))]), "\n";
        foreach (['', '[', 'div::before'] as $selector) {
            try {
                $context->querySelector($selector);
            } catch (DOMException $error) {
                echo json_encode([$selector, $error->code, $error->getMessage()]), "\n";
            }
        }
        echo json_encode([$document->characterSet, $document->documentElement->nodeName]), "\n";
    }
}
foreach (['<p>A &amp; B</p>', '<!doctype html><p>é<input checked>'] as $source) {
    echo json_encode(Dom\HTMLDocument::createFromString($source, LIBXML_NOERROR)->saveHtml()), "\n";
}
// An HTML collection finds an element by its `id` or `name`; it has no `getNamedItem()`.
$document = Dom\HTMLDocument::createFromString(
    '<!doctype html><p id="first">one</p><p name="second">two</p>',
    LIBXML_NOERROR,
);
$paragraphs = $document->getElementsByTagName('p');
echo json_encode([
    get_class($paragraphs), count($paragraphs),
    $paragraphs->namedItem('first')->textContent, $paragraphs->namedItem('second')->textContent,
    $paragraphs->namedItem('missing'), method_exists($paragraphs, 'getNamedItem'),
]), "\n";
echo json_encode([
    get_class($paragraphs->item(0)->getRootNode()),
    get_class($paragraphs->item(0)->getRootNode(options: [])),
]), "\n";
"#;
