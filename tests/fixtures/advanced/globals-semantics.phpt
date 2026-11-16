--TEST--
Global symbols, local isolation, copies and unset
--FILE--
<?php
$x = 3;
function mutate() {
    $x = 99;
    echo $GLOBALS['x'], ":", $x, "\n";
    $GLOBALS['x'] = 4;
    $GLOBALS['created'] = ['k' => 8];
    $GLOBALS['created']['k']++;
}
mutate();
echo $x, ":", $created['k'], "\n";
$copy = $GLOBALS;
$copy['x'] = 22;
echo $x, ":", $copy['x'], "\n";
$alias =& $x;
unset($GLOBALS['x']);
var_dump(isset($x), isset($GLOBALS['x']));
$GLOBALS['x'] = 6;
echo $x, ":", $alias, "\n";
--EXPECT--
3:99
4:9
4:22
bool(false)
bool(false)
6:4
