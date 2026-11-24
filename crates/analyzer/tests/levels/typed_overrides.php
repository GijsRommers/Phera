<?php

declare(strict_types=1);

interface TextWidth
{
    /** @param string $text */
    public function width($text): int;
}

final class ByteWidth implements TextWidth
{
    public function width($value): int
    {
        return strlen($value);
    }
}

/** @param list<TextWidth> $widths
 *  @return list<int>
 */
function measure(array $widths): array
{
    return array_map(static fn ($width) => $width->width('word'), $widths);
}
