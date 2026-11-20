--TEST--
Native named defaults: reflection defaults
--FILE--
<?php
echo new ReflectionFunction('json_decode');
foreach ([
    ['json_decode', 2],
    ['array_keys', 1],
    ['str_pad', 3],
    ['htmlspecialchars', 1],
    ['round', 2],
] as [$name, $index]) {
    $parameter = (new ReflectionFunction($name))->getParameters()[$index];
    var_dump($parameter->isOptional(), $parameter->isDefaultValueAvailable());
    try {
        var_dump($parameter->getDefaultValue(), $parameter->getDefaultValueConstantName());
    } catch (ReflectionException $e) {
        echo $e->getMessage(), "\n";
    }
}
?>
--EXPECT--
Function [ <internal:json> function json_decode ] {

  - Parameters [4] {
    Parameter #0 [ <required> string $json ]
    Parameter #1 [ <optional> ?bool $associative = null ]
    Parameter #2 [ <optional> int $depth = 512 ]
    Parameter #3 [ <optional> int $flags = 0 ]
  }
  - Return [ mixed ]
}
bool(true)
bool(true)
int(512)
NULL
bool(true)
bool(false)
Internal error: Failed to retrieve the default value
bool(true)
bool(true)
int(1)
string(13) "STR_PAD_RIGHT"
bool(true)
bool(true)
int(11)
NULL
bool(true)
bool(true)
enum(RoundingMode::HalfAwayFromZero)
string(30) "RoundingMode::HalfAwayFromZero"
