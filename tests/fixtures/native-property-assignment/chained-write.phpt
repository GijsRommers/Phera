--TEST--
A write through an object-valued native property updates the returned object
--FILE--
<?php
$document = new DOMDocument();
$document->loadXML('<root/>');

try {
    $document->documentElement->textContent = 'updated';
    echo $document->saveXML($document->documentElement), "\n";
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
<root>updated</root>
