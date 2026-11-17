<?php
class Item { public int $key = 0; public int $amount = 1; }
$items = [];
for ($i = 0; $i < 200000; $i++) {
    $item = new Item;
    $item->key = (200000 - $i) % 997;
    $items[] = $item;
}
// Bottom-up merge sort uses only plain PHP loops and property comparisons.
$n = count($items);
for ($width = 1; $width < $n; $width *= 2) {
    $sorted = [];
    for ($start = 0; $start < $n; $start += 2 * $width) {
        $left = $start; $middle = $start + $width;
        if ($middle > $n) { $middle = $n; }
        $right = $middle; $end = $start + 2 * $width;
        if ($end > $n) { $end = $n; }
        while ($left < $middle || $right < $end) {
            if ($right >= $end || ($left < $middle && $items[$left]->key <= $items[$right]->key)) {
                $sorted[] = $items[$left++];
            } else { $sorted[] = $items[$right++]; }
        }
    }
    $items = $sorted;
}
$sum = 0;
foreach ($items as $item) { $sum += $item->amount + $item->key; }
echo $sum, ':', $items[0]->key, ':', $items[$n-1]->key, "\n";
