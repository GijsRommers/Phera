--TEST--
Variance diagnostics resolve relative declarations and expand iterable
--FILE--
<?php
class ParentFactory {
    function create(iterable $values): static {
        return new static;
    }
}
class ChildFactory extends ParentFactory {
    function create(iterable $values): self {
        return new self;
    }
}
--EXPECTF--
Fatal error: Declaration of ChildFactory::create(Traversable|array $values): ChildFactory must be compatible with ParentFactory::create(Traversable|array $values): static in %s on line 8
