--TEST--
list
--FILE--
<?php
class A { function __construct(public $n = '') {} function __destruct() { echo "  ~{$this->n}\n"; }
  function hi() { return 1; } static function make(): static { return new static('made'); } function self() { return $this; } }
function f(A $a) { return 1; }
function r() { return new A('ret'); }
function step($s) { echo "$s\n"; }
step('start');
[$p, $q] = [new A('p'), new A('q')]; unset($p); step('mid'); $q = 2;
step('end');
--EXPECT--
start
  ~p
mid
  ~q
end
