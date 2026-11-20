<?php
$root = $argv[1];
foreach (array_slice($argv, 2) as $name) {
    $path = $root . '/' . $name;
    $bytes = file_get_contents($path);
    $image = imagecreatefromstring($bytes);
    $dimensions = [imagesx($image), imagesy($image), imageistruecolor($image)];
    $pixels = '';
    for ($y = 0; $y < $dimensions[1]; $y++) {
        for ($x = 0; $x < $dimensions[0]; $x++) {
            $pixel = imagecolorat($image, $x, $y);
            $rgba = imagecolorsforindex($image, $pixel);
            $pixels .= pack('C4', ...array_values($rgba));
        }
    }
    echo $name, "\t", implode(',', $dimensions), "\t", bin2hex($pixels), "\n";
}
