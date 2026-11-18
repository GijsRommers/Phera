--TEST--
Sibling clone visibility is resolved against the protected prototype
--FILE--
<?php
abstract class Base { abstract protected function __clone(); }
class Source extends Base { protected function __clone() { echo "clone"; } }
class Caller extends Base {
    protected function __clone() {}
    function duplicate($source) { return clone $source; }
}
(new Caller)->duplicate(new Source);
--EXPECT--
clone
