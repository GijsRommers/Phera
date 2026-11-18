--TEST--
anonymous-classes: explicit outer argument
--FILE--
<?php
class P { public $tag = "outside"; function inner() { return new class($this) { function __construct(public P $outer) {} }; } } echo (new P)->inner()->outer->tag;
--EXPECT--
outside
