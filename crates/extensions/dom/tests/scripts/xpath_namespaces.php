<?php

$document = new DOMDocument();
$document->loadXML('<r xmlns="urn:r" xmlns:a="urn:a" xmlns:b="urn:b">'
    . '<p xmlns:a="urn:c" xmlns:c="urn:c2"><u xmlns=""/></p></r>');
$xpath = new DOMXPath($document);
foreach ($xpath->query('//namespace::*') as $node) {
    echo json_encode([$node->parentElement->nodeName, $node->nodeName, $node->nodeValue]), "\n";
}

$xpath->registerNamespace('php', 'http://php.net/xpath');
$other = new DOMDocument();
$other->loadXML('<other><entry id="foreign">value</entry></other>');
$entry = $other->documentElement->firstChild;
$xpath->registerPhpFunctions([
    'other' => fn () => $other->documentElement,
    'entry' => fn () => $entry,
]);
$result = $xpath->query('php:function("other")/entry');
echo json_encode([
    $result->length,
    $result[0] === $entry,
    $result[0]->ownerDocument === $other,
]), "\n";
echo json_encode($xpath->evaluate('string(php:function("entry")/@id)')), "\n";
echo json_encode($xpath->evaluate('name(php:function("entry")/parent::*)')), "\n";

$xpath->registerPhpFunctions(['total' => fn ($nodes) => count($nodes)]);
foreach ([
    'name(/*/namespace::*[2])',
    'namespace-uri(/*/namespace::*[2])',
    'count(//namespace::xml)',
    'count(//namespace::node())',
    'number(php:function("total", //namespace::*))',
    'php:functionString("strlen", /*/namespace::xml)',
    'php:function(false())',
    'php:function()',
] as $expression) {
    try {
        echo json_encode($xpath->evaluate($expression)), "\n";
    } catch (Throwable $error) {
        echo json_encode([get_class($error), $error->getMessage()]), "\n";
    }
}
