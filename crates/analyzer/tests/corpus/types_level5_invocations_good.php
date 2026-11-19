<?php
// Rule: Argument contracts follow the resolved instance, static and constructor signature.
final class CheckedCall {
    public function __construct(public int $value) {}
    public function method(string $name): string { return $name; }
    public static function staticMethod(int $count): int { return $count; }
}
function checkInvocations(CheckedCall $receiver): void {
    echo $receiver->method('x');
    echo CheckedCall::staticMethod(1);
    $instance = new CheckedCall(1);
    echo $instance->value;
}
