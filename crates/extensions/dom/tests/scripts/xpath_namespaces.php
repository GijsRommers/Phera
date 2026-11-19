<?php

$document = new DOMDocument();
$document->loadXML('<r xmlns="urn:r" xmlns:a="urn:a" xmlns:b="urn:b">'
    . '<p xmlns:a="urn:c" xmlns:c="urn:c2"><u xmlns=""/></p></r>');
$xpath = new DOMXPath($document);
foreach ($xpath->query('//namespace::*') as $node) {
    echo json_encode([$node->parentElement->nodeName, $node->nodeName, $node->nodeValue]), "\n";
}

$xpath->registerNamespace('php', 'http://php.net/xpath');
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
