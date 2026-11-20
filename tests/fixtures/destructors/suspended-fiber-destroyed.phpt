--TEST--
suspended-fiber-destroyed
--FILE--
<?php
// Objects that only a suspended fiber's stack holds are destructed when the
// fiber is destroyed: its finally blocks run first, then its variables go.
class T { function __construct(public $n) {} function __destruct() { echo "destruct {$this->n}\n"; } }
function inner() {
    $deep = new T('inner local');
    try { Fiber::suspend(1); } finally { echo "inner finally\n"; }
}
$fiber = new Fiber(function (T $argument) {
    $local = new T('fiber local');
    try { inner(); } finally { echo "outer finally\n"; }
});
$fiber->start(new T('argument'));
echo "suspended\n";
unset($fiber);
echo "after unset\n";

$kept = new Fiber(function () {
    $local = new T('kept to the end');
    Fiber::suspend();
    echo "never resumed\n";
});
$kept->start();
$done = new Fiber(function () { $x = new T('finished fiber'); });
$done->start();
echo "end of script\n";
--EXPECT--
suspended
inner finally
destruct inner local
outer finally
destruct argument
destruct fiber local
after unset
destruct finished fiber
end of script
destruct kept to the end
