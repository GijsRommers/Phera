--TEST--
exit-in-function
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
$a = new D("global");
function g() { $local = new D("local"); exit("bye\n"); }
g();
--EXPECT--
bye
~local
~global
