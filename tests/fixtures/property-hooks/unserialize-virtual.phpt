--TEST--
Unserialize cannot give an inherited virtual property a backing slot
--FILE--
<?php
class P {
    protected int $x { get => 1; }
}
class C extends P {}
var_dump(unserialize('O:1:"C":1:{s:1:"x";i:3;}'));
--EXPECTF--

Warning: unserialize(): Cannot unserialize value for virtual property P::$x in %s on line %d

Warning: unserialize(): Error at offset 19 of 24 bytes in %s on line %d
bool(false)
