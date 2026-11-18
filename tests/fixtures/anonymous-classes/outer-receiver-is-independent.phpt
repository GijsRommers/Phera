--TEST--
anonymous-classes: outer receiver is independent
--FILE--
<?php
class P { public $tag = "outside"; function inner() { return new class { public $tag = "inside"; function tag() { return $this->tag; } }; } } echo (new P)->inner()->tag();
--EXPECT--
inside
