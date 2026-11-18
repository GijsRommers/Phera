--TEST--
traits: array property default
--FILE--
<?php
trait T {public array $v = ["a" => 2];} class C {use T;} echo (new C)->v["a"];
--EXPECT--
2
