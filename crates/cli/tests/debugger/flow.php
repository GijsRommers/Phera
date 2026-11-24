<?php
enum Mode: string { case Ready = 'ready'; }
class DebugFailure extends RuntimeException {}
class Pending { public string $label; }
require __DIR__ . '/flow-include.php';
$mode = Mode::Ready;
$now = new DateTimeImmutable('2026-11-20T08:00:00+00:00');
$lazy = (new ReflectionClass(Pending::class))->newLazyGhost(static function (Pending $pending) {
    $pending->label = 'initialized';
});
$recursive = [];
$recursive['self'] =& $recursive;
$total = 0;
foreach (marks() as $mark) {
    $total += $mark;
}
try {
    throw new DebugFailure('caught at the debugger', 17);
} catch (DebugFailure $failure) {
    $total += $failure->getCode();
}
echo $total, "\n";
