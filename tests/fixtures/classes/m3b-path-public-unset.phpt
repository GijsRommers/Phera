--TEST--
Class members: path-public-unset
--FILE--
<?php class Foo { public $x=1; function run(){unset($this->x); var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (0) {
}
