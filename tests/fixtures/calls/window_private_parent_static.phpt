--TEST--
Call window: private parent static
--FILE--
<?php
class A{private function p($x){return $x+1;}function f($x){return $this->p($x);}static function s($x){return static::class.':'.$x;}}class B extends A{function f($x){return parent::f($x)+1;}static function t(){return parent::s(3);}}echo (new B)->f(4),' ',B::t();
--EXPECT--
6 B:3
