<?php

$document = new DOMDocument();
$document->loadXML('<r a="1" b="2"><x/><y/></r>');
foreach ([$document->documentElement->childNodes, $document->documentElement->attributes] as $list) {
    foreach ([-1, 0, 1, 2, 2147483648, 'a', '0', '01', ' 1 ', '1e0', '', null, [], [1], 0.5, true] as $index) {
        $reference =& $index;
        $present = isset($list[$reference]);
        try {
            $result = $list[$reference]?->nodeName;
        } catch (ValueError $error) {
            $result = $error->getMessage();
        }
        echo json_encode([$index, $present, $result]), "\n";
    }
}

try {
    $node = new DOMNode();
    echo $node->nodeName;
} catch (DOMException $error) {
    echo json_encode([$error->code, $error->getCode(), $error->getMessage()]), "\n";
    $error->code = 'updated';
    echo json_encode([$error->code, $error->getCode()]), "\n";
}

foreach ([true, false] as $retainParent) {
    $document->loadXML('<r><parent><child/></parent></r>');
    $child = $document->documentElement->firstChild->firstChild;
    $parent = $retainParent ? $child->parentNode : null;
    $document->documentElement->remove();
    echo json_encode([$retainParent, $child->parentNode?->nodeName]), "\n";
    unset($parent);
    echo json_encode($child->parentNode?->nodeName), "\n";
}
