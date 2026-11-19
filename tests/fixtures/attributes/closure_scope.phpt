--TEST--
Closure and parameter attribute expressions use the current bound class
--FILE--
<?php
#[Attribute(Attribute::TARGET_FUNCTION | Attribute::TARGET_PARAMETER)]
class ScopeMark {
    public function __construct(public string $owner, public string $secret) {}
}
class Definition {
    private const SECRET = 'defined';
    public static function closure(): Closure {
        return #[ScopeMark(self::class, self::SECRET)]
            function (#[ScopeMark(self::class, self::SECRET)] $argument) {};
    }
}
class Rebound {
    private const SECRET = 'rebound';
}
$closure = Definition::closure();
foreach ([$closure, $closure->bindTo(null, Rebound::class)] as $bound) {
    $function = new ReflectionFunction($bound);
    foreach ([$function, $function->getParameters()[0]] as $declaration) {
        $attribute = $declaration->getAttributes()[0];
        $mark = $attribute->newInstance();
        echo $attribute->getTarget(), ' ', $mark->owner, ' ', $mark->secret, "\n";
    }
}
--EXPECT--
2 Definition defined
32 Definition defined
2 Rebound rebound
32 Rebound rebound
