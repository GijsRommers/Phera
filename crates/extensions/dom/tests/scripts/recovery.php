<?php

libxml_use_internal_errors(true);
foreach ([
    '<r><x></r>',
    '<r><x></y><q/></r>',
    "<r>\n<x>hi",
    '<r>A&unknown;B&other;<q/></r>',
    '<a/><b/>',
] as $xml) {
    foreach ([0, LIBXML_RECOVER] as $flags) {
        libxml_clear_errors();
        $document = new DOMDocument();
        $loaded = $document->loadXML($xml, $flags);
        $errors = [];
        foreach (libxml_get_errors() as $error) {
            $errors[] = [$error->code, $error->line, $error->column, $error->message];
        }
        echo json_encode([$xml, $flags, $loaded, $document->saveXML(), $errors]), "\n";
    }
}

foreach ([
    '<r> </r>',
    '<r> <x/> </r>',
    '<r>x<a/> <b/> </r>',
    '<r><a/>x<b/> <c/> </r>',
    '<r><![CDATA[x]]> <a/> </r>',
    '<r> &amp;<x/> </r>',
    '<r xml:space="preserve"><p xml:space="default"> <a/> </p> </r>',
] as $xml) {
    $document = new DOMDocument();
    $document->loadXML($xml, LIBXML_NOBLANKS);
    echo json_encode($document->saveXML()), "\n";
}

foreach (['<p>x</p>', '&lt;p>x&lt;/p>', '&#60;p>x&#60;/p>', 'A &amp; B', '&#xD;'] as $value) {
    foreach ([0, LIBXML_NOENT] as $flags) {
        $document = new DOMDocument();
        $loaded = $document->loadXML('<!DOCTYPE r [<!ENTITY e "' . $value . '">]><r>&e;</r>', $flags);
        $root = $document->documentElement;
        echo json_encode([$loaded, $root->textContent, $document->saveXML($root)]), "\n";
    }
}
