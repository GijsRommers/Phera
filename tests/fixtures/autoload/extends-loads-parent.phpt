--TEST--
Extends loads parent
--FILE--
<?php
spl_autoload_register(function ($name) {
    echo "load ", $name, "\n";
    require __DIR__ . '/autoload-example.inc';
});
class Child extends AutoloadExample {} var_dump(get_parent_class("Child"));
--EXPECT--
load AutoloadExample
string(15) "AutoloadExample"
