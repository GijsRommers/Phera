--TEST--
ReflectionParameter reads leading and trailing parameter doc comments
--FILE--
<?php
function documented(
    /** A leading comment */ int $a,
    string $b /** A trailing comment */,
    /** Replaced */ ?int $c = null /** A later comment */,
    $d = false,
) {}
foreach ((new ReflectionFunction('documented'))->getParameters() as $parameter) {
    echo $parameter->getName(), PHP_EOL;
    if (method_exists($parameter, 'getDocComment')) {
        var_dump($parameter->getDocComment());
    }
}
--EXPECT--
a
string(24) "/** A leading comment */"
b
string(25) "/** A trailing comment */"
c
string(22) "/** A later comment */"
d
bool(false)
