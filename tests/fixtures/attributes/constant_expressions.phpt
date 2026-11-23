--TEST--
Attribute expressions can cast values and compute class constant names
--FILE--
<?php
#[Attribute]
class Initializers {
    public array $values;
    public function __construct(mixed ...$values) { $this->values = $values; }
}

const VALUE = '24';
const MEMBER = 'RESULT';
class Configuration {
    private const RESULT = 17;

    #[Initializers((int) VALUE, self::{MEMBER}, (object) ['parsed' => VALUE], [...[1, 2], 3])]
    public function run($fallback = (int) VALUE) {}
}

$method = new ReflectionMethod(Configuration::class, 'run');
var_dump($method->getAttributes()[0]->newInstance()->values);
var_dump($method->getParameters()[0]->getDefaultValue());
--EXPECT--
array(4) {
  [0]=>
  int(24)
  [1]=>
  int(17)
  [2]=>
  object(stdClass)#3 (1) {
    ["parsed"]=>
    string(2) "24"
  }
  [3]=>
  array(3) {
    [0]=>
    int(1)
    [1]=>
    int(2)
    [2]=>
    int(3)
  }
}
int(24)
