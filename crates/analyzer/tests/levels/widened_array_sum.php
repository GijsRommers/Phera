<?php
declare(strict_types=1);

/** @param list<int> $scores */
function totalScores(array $scores): int
{
    $rows = [];
    foreach ($scores as $score) {
        $rows[] = ['score' => $score];
    }

    return array_sum(array_column($rows, 'score'));
}
