<?php
$root = $argv[1];
$packed = ($argv[2] ?? null) === '--packed';
foreach (array_slice($argv, $packed ? 3 : 2) as $name) {
    $path = $root . '/' . $name;
    $bytes = file_get_contents($path);
    $image = imagecreatefromstring($bytes);
    $dimensions = [imagesx($image), imagesy($image), imageistruecolor($image)];
    $pixels = '';
    for ($y = 0; $y < $dimensions[1]; $y++) {
        for ($x = 0; $x < $dimensions[0]; $x++) {
            $pixel = imagecolorat($image, $x, $y);
            $pixels .= $packed
                ? pack('N', $pixel)
                : pack('C4', ...array_values(imagecolorsforindex($image, $pixel)));
        }
    }
    echo $name, "\t", implode(',', $dimensions), "\t", bin2hex($pixels), "\n";
}
