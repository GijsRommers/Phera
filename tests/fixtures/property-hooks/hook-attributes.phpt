--TEST--
Hooked properties and hook methods preserve attributes and redact sensitive setter parameters
--FILE--
<?php
class ParentHook {
    public int $value {
        get => 1;
        set => $value;
    }
}
class ChildHook extends ParentHook {
    #[\Override]
    public int $value {
        #[\Override]
        #[\Deprecated('get is old', '8.5')]
        get => parent::$value::get() + 1;
        #[\Override]
        #[\Deprecated('set is old')]
        set(#[\SensitiveParameter] int $value) {
            if ($value < 0) {
                throw new RuntimeException('rejected');
            }
            parent::$value::set($value);
        }
    }
}
set_error_handler(function ($level, $message) {
    echo $level, ': ', $message, "\n";
    return true;
});
$object = new ChildHook();
echo $object->value, "\n";
$object->value = 3;
try {
    $object->value = -42;
} catch (RuntimeException $e) {
    $frame = $e->getTrace()[0];
    echo $frame['class'], $frame['type'], $frame['function'], "\n";
    echo get_class($frame['args'][0]), "\n";
}
$rp = new ReflectionProperty(ChildHook::class, 'value');
echo $rp->getAttributes()[0]->getName(), "\n";
foreach ($rp->getHooks() as $hook) {
    echo $hook->getName(), ': ', implode(', ', array_map(fn($attribute) => $attribute->getName(), $hook->getAttributes())), "\n";
}
echo $rp->getHook(PropertyHookType::Set)->getParameters()[0]->getAttributes()[0]->getName(), "\n";
--EXPECT--
16384: Method ChildHook::$value::get() is deprecated since 8.5, get is old
2
16384: Method ChildHook::$value::set() is deprecated, set is old
16384: Method ChildHook::$value::set() is deprecated, set is old
ChildHook->$value::set
SensitiveParameterValue
Override
$value::get: Override, Deprecated
$value::set: Override, Deprecated
SensitiveParameter
