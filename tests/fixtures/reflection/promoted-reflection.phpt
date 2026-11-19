--TEST--
Reflection distinguishes promoted readonly properties from their constructor defaults
--FILE--
<?php
class Customer {
    public const DEFAULT_ID = 8;
    public function __construct(
        /** Database primary key */
        public readonly int $id = self::DEFAULT_ID,
        public readonly float $balance = 3
    ) {}
}
$r = new ReflectionClass(Customer::class);
$customer = $r->newInstanceWithoutConstructor();
$p = $r->getProperty('id');
echo $p->getDocComment(), ':', (int) $p->isPromoted(), (int) $p->isReadOnly(),
    (int) $p->hasDefaultValue(), (int) $p->isInitialized($customer), "\n";
echo (int) $p->isProtectedSet(), ':', $p->getType(), ':', $p->getDeclaringClass()->name, "\n";
$p->setValue($customer, 7);
echo $p->getValue($customer), ':', (int) $p->isInitialized($customer), "\n";
try { $p->setValue($customer, 9); } catch (Error $e) { echo $e->getMessage(), "\n"; }
foreach ($r->getConstructor()->getParameters() as $parameter) {
    echo $parameter->name, ':', (int) $parameter->isDefaultValueAvailable(), ':',
        $parameter->getDefaultValueConstantName(), ':';
    var_dump($parameter->getDefaultValue());
}
--EXPECT--
/** Database primary key */:1100
1:int:Customer
7:1
Cannot modify readonly property Customer::$id
id:1:self::DEFAULT_ID:int(8)
balance:1::float(3)
