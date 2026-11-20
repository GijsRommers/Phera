--TEST--
Closure binding autoloads the named class scope
--FILE--
<?php
spl_autoload_register(function ($class) {
    echo "load:$class\n";
    if ($class === "BoundScope") eval("class BoundScope {}");
});
$f = (function () { return self::class; })->bindTo(null, "BoundScope");
echo $f(), "\n";
?>
--EXPECT--
load:BoundScope
BoundScope
