--TEST--
Reflected hook access and invocation suspend on PHP frames
--FILE--
<?php
class StackLink {
    public static ReflectionProperty $getter;
    public static ReflectionProperty $setter;
    public static ReflectionMethod $method;
    public ?self $next = null;
    public int $read {
        get => $this->next ? self::$getter->getValue($this->next) + 1 : 1;
    }
    public int $walk {
        get => $this->next ? self::$method->invoke($this->next) + 1 : 1;
    }
    public int $seen = 0;
    public int $write {
        set(#[\SensitiveParameter] int $value) {
            if ($this->next) {
                self::$setter->setValue($this->next, $value + 1);
            } else {
                $this->seen = $value;
            }
        }
    }
}
StackLink::$getter = new ReflectionProperty(StackLink::class, 'read');
StackLink::$setter = new ReflectionProperty(StackLink::class, 'write');
StackLink::$method = (new ReflectionProperty(StackLink::class, 'walk'))->getHook(PropertyHookType::Get);
$leaf = new StackLink();
$head = $leaf;
for ($i = 0; $i < 200; ++$i) {
    $node = new StackLink();
    $node->next = $head;
    $head = $node;
}
StackLink::$setter->setValue($head, 1);
var_dump(StackLink::$getter->getValue($head), StackLink::$method->invoke($head), $leaf->seen);
--EXPECT--
int(201)
int(201)
int(201)
