--TEST--
Lazy reflection expressions retain their source lines and lexical scope
--FILE--
<?php
namespace Example;
use stdClass as Imported;

#[\Attribute(\Attribute::TARGET_METHOD)]
class Data {
    public function __construct(public array $values) {}
}

class Holder {
    private const BASE = 3;

    #[Data([
        'class' => Imported::class,
        'line' => __LINE__,
        'value' => self::BASE + 2,
    ])]
    public function sample($value = [self::BASE, Imported::class]) {}
}

$method = new \ReflectionMethod(Holder::class, 'sample');
$attribute = $method->getAttributes()[0];
foreach ($attribute->getArguments()[0] as $key => $value) {
    echo "$key=$value\n";
}
echo $attribute->newInstance()->values['value'], "\n";
echo implode(',', $method->getParameters()[0]->getDefaultValue()), "\n";
--EXPECT--
class=stdClass
line=15
value=5
5
3,stdClass
