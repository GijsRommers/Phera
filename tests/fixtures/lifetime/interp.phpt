--TEST--
interp
--FILE--
<?php
class A { function __construct(public $n = '') {} function __destruct() { echo "  ~{$this->n}\n"; }
  function hi() { return 1; } static function make(): static { return new static('made'); } function self() { return $this; } }
function f(A $a) { return 1; }
function r() { return new A('ret'); }
function step($s) { echo "$s\n"; }
step('start');
$s = "x{$c2}" . (new A('i'))->hi();
step('end');
--EXPECTF--
start

Warning: Undefined variable $c2 in %s on line 8
  ~i
end
