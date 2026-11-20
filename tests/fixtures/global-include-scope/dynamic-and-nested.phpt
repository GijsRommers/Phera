--TEST--
Dynamic globals without entry registers and nested includes that throw
--FILE--
<?php
function declareGlobals() {
    global $first_from_function;
    $first_from_function = 'function';
    $GLOBALS['first_from_table'] = 'table';
}
declareGlobals();
$name = 'first_' . 'from_function';
var_dump($$name, compact('first_from_table')['first_from_table']);
var_dump(get_defined_vars()[$name]);
$outer = tempnam(sys_get_temp_dir(), 'scope');
$inner = tempnam(sys_get_temp_dir(), 'scope');
file_put_contents($outer, '<?php $from_outer = "outside"; require $inner;');
file_put_contents($inner, '<?php global $from_outer; echo $from_outer, "\n"; $GLOBALS["from_inner"] = "inside"; throw new Exception();');
try {
    require $outer;
} catch (Exception $error) {
    echo $from_inner, "\n";
}
var_dump(compact('from_outer')['from_outer']);
unlink($outer);
unlink($inner);
--EXPECT--
string(8) "function"
string(5) "table"
string(8) "function"
outside
inside
string(7) "outside"
