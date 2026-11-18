--TEST--
An anonymous class becomes Stringable through a trait
--FILE--
<?php
trait DisplayName {
    public function __toString() { return 'label'; }
}
$object = new class { use DisplayName; };
var_dump(class_implements(DisplayName::class), $object instanceof Stringable);
echo $object, "\n";
--EXPECT--
array(0) {
}
bool(true)
label
