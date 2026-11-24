<?php

final readonly class ReadonlyTitle
{
    public function __construct(public ?string $text = null) {}
}

final class MutableTitle
{
    public ?string $text;
}

function readonlyText(ReadonlyTitle $title): ?string
{
    return $title->text ?? null;
}

function mutableText(MutableTitle $title): ?string
{
    return $title->text ?? null;
}
