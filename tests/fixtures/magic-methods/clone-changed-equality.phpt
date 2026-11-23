--TEST--
clone changed equality
--FILE--
<?php
class Copy {
    public $value = 1;
    function __clone() { $this->value++; }
}
$original = new Copy;
$copy = clone $original; var_dump($original == $copy, $original === $copy);
--EXPECT--
bool(false)
bool(false)
