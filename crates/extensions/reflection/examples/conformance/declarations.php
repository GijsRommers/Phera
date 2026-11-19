<?php
interface InterfaceA { public function required(int $number): int; }
interface InterfaceB extends InterfaceA {}
abstract class BaseClass implements InterfaceB {
    private int $hidden = 12;
    protected static ?int $counter = null;
    protected const TEXT = 'text';
    public function required(int $number): int { return $number * 2; }
    protected function optional(string $word = PHP_VERSION): string { return $word; }
}
class MiddleClass extends BaseClass {}
#[AuditMarker(3)]
class Subject extends MiddleClass {
    public const int CODE = 9;
    private static string $value = 'x';
    public function __construct(public int $id = 2) {}
    #[AuditMarker(4)]
    public function method(#[AuditMarker(5)] int|string $value = self::CODE): int|string {
        return $value;
    }
    public function &reference(&$value) { return $value; }
    public static function rest(int ...$values): array { return $values; }
}
#[Attribute(Attribute::TARGET_ALL | Attribute::IS_REPEATABLE)]
class AuditMarker { public function __construct(public int $number = 3) {} }
interface HookedContract { public int $hooked { get; } }
class HookedSubject extends Subject implements HookedContract {
    #[AuditMarker(6)]
    final public int $hooked = 4 {
        #[AuditMarker(7)]
        get => $this->hooked + 1;
        #[AuditMarker(8)]
        final set(#[SensitiveParameter] int|string $value) => (int) $value * 2;
    }
    public int $virtual { get => 5; }
}
class Inaccessible { private function __construct() {} }
readonly class Immutable { public function __construct(public int $id = 7) {} }
final class FinalClass { public static $value = 2; }
class Typed { public int $integer; protected ?string $text = null; }
class Asymmetric { public private(set) int $id = 3; }
class Untyped { public $value; }
class PrivateProperty { private $value; }
class ProtectedProperty { protected $value; }
class Uninitialized { public string $value; }
class PublicStatic { public static $value = 3; }
class ProtectedStatic { protected static $value = 3; }
class PrivateStatic { private static $value = 3; }
class InheritedPrivate extends PrivateProperty {}
class InheritedProtected extends ProtectedProperty {}
class InheritedPublic extends Untyped {}
class FinalProperty { final public int $value = 3; }
class ReadOnlyProperty { public readonly int $value; }
class ProtectedSetProperty { public protected(set) int $value = 3; }
class PrivateSetProperty { public private(set) int $value = 3; }
class OptionalObject { public function __construct(public ?Subject $subject = null) {} }
class EmptyContract implements Stringable { public function __toString(): string { return 's'; } }
class CountableObject implements Countable { public function count(): int { return 1; } }
class UnionProperty { public int|string $value = 5; }
class MixedProperty { public mixed $value = null; }
class NullableProperty { public ?string $value = null; }
class EmptyClass {}
class PrivateMethod { private function hidden() {} }
function testFunction(?int $value = 1, int|string ...$more): ?int { return $value; }
function testIntersection(Countable&Stringable $value): (Countable&Stringable)|null { return $value; }
function testStatic(): int { static $value = 3; return $value++; }
function testClosure(): Closure { $value = 4; return static fn (int $x): int => $x + $value; }
#[AuditMarker]
const AUDIT_CONSTANT = 7;
$subject = new Subject();
$ref = 11;
$refs = [&$ref, &$ref, 'not a reference'];
