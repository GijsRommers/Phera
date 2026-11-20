--TEST--
while-cond
--FILE--
<?php
class A { function __construct(public $n = '') {} function __destruct() { echo "  ~{$this->n}\n"; }
  function hi() { return 1; } static function make(): static { return new static('made'); } function self() { return $this; } }
function f(A $a) { return 1; }
function r() { return new A('ret'); }
function step($s) { echo "$s\n"; }
step('start');
$k = 0; while ((new A("w$k"))->hi() && $k++ < 1) { step('body'); }
step('end');
--EXPECT--
start
  ~w0
body
  ~w1
end
