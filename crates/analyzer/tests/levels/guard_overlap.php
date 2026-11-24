<?php
declare(strict_types=1);

function acceptText(string $text): string { return $text; }

function numericText(string $text): string
{
    return is_numeric($text) ? acceptText($text) : '';
}

class LabelView
{
    public string $label = '';
}

function sameLabel(LabelView $view, ?string $text): string
{
    if ($view->label !== $text || $text === '') {
        return '';
    }

    return acceptText($text);
}
