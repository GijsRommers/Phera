--TEST--
gc-functions
--FILE--
<?php
foreach (['gc_enabled','gc_enable','gc_disable','gc_mem_caches','gc_collect_cycles','gc_status'] as $f) { echo $f, ' ', var_export(function_exists($f), true), ' ', var_export(is_callable($f), true), "\n"; }
var_dump(gc_enabled()); var_dump(gc_disable()); var_dump(gc_enabled()); var_dump(gc_enable()); var_dump(gc_enabled());
var_dump(is_int(gc_mem_caches())); var_dump(gc_collect_cycles());
var_dump(array_keys(gc_status()));
$f = 'gc_enabled'; var_dump($f()); var_dump(call_user_func('gc_enabled'));
try { gc_enabled(1); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
$r = new ReflectionFunction('gc_enabled'); var_dump($r->getNumberOfParameters(), $r->isInternal());
--EXPECT--
gc_enabled true true
gc_enable true true
gc_disable true true
gc_mem_caches true true
gc_collect_cycles true true
gc_status true true
bool(true)
NULL
bool(false)
NULL
bool(true)
bool(true)
int(0)
array(12) {
  [0]=>
  string(7) "running"
  [1]=>
  string(9) "protected"
  [2]=>
  string(4) "full"
  [3]=>
  string(4) "runs"
  [4]=>
  string(9) "collected"
  [5]=>
  string(9) "threshold"
  [6]=>
  string(11) "buffer_size"
  [7]=>
  string(5) "roots"
  [8]=>
  string(16) "application_time"
  [9]=>
  string(14) "collector_time"
  [10]=>
  string(15) "destructor_time"
  [11]=>
  string(9) "free_time"
}
bool(true)
bool(true)
gc_enabled() expects exactly 0 arguments, 1 given
int(0)
bool(true)
