--TEST--
foreach-closures
--FILE--
<?php
class A { function __construct(public $n = '') {} function __destruct() { echo "  ~{$this->n}\n"; }
  function hi() { return 1; } static function make(): static { return new static('made'); } function self() { return $this; } }
function f(A $a) { return 1; }
function r() { return new A('ret'); }
function step($s) { echo "$s\n"; }
step('start');
foreach ([new A('c1'), new A('c2')] as $f) { } unset($f);
step('end');
--EXPECT--
start
  ~c1
  ~c2
end
