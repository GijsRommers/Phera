--TEST--
arith
--FILE--
<?php
class A { function __construct(public $n = '') {} function __destruct() { echo "  ~{$this->n}\n"; }
  function hi() { return 1; } static function make(): static { return new static('made'); } function self() { return $this; } }
function f(A $a) { return 1; }
function r() { return new A('ret'); }
function step($s) { echo "$s\n"; }
step('start');
$z = (new A('a1'))->hi() + (new A('a2'))->hi();
step('end');
--EXPECT--
start
  ~a1
  ~a2
end
