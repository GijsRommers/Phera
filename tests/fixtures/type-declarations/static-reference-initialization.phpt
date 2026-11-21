--TEST--
Taking a static property reference respects typed initialization
--FILE--
<?php
class Settings {
    public static int $required;
    public static ?int $optional;
}

try {
    $required =& Settings::$required;
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
var_dump((new ReflectionProperty(Settings::class, 'required'))->isInitialized());

$optional =& Settings::$optional;
var_dump(Settings::$optional);
$optional = 7;
var_dump(Settings::$optional);
--EXPECT--
Cannot access uninitialized non-nullable property Settings::$required by reference
bool(false)
NULL
int(7)
