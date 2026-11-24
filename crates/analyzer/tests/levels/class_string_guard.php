<?php

interface Row {}

/** @return class-string<Row> */
function rowClass(string $name): string
{
    if (class_exists($name)) {
        return $name;
    }

    throw new InvalidArgumentException();
}
