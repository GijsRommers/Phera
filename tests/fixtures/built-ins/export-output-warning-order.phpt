--TEST--
var_export writes after the circular reference warning handler
--FILE--
<?php
foreach ([false, true] as $throws) {
    $arr = [];
    $arr['self'] = &$arr;
    ob_start();
    set_error_handler(function ($level, $message) use ($throws) {
        echo "$message\n";
        if ($throws) { throw new RuntimeException('stopped'); }
        return true;
    });
    try {
        var_export($arr);
    } catch (RuntimeException $error) {
        echo $error->getMessage();
    }
    restore_error_handler();
    var_dump(ob_get_clean());
    unset($arr);
}
--EXPECT--
string(74) "var_export does not handle circular references
array (
  'self' => NULL,
)"
string(81) "var_export does not handle circular references
array (
  'self' => NULL,
)stopped"
