--TEST--
callback-locals-released-on-return
--FILE--
<?php
// A callback that native code calls releases its variables when it
// returns, before the native code goes on: their destructors run there.
class T { function __construct(public $n) {} function __destruct() { echo "-{$this->n}\n"; } }
function step($s) { echo "$s\n"; }

step('array_map');
array_map(function ($v) { $t = new T("map $v"); return $v; }, [1, 2]);
step('usort');
$a = [2, 1];
usort($a, function ($x, $y) { $t = new T("cmp $x $y"); return $x <=> $y; });
step('call_user_func');
call_user_func(function () { $t = new T('cuf'); });
step('output callback');
ob_start(function ($buffer) { $t = new T('ob'); return strtoupper($buffer); });
echo "buffered\n";
ob_end_flush();
step('autoloader');
spl_autoload_register(function ($class) { $t = new T("autoload $class"); eval("class $class {}"); });
new Missing();
step('after autoload');
register_shutdown_function(function () { step('shutdown 1'); $t = new T('made in shutdown'); });
register_shutdown_function(function () { step('shutdown 2'); });
set_exception_handler(function ($e) { step('handler'); $t = new T('in handler'); });
throw new Exception('uncaught');
--EXPECTF--
array_map
-map 1
-map 2
usort
-cmp 2 1
call_user_func
-cuf
output callback

Deprecated: ob_end_flush(): Producing output from user output handler %s:15} is deprecated in %s on line 17
BUFFERED
autoloader
-autoload Missing
after autoload
handler
-in handler
shutdown 1
-made in shutdown
shutdown 2
