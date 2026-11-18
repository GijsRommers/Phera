--TEST--
captured
--FILE--
<?php
class A { function __construct(public $n = '') {} function __destruct() { echo "  ~{$this->n}\n"; }
  function hi() { return 1; } static function make(): static { return new static('made'); } function self() { return $this; } }
function f(A $a) { return 1; }
function r() { return new A('ret'); }
function step($s) { echo "$s\n"; }
step('start');
$o = new A('cap'); $h = function () use ($o) { return 1; }; unset($o); $h(); step('mid'); unset($h);
step('end');
--EXPECT--
start
mid
  ~cap
end
