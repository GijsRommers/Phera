--TEST--
Generator is a final Iterator and reports its originating function
--FILE--
<?php
class Source {
    public function values() { yield 1; }
}
$generator = (new Source)->values();
var_dump(get_class($generator), is_iterable($generator),
    $generator instanceof Traversable, $generator instanceof Iterator,
    $generator instanceof Generator);
var_dump($generator);
print_r($generator);
try { new Generator; } catch (Error $error) { echo $error->getMessage(), "\n"; }
try { clone $generator; } catch (Error $error) { echo $error->getMessage(), "\n"; }
try { array_map(fn($x) => $x, $generator); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
--EXPECTF--
string(9) "Generator"
bool(true)
bool(true)
bool(true)
bool(true)
object(Generator)#%d (1) {
  ["function"]=>
  string(14) "Source::values"
}
Generator Object
(
    [function] => Source::values
)
The "Generator" class is reserved for internal use and cannot be manually instantiated
Trying to clone an uncloneable object of class Generator
array_map(): Argument #2 ($array) must be of type array, Generator given
