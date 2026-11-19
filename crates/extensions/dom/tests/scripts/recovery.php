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
