<?php
$root = $argv[1];
$destination = $argv[2];
$rgba = imagecreatefrompng($root . '/rgba.png');
$indexed = imagecreatefrompng($root . '/indexed.png');
imagesavealpha($rgba, true);
imagepng($rgba, $destination . '/rgba.png', 6, PNG_FILTER_PAETH);
imagegif($indexed, $destination . '/indexed.gif');
imagebmp($indexed, $destination . '/indexed.bmp', true);
imagebmp($rgba, $destination . '/rgb.bmp', false);
imagewebp($rgba, $destination . '/rgba-lossless.webp', IMG_WEBP_LOSSLESS);
imagejpeg($rgba, $destination . '/gradient.jpeg', 87);
ob_start();
imagepng($rgba);
file_put_contents($destination . '/stdout.png', ob_get_clean());
$handle = fopen($destination . '/stream.png', 'wb');
imagepng($rgba, $handle);
fclose($handle);
