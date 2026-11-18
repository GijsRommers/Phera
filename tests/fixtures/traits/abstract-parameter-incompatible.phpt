--TEST--
traits: abstract parameter incompatible
--FILE--
<?php
trait T{abstract function m(int $n);} class C{use T;function m(string $n){}}
--EXPECTF--
Fatal error: Declaration of C::m(string $n) must be compatible with T::m(int $n) in %s on line 2
