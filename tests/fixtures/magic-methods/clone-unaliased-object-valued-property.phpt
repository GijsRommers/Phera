--TEST--
A copied object-valued property can be reassigned once its reference has no alias
--FILE--
<?php
class Holder {
    public $value;
    function &getValue() { return $this->value; }
}
$o = new Holder;
$o->value = (object) ['n' => 1];
$value = $o->getValue();
$c = clone $o;
$c->value = null;
echo $o->value->n;
--EXPECT--
1
