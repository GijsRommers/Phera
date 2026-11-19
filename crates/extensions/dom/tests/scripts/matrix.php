<?php

function row(string $label, mixed $value): void
{
    echo $label, ' ', json_encode($value, JSON_INVALID_UTF8_SUBSTITUTE), "\n";
}

libxml_use_internal_errors(true);

foreach (['alpha', 'book', 'item', 'café'] as $name) {
    foreach (['', 'x', 'é', 'x&amp;y', '&lt;', '&#x1f680;', "alpha\nbeta", ' &gt; '] as $text) {
        foreach (['', ' a="1"', ' k="é" z="&amp;"', ' xml:id="the-id"'] as $attrs) {
            foreach (['', '<?xml version="1.0" encoding="UTF-8"?>'] as $declaration) {
                $document = new DOMDocument();
                $source = "$declaration<$name$attrs><empty/><value>$text</value></$name>";
                $case = hash('sha256', $source);
                libxml_clear_errors();
                row("$case loadXML", $document->loadXML($source));
                row("$case diagnostics", libxml_get_errors());
                $root = $document->documentElement;
                row("$case documentElement", [
                    $root->nodeName, $root->textContent, $root->getLineNo(),
                ]);
                row("$case element list", [$root->childNodes->length,
                    $document->getElementsByTagName('value')->item(0)->nodeName,
                    isset($root->childNodes[1]), $root->childNodes[1]->nodeName]);
                row("$case unformatted save", $document->saveXML());
                $document->formatOutput = true;
                $flags = LIBXML_NOEMPTYTAG | LIBXML_NOXMLDECL;
                row("$case formatted save", $document->saveXML(null, $flags));
                row("$case subtree save", $document->saveXML($root->lastChild));
                row("$case C14N", $root->C14N());
                row("$case getElementById", $document->getElementById('the-id')?->nodeName);
                $xpath = new DOMXPath($document);
                row("$case XPath count", $xpath->evaluate('count(//value | //empty)'));
                row("$case XPath text", $xpath->evaluate('normalize-space(string(//value))'));
                row("$case XPath step", $xpath->query('//*[position()=2]')->item(0)?->nodeName);
                $list = $root->childNodes;
                $node = $document->createElement('additional');
                $root->appendChild($node);
                row("$case insert", [$list->length, $node === $root->lastChild]);
                $root->removeChild($node);
                row("$case remove", [
                    $list->length, $node->parentNode, $node->ownerDocument === $document,
                ]);
            }
        }
    }
}

$malformed = [
    'plain', '<r>', '<a/><b/>', '<r>&bad;</r>', '', '<r a="1" a="2"/>', "<r>\x01</r>",
];
foreach ($malformed as $input) {
    foreach ([0, LIBXML_NOERROR, LIBXML_NOBLANKS] as $options) {
        $document = new DOMDocument();
        $case = bin2hex($input) . "/$options";
        libxml_clear_errors();
        try {
            row("$case malformed loadXML", $document->loadXML($input, $options));
        } catch (Throwable $error) {
            row("$case malformed exception", [get_class($error), $error->getMessage()]);
        }
        row("$case malformed diagnostics", libxml_get_errors());
    }
}

$document = new DOMDocument();
$document->loadXML('<r xmlns:t="urn:test"><t:item/><t:item id="2"/><x>2</x><x>3</x></r>');
$xpath = new DOMXPath($document);
$xpath->registerNamespace('p', 'urn:test');
$xpath->registerNamespace('php', 'http://php.net/xpath');
$xpath->registerPhpFunctions(['strtoupper', 'count']);
foreach ([
    'count(//p:item[@id="2"])', 'sum(//x)', 'string(//x[2]/preceding-sibling::*[1])',
    'translate("abracadabra","abc","def")', 'substring("12345",1.5,2.5)',
    'boolean(//x[.="3"])', 'php:function("count",//x)',
    'php:functionString("strtoupper","abc")', 'string(//x[php:function("count",.)=1][1])',
] as $expression) {
    row($expression, $xpath->evaluate($expression));
}

foreach ([
    ['ISO-8859-1', "\xe9"],
    ['US-ASCII', 'ascii'],
    ['UTF-8', 'é'],
] as [$encoding, $content]) {
    libxml_clear_errors();
    $document = new DOMDocument();
    $source = '<?xml version="1.0" encoding="' . $encoding . '"?><r>' . $content . '</r>';
    row("$encoding encoded load", $document->loadXML($source));
    row("$encoding encoded document", [
        $document->encoding,
        $document->documentElement?->textContent,
        bin2hex($document->saveXML()),
    ]);
    row("$encoding encoded diagnostics", libxml_get_errors());
}

$document = new DOMDocument();
$document->loadXML('<r xmlns="urn:root" xmlns:p="urn:p"><item/><p:item/><item z="2" a="1"/></r>');
$xpath = new DOMXPath($document);
$xpath->registerNamespace('r', 'urn:root');
row('canonical subtree', $xpath->query('//r:item')->item(1)->C14N());
row('canonical exclusive', $document->C14N(true, false, null, ['p']));
row('canonical XPath subset', $document->C14N(true, false, [
    'query' => '//r:r | //r:item | //namespace::*',
    'namespaces' => ['r' => 'urn:root'],
]));

foreach ([
    ["\xff\xfe", 'UTF-16LE', 'UTF-16'],
    ["\xfe\xff", 'UTF-16BE', 'UTF-16'],
    ['', 'UTF-16LE', 'UTF-16LE'],
] as [$head, $inputEncoding, $declaredEncoding]) {
    $source = '<?xml version="1.0" encoding="' . $declaredEncoding . '"?><r>é</r>';
    $bytes = $head . mb_convert_encoding($source, $inputEncoding, 'UTF-8');
    $document = new DOMDocument();
    libxml_clear_errors();
    row("$inputEncoding load", $document->loadXML($bytes));
    row("$inputEncoding document", [
        $document->encoding,
        $document->documentElement?->textContent,
        bin2hex($document->saveXML()),
    ]);
    row("$inputEncoding diagnostics", libxml_get_errors());
}
