--TEST--
traits: autoload when matching lazy property defaults
--FILE--
<?php
spl_autoload_register(function($n){echo $n; class D{ const X=1; }});
trait T{public $x=D::X;}
class C{use T; public $x=1;}
echo (new C)->x;
--EXPECT--
D1
