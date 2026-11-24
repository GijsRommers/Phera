<?php declare(strict_types=1);
final class TextHolder
{
    public string $text;
}
function takeAssignedText(string $text): void {}
function readAfterWrite(TextHolder $holder, ?string $text): void {
    $holder->text = $text;
    takeAssignedText($holder->text);
}
function readAfterRegex(TextHolder $holder, string $text): void {
    $holder->text = preg_replace('~\s~', ' ', $text);
    takeAssignedText($holder->text);
}

final class OtherNode {}

/** @param list<OtherNode|TextHolder> $nodes */
function readAfterInstanceof(array $nodes, string $text): void
{
    foreach ($nodes as $node) {
        if ($node instanceof TextHolder) {
            $node->text = $text;
            if ($text !== '') {
                $node->text = preg_replace('~\s~', ' ', $text);
            }
            takeAssignedText($node->text);
        }
    }
}
