--TEST--
anonymous-classes: method nesting
--FILE--
<?php
class P { function inner() { return new class { function x() { return $this::class; } }; } } echo strstr((new P)->inner()->x(), chr(0), true);
--EXPECT--
class@anonymous
