--TEST--
uncaught-exception-from-function
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
$a = new D("global");
function g() { $local = new D("local"); throw new Exception("e"); }
g();
--EXPECTF--
~local

Fatal error: Uncaught Exception: e in %s:4
Stack trace:
#0 %s(5): g()
#1 {main}
  thrown in %s on line 4
~global
