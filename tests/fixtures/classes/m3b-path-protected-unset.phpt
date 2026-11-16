--TEST--
Class members: path-protected-unset
--FILE--
<?php class Foo { protected $x=1; function run(){unset($this->x); var_dump($this);} } (new Foo)->run();
--EXPECT--
object(Foo)#1 (0) {
}
