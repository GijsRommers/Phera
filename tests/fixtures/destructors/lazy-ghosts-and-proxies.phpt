--TEST--
lazy-ghosts-and-proxies
--FILE--
<?php
// PHP calls no destructor on a lazy object that was never initialized, and
// none on a proxy: only on its real instance, when that goes.
class D { public $n = 'real'; function __construct() { echo "construct\n"; } function __destruct() { echo "destruct ", $this->n, "\n"; } }
$r = new ReflectionClass(D::class);
echo "-- ghost, never initialized\n";
$g = $r->newLazyGhost(function (D $o) { echo "init ghost\n"; $o->n = 'ghost'; });
unset($g);
echo "-- ghost, initialized\n";
$g = $r->newLazyGhost(function (D $o) { echo "init ghost\n"; $o->n = 'ghost'; });
echo $g->n, "\n";
unset($g);
echo "-- proxy, never initialized\n";
$p = $r->newLazyProxy(function (D $o) { echo "init proxy\n"; $real = new D; $real->n = 'proxied'; return $real; });
unset($p);
echo "-- proxy, initialized\n";
$p = $r->newLazyProxy(function (D $o) { echo "init proxy\n"; $real = new D; $real->n = 'proxied'; return $real; });
echo $p->n, "\n";
unset($p);
echo "-- proxy initialized, real instance kept elsewhere\n";
$keep = null;
$p = $r->newLazyProxy(function (D $o) use (&$keep) { $keep = new D; $keep->n = 'kept'; return $keep; });
echo $p->n, "\n";
unset($p);
echo "after unset\n";
$keep = null;
echo "-- ghost marked initialized without initializer\n";
$g = $r->newLazyGhost(function (D $o) { echo "never\n"; });
$r->markLazyObjectAsInitialized($g);
unset($g);
echo "-- uninitialized ghost at end of request\n";
$end = $r->newLazyGhost(function (D $o) { echo "init end\n"; });
$end2 = $r->newLazyProxy(function (D $o) { return new D; });
echo "end\n";
--EXPECT--
-- ghost, never initialized
-- ghost, initialized
init ghost
ghost
destruct ghost
-- proxy, never initialized
-- proxy, initialized
init proxy
construct
proxied
destruct proxied
-- proxy initialized, real instance kept elsewhere
construct
kept
after unset
destruct kept
-- ghost marked initialized without initializer
destruct real
-- uninitialized ghost at end of request
end
