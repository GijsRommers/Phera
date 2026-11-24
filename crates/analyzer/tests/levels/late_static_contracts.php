<?php

abstract class ExtensibleFactory
{
    public static function make(): self
    {
        return new static();
    }

    public static function limit(): int
    {
        return static::LIMIT;
    }
}

/** @phpstan-consistent-constructor */
class DocumentedFactory
{
    public static function make(): self
    {
        return new static();
    }
}

class FixedConstructor
{
    final public function __construct() {}

    public static function make(): self
    {
        return new static();
    }
}
