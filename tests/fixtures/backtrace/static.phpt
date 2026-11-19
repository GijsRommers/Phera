--TEST--
A static call has a scope and no object
--FILE--
<?php
class Example {
    static function inspect() { echo json_encode(debug_backtrace(), JSON_UNESCAPED_SLASHES); }
}
Example::inspect();
--EXPECTF--
[{"file":"%s/static.php","line":5,"function":"inspect","class":"Example","type":"::","args":[]}]
