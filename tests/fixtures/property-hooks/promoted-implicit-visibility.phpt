--TEST--
A constructor parameter with hooks promotes without a visibility modifier
--FILE--
<?php
class C {
    public function __construct($length { set => $value * 2; }) {}
}
echo (new C(5))->length, "\n";
--EXPECT--
10
