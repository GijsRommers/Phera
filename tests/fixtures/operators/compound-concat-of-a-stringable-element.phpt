--TEST--
Compound concatenation of a Stringable object held in a variable, an array element and a property
--FILE--
<?php
class S { public function __toString(): string { return 'S'; } }
$v = new S; $v .= 'x'; var_dump($v);
$a = ['n' => new S]; $a['n'] .= 'y'; var_dump($a['n']);
class H { public $p; } $h = new H; $h->p = new S; $h->p .= 'z'; var_dump($h->p);

--EXPECTF--
string(2) "Sx"
string(2) "Sy"
string(2) "Sz"
