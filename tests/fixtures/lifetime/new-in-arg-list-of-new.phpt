--TEST--
new-in-arg-list-of-new
--FILE--
<?php
class A { function __construct(public $n = '') {} function __destruct() { echo "  ~{$this->n}\n"; }
  function hi() { return 1; } static function make(): static { return new static('made'); } function self() { return $this; } }
function f(A $a) { return 1; }
function r() { return new A('ret'); }
function step($s) { echo "$s\n"; }
step('start');
$w = new A((new A('inner'))->n); unset($w);
step('end');
--EXPECT--
start
  ~inner
  ~inner
end
