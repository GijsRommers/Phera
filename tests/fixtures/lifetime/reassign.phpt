--TEST--
reassign
--FILE--
<?php
class A { function __construct(public $n = '') {} function __destruct() { echo "  ~{$this->n}\n"; }
  function hi() { return 1; } static function make(): static { return new static('made'); } function self() { return $this; } }
function f(A $a) { return 1; }
function r() { return new A('ret'); }
function step($s) { echo "$s\n"; }
step('start');
$x = new A('x1'); $x = new A('x2'); step('mid'); unset($x);
step('end');
--EXPECT--
start
  ~x1
mid
  ~x2
end
