--TEST--
bound
--FILE--
<?php
class A { function __construct(public $n = '') {} function __destruct() { echo "  ~{$this->n}\n"; }
  function hi() { return 1; } static function make(): static { return new static('made'); } function self() { return $this; } }
function f(A $a) { return 1; }
function r() { return new A('ret'); }
function step($s) { echo "$s\n"; }
step('start');
$b = Closure::bind(function () { return $this; }, new A('bd'), A::class); unset($b);
step('end');
--EXPECT--
start
  ~bd
end
