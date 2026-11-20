--TEST--
fatal-error-leaves-fiber-suspended
--FILE--
<?php
// After a fatal error a suspended fiber is not unwound: its finally block
// and the destructors of what it holds do not run.
class T { function __construct(public $n) {} function __destruct() { echo "destruct {$this->n}\n"; } }
$first = new T('first global');
$fiber = new Fiber(function () { $l = new T('fiber local'); try { Fiber::suspend(); } finally { echo "fiber finally\n"; } });
$fiber->start();
$last = new T('last global');
register_shutdown_function(function () { echo "shutdown\n"; });
eval('function twice() {}'); eval('function twice() {}');
echo "end\n";
--EXPECTF--

Fatal error: Cannot redeclare function twice() (previously declared in %s(10) : eval()'d code:1) in %s(10) : eval()'d code on line 1
shutdown
