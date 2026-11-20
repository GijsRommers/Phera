--TEST--
array-lit
--FILE--
<?php
class A { function __construct(public $n = '') {} function __destruct() { echo "  ~{$this->n}\n"; }
  function hi() { return 1; } static function make(): static { return new static('made'); } function self() { return $this; } }
function f(A $a) { return 1; }
function r() { return new A('ret'); }
function step($s) { echo "$s\n"; }
step('start');
$arr = [new A('al0'), new A('al1')]; $arr = null;
step('end');
--EXPECT--
start
  ~al0
  ~al1
end
