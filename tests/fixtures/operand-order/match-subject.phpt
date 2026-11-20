--TEST--
Sequential match arms consume a compiled variable subject for each comparison
--FILE--
<?php
function changeSubject(&$subject) {
    $subject = 5;
    return 3;
}
$subject=2;
var_dump(match ($subject) { changeSubject($subject) => 'three', 2 => 'old', 5 => 'new' });
$subject=[2];
var_dump(match ($subject[0]) { changeSubject($subject[0]) => 'three', 2 => 'old', 5 => 'new' });
set_error_handler(static function ($level, $message) {
    echo $message, "\n";
    return true;
});
var_dump(match ($missing) { default => 'only' });
var_dump(match ($missing) { 2 => 'two', 3 => 'three', default => 'integers' });
var_dump(match ($missing) { true => 'true', null => 'null', default => 'other' });
--EXPECT--
string(3) "new"
string(3) "old"
string(4) "only"
Undefined variable $missing
string(8) "integers"
Undefined variable $missing
Undefined variable $missing
string(4) "null"
