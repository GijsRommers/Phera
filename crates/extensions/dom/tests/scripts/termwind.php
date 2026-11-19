<?php

// Observe the DOM calls made by Termwind's HtmlRenderer and its Node wrapper.
function observe(DOMNode $node): array
{
    $previous = $node->previousSibling;
    $next = $node->nextSibling;
    $result = [
        $node->nodeName,
        $node instanceof DOMText,
        $node instanceof DOMComment,
        $node->nodeValue,
        $node instanceof DOMElement ? $node->getAttribute('class') : '',
        $node instanceof DOMElement ? $node->getAttribute('href') : '',
        $previous?->nodeName,
        $next?->nodeName,
    ];
    $children = [];
    $xml = '';
    foreach ($node->childNodes as $child) {
        $children[] = observe($child);
        if ($child->ownerDocument instanceof DOMDocument) {
            $xml .= $child->ownerDocument->saveXML($child);
        }
    }
    $result[] = $children;
    $result[] = html_entity_decode($xml);
    return $result;
}

foreach ([
    '<div class="font-bold">Laravel <span>13</span></div>',
    '<p><a href="/help?x=1&amp;y=2">Read &amp; learn</a><!-- ignored --><br>Here</p>',
    '<table><tr><td>one</td><td>two</td></tr></table>',
    '<pre><code>café &lt;tag&gt;</code></pre>',
    '<ul><li><strong>List</strong> item</li><li>second</li></ul>',
] as $fragment) {
    $document = new DOMDocument();
    $source = '<?xml encoding="UTF-8"><!DOCTYPE html><html><body>'
        . trim($fragment) . '</body></html>';
    $ok = $document->loadHTML(
        $source,
        LIBXML_NOERROR | LIBXML_COMPACT | LIBXML_HTML_NODEFDTD | LIBXML_NOBLANKS | LIBXML_NOXMLDECL,
    );
    echo json_encode([$ok, observe($document->getElementsByTagName('body')->item(0))]), "\n";
}
