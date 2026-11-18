--TEST--
include converts an object path once
--FILE--
<?php
if (isset($included)) { return 42; }
$included = true;
if ($included) {
    class IncludePath {
        function __toString() { echo 'path '; return __FILE__; }
    }
}
var_dump(include new IncludePath);
--EXPECT--
path int(42)
