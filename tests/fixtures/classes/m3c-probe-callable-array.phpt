--TEST--
Class conformance: probe-callable-array
--FILE--
<?php class C {function hi(){return "C";}}$f=[new C,"hi"];echo call_user_func($f),$f();
--EXPECT--
CC
