<?php
// Rule: PHPDoc syntax and compatible refinements of a native type.
/**
 * @template T of object
 * @method void log(string $message = 'a', array $options = [1, 2])
 * @property T $item
 */
class AnnotatedBox {
    public function __call(string $name, array $args): void {}
}
/**
 * @param callable(mixed...): int $callback
 * @param array{item: AnnotatedBox<stdClass>, optional?: string} $items
 * @return non-empty-list<int>
 */
function validDocs(callable $callback, array $items): array {
    $items['item']->log();
    return [$callback()];
}
