<?php

final class Labels
{
    public function render(string $label): string
    {
        return $this->normalize($label, 3);
    }

    private function normalize(string $label, int $limit): string
    {
        return trim($label);
    }
}
