--TEST--
Implicit Stringable participates in method return variance
--FILE--
<?php
class Base { function value(): Stringable { return new Target; } }
class Target extends Base {
    function __toString() { return "text"; }
    function value(): Target { return $this; }
}
echo (new Target)->value();
--EXPECT--
text
