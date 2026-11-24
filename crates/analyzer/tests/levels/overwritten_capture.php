<?php

function createCallback(string $label): Closure
{
    return static function (bool $replace) use ($label): string {
        echo $label;
        if ($replace) {
            $label = 'discarded';
        }

        $label = 'kept';

        return $label;
    };
}
