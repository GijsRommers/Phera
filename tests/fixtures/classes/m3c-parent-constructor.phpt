--TEST--
Class conformance: parent-constructor
--FILE--
<?php class P {public $v; function __construct($v){$this->v=$v;}} class D extends P {function __construct(){parent::__construct(9);}} echo (new D)->v;
--EXPECT--
9
