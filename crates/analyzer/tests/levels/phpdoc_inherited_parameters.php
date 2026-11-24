<?php

interface ReaderContract
{
    /** @param string|string[] $input */
    public function read($input): string;
}

final class DocumentedReader implements ReaderContract
{
    public function read($values): string
    {
        if ($values instanceof Traversable) {
            return 'traversable';
        }

        return 'array or string';
    }
}
