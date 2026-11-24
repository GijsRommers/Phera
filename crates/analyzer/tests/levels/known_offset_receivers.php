<?php

/** @param list<string>|false $matches */
function printMatch(array|false $matches): void
{
    echo $matches[0];
    echo $matches[0] ?? '';
}
