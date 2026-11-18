--TEST--
The called class of an instance method is the class of $this, however it was called
--FILE--
<?php
class A {
    function foo() { echo static::class, " ", get_class($this), " ", self::class, "\n"; }
    static function s() { echo static::class, "\n"; }
}
class B extends A {
    function bar() { A::foo(); parent::foo(); self::foo(); static::foo(); A::s(); parent::s(); B::s(); }
}
class C extends B {}
(new C)->bar();
$f = function () { echo static::class, "\n"; };
$g = Closure::bind($f, new C, A::class);
$g();
$h = Closure::bind($f, null, B::class);
$h();
--EXPECT--
C C A
C C A
C C A
C C A
A
C
B
C
B
