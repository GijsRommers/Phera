--TEST--
Class members: path-private-unset
--FILE--
<?php class Foo { private $x=1; function run(){unset($this->x); var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (0) {
}
