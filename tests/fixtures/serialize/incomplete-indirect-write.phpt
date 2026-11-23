--TEST--
Nested and compound writes on incomplete objects fail before reading properties
--FILE--
<?php
function probe(Closure $operation) {
    try { $operation(); } catch (Throwable $error) { echo $error->getMessage(), "\n"; }
}
$value = unserialize('O:7:"Missing":1:{s:4:"list";a:0:{}}');
probe(function () use ($value) { $value->list['k'][] = 1; });
probe(function () use ($value) { $value->list['k'] += 1; });
probe(function () use ($value) { ++$value->list['k']; });
probe(function () use ($value) { unset($value->list['k']); });
probe(function () use ($value) { $value->list .= 'x'; });
echo serialize($value), "\n";
--EXPECT--
The script tried to modify a property on an incomplete object. Please ensure that the class definition "Missing" of the object you are trying to operate on was loaded _before_ unserialize() gets called or provide an autoloader to load the class definition
The script tried to modify a property on an incomplete object. Please ensure that the class definition "Missing" of the object you are trying to operate on was loaded _before_ unserialize() gets called or provide an autoloader to load the class definition
The script tried to modify a property on an incomplete object. Please ensure that the class definition "Missing" of the object you are trying to operate on was loaded _before_ unserialize() gets called or provide an autoloader to load the class definition
The script tried to modify a property on an incomplete object. Please ensure that the class definition "Missing" of the object you are trying to operate on was loaded _before_ unserialize() gets called or provide an autoloader to load the class definition
The script tried to modify a property on an incomplete object. Please ensure that the class definition "Missing" of the object you are trying to operate on was loaded _before_ unserialize() gets called or provide an autoloader to load the class definition
O:7:"Missing":1:{s:4:"list";a:0:{}}
