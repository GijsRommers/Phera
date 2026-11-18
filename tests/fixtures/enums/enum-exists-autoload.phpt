--TEST--
enums: enum exists autoload
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load $name
";
    enum LoadedEnum {case A;}
});
var_dump(enum_exists("LoadedEnum", false), enum_exists("LoadedEnum"));
--EXPECT--
load LoadedEnum
bool(false)
bool(true)
