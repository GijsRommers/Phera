--TEST--
Strict native string property assignments reject Stringable objects before calling PHP
--FILE--
<?php
declare(strict_types=1);
class Label {
    public function __toString(): string {
        echo "unexpected conversion\n";
        return 'label';
    }
}
$d = new DOMDocument();
$d->loadXML('<root/>');
foreach (['version' => $d, 'textContent' => $d->documentElement] as $field => $receiver) {
    try {
        $receiver->$field = new Label();
    } catch (TypeError $error) {
        echo $error->getMessage(), "\n";
    }
}
echo $d->saveXML($d->documentElement), "\n";
--EXPECT--
Cannot assign Label to property DOMDocument::$version of type ?string
Cannot assign Label to property DOMNode::$textContent of type string
<root/>
