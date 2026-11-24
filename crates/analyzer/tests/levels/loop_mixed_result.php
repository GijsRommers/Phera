<?php

function emitText(string $text, Closure $replace, bool $again): void
{
    while ($again) {
        echo strlen($text);
        $text = $replace($text);
    }
}
