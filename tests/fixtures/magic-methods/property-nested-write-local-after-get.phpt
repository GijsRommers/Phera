--TEST--
A nested write reads its local source after __get and before __set
--FILE--
<?php
class Items {
    function __get($name) { $GLOBALS['value'] = 'get'; return $this; }
    function __set($name, $value) { echo "$name=$value"; $GLOBALS['value'] = 'set'; }
}
$value = 'start';
$items = new Items;
var_dump($items->entries->entry = $value);
--EXPECT--
entry=getstring(3) "set"
