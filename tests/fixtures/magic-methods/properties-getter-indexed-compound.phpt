--TEST--
Compound assignment to a getter array runs __get once and writes through a reference
--FILE--
<?php
class Items {
    public $values = ['k' => 3];
    function &__get($name) { echo "get $name\n"; return $this->values; }
}
$o = new Items;
var_dump($o->proxy['k'] += 2, $o->values['k']);
--EXPECT--
get proxy
int(5)
int(5)
