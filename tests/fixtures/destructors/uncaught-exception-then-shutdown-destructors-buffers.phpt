--TEST--
uncaught-exception-then-shutdown-destructors-buffers
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "d{$this->n}\n"; if ($this->n === 'g2') { $GLOBALS['late'] = new D('late'); new D('tmp'); } } }
register_shutdown_function(function () { echo "shutdown\n"; });
ob_start(function ($s) { return "[ob:" . $s . "]"; });
$a = new D('g1');
$b = new D('g2');
$shared = new D('shared'); $alias = $shared;
$arr = [new D('inarr1'), new D('inarr2')];
$c = new D('g3');
$cyc1 = new D('cyc1'); $cyc2 = new D('cyc2'); $cyc1->o = $cyc2; $cyc2->o = $cyc1; unset($cyc1, $cyc2);
$r = new D('ref'); $rr = &$r;
function f() { static $s; $s = new D('static'); } f();
class S { static $p; } S::$p = new D('staticprop');
echo "end\n"; throw new Exception("boom");
--EXPECTF--
[ob:
Deprecated: Creation of dynamic property D::$o is deprecated in %s on line 10

Deprecated: Creation of dynamic property D::$o is deprecated in %s on line 10
end

Fatal error: Uncaught Exception: boom in %s:14
Stack trace:
#0 {main}
  thrown in %s on line 14
shutdown
dg3
dg2
dtmp
dg1
dlate
dshared
dinarr1
dinarr2
dcyc1
dcyc2
dref
dstatic
dstaticprop
]
