--TEST--
Enum cases are object keys for SplObjectStorage and WeakMap
--FILE--
<?php
enum CacheKey: string {
    case Routes = 'routes';
    case Services = 'services';
}

$storage = new SplObjectStorage();
$weak = new WeakMap();
foreach (CacheKey::cases() as $case) {
    $storage[$case] = strtoupper($case->value);
    $weak[$case] = strlen($case->value);
}
foreach ($storage as $case) {
    echo $case->name, ' ', $storage[$case], ' ', $weak[$case], "\n";
}
unset($weak[CacheKey::Routes]);
echo count($storage), ' ', count($weak), "\n";
--EXPECT--
Routes ROUTES 6
Services SERVICES 8
2 1
