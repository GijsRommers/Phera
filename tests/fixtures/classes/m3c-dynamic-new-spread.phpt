--TEST--
Class conformance: dynamic-new-spread
--FILE--
<?php class C {public $v; function __construct($v){$this->v=$v;}} $n='C';$args=[8];echo (new $n(...$args))->v;
--EXPECT--
8
