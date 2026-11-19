--TEST--
A restricted object property returns a detached reference
--FILE--
<?php
class Owner {
    public private(set) object $object;
    public function __construct() { $this->object = new stdClass(); }
}
$owner = new Owner();
$alias =& $owner->object;
$alias->number = 2;
$alias = new stdClass();
var_dump($owner->object->number);
--EXPECT--
int(2)
