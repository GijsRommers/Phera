--TEST--
array_map callbacks on the frame stack: fibers, error handlers, exit
--FILE--
<?php
$fiber = new Fiber(function () {
    return array_map(function ($v) { $got = Fiber::suspend($v); return "$v:$got"; }, [1, 2]);
});
$value = $fiber->start();
while (!$fiber->isTerminated()) { echo "suspended $value\n"; $value = $fiber->resume("r$value"); }
echo json_encode($fiber->getReturn()), "\n";
set_error_handler(function ($no, $msg) { echo "handler: $msg\n"; return true; });
echo json_encode(array_map(fn ($v) => $v + $undefined, [1])), "\n";
restore_error_handler();
register_shutdown_function(fn () => print("shutdown\n"));
array_map(function ($v) { if ($v === 2) { exit("exit in callback\n"); } echo "v$v\n"; return $v; }, [1, 2, 3]);
--EXPECTF--
suspended 1
suspended 2
["1:r1","2:r2"]
handler: Undefined variable $undefined
[1]
v1
exit in callback
shutdown
