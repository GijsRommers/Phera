--TEST--
After unset(), a declared property's next read and write go to __get and __set, from outside and from inside the class
--FILE--
<?php
class C {
    public $p = 1;
    public int $t = 0;
    protected $q = 2;
    private $r = 3;
    function __set($name, $value) { echo "__set $name = ", var_export($value, true), "\n"; }
    function __get($name) { echo "__get $name\n"; return 7; }
    function writeInside() { $this->p = 2; $this->t = 3; $this->q = 4; $this->r = 5; }
    function readInside() { return [$this->p, $this->t, $this->q, $this->r]; }
    function dropAll() { unset($this->p, $this->t, $this->q, $this->r); }
}

$c = new C;
$c->dropAll();
$c->p = 5;
$c->t = 6;
$c->writeInside();
var_dump($c->p);
var_dump($c->readInside());

// A typed property that was never initialized is not "unset": a write fills it.
class D {
    public int $n;
    function __set($name, $value) { echo "__set $name\n"; }
}
$d = new D;
$d->n = 1;
var_dump($d->n);

// Without __set, a write fills an unset property again.
class E { public $p = 1; }
$e = new E;
unset($e->p);
$e->p = 9;
var_dump($e->p);

// In a loop, so that the access sites are warm.
$c2 = new C;
$c2->dropAll();
for ($i = 0; $i < 3; $i++) { $c2->p = $i; }
--EXPECT--
__set p = 5
__set t = 6
__set p = 2
__set t = 3
__set q = 4
__set r = 5
__get p
int(7)
__get p
__get t
__get q
__get r
array(4) {
  [0]=>
  int(7)
  [1]=>
  int(7)
  [2]=>
  int(7)
  [3]=>
  int(7)
}
int(1)
int(9)
__set p = 0
__set p = 1
__set p = 2
