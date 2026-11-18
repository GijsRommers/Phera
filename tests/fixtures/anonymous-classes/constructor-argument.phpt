--TEST--
anonymous-classes: constructor argument
--FILE--
<?php
echo (new class(17) { public $p; function __construct($p) { $this->p = $p; } })->p;
--EXPECT--
17
