--TEST--
gc-collects-object-cycles
--FILE--
<?php
class D { public $other; function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
$a = new stdClass; $b = new stdClass; $a->o = $b; $b->o = $a; unset($a, $b);
var_dump(gc_collect_cycles());
$o = new stdClass; $o->list = [$o]; unset($o); var_dump(gc_collect_cycles());
$d = new D('d1'); $e = new D('d2'); $d->other = $e; $e->other = $d; unset($d, $e);
echo "before\n"; var_dump(gc_collect_cycles()); echo "after\n";
class R { public $self; function __destruct() { echo "~R\n"; $GLOBALS['keep'] = $this; } }
$r = new R; $r->self = $r; unset($r); var_dump(gc_collect_cycles()); var_dump(isset($keep)); unset($keep); var_dump(gc_collect_cycles());
--EXPECT--
int(2)
int(2)
before
~d1
~d2
int(2)
after
~R
int(0)
bool(true)
int(1)
