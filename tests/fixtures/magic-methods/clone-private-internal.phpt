--TEST--
A private clone method can be used from its lexical scope
--FILE--
<?php
class Copy {
    private function __clone() { echo "cloned"; }
    public function copy() { return clone $this; }
}
(new Copy)->copy();
--EXPECT--
cloned
