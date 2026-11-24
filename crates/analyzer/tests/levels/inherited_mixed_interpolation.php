<?php

interface ValueSource
{
    /** @return mixed */
    public function value();
}

final class FixedValue implements ValueSource
{
    public function value()
    {
        return 1;
    }
}

function interpolate(FixedValue $source): string
{
    $value = $source->value();

    return "Value: $value";
}
