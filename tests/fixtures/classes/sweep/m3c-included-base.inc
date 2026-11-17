<?php
class IncludedBase
{
    public function __construct(public int $value)
    {
    }

    public function label(): string
    {
        return 'base:'.$this->value;
    }
}
