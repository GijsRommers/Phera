<?php
class IncludedChild extends IncludedBase
{
    public static function make(int $value): static
    {
        return new static($value);
    }

    public function label(): string
    {
        return parent::label().'/child';
    }
}
