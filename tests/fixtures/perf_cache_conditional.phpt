--TEST--
Performance cache conditional
--FILE--
<?php
if (true) { class Conditional { public $x=3; function get(){return $this->x;} } } $a=new Conditional; echo $a->get(), "\n";
--EXPECT--
3
