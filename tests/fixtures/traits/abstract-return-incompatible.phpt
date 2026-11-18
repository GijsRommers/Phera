--TEST--
traits: abstract return incompatible
--FILE--
<?php
trait T{abstract function m():int;} class C{use T;function m():string{return "a";}}
--EXPECTF--
Fatal error: Declaration of C::m(): string must be compatible with T::m(): int in /private%s on line 2
