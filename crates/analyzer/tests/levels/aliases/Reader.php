<?php

namespace AliasFixture\Reader;

use AliasFixture\Packets as Definitions;

/** @psalm-import-type Packet from Definitions as Input */
final class Reader
{
    /** @param Input $packet */
    public function number(array $packet): string
    {
        return $packet['record']['number'];
    }

    /** @param Input $packet */
    public function label(array $packet): string
    {
        return $packet['record']['label'];
    }
}
