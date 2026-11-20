<?php
set_error_handler(function ($level, $message) { echo $level,':',$message,'|'; return true; });
$image = imagecreatetruecolor(5, 3);
foreach ([
'dim-x' => fn() => imagecreatetruecolor(0, 2),
'dim-y' => fn() => imagecreate(2, -1),
'max-x' => fn() => imagecreate(PHP_INT_MAX, 2),
'object' => fn() => new GdImage(),
'constructor-reflection' => fn() => (new ReflectionClass(GdImage::class))->getConstructor(),
'construction-reflection' => fn() => (new ReflectionClass(GdImage::class))
    ->newInstanceWithoutConstructor(),
'dynamic-property' => fn() => $image->custom = true,
'same-image' => fn() => $image == $image,
'different-image' => fn() => $image == imagecreatetruecolor(5, 3),
'clone' => fn() => clone $image,
'serialize' => fn() => serialize($image),
'type' => fn() => imagecolorsforindex(33, 0),
'index' => fn() => imagecolorsforindex($image, -1),
'color-low' => fn() => imagecolorallocate($image, -1, 0, 0),
'alpha-high' => fn() => imagecolorallocatealpha($image, 0, 0, 0, 128),
'pixel-x' => fn() => imagecolorat($image, -1, 0),
'pixel-y' => fn() => imagecolorat($image, 4, 3),
'pixel-set' => fn() => imagesetpixel($image, -1, 0, 0),
'empty-png' => fn() => imagecreatefrompng(''),
'missing-png' => fn() => imagecreatefrompng($argv[1] . '/missing-image.png'),
'nul-png' => fn() => imagecreatefrompng("a\0b"),
'invalid-data' => fn() => imagecreatefromstring("hello"),
'truncated-png' => fn() => imagecreatefromstring("\x89PNG\r\n\x1a\n"),
'truncated-jpeg' => fn() => imagecreatefromstring("\xff\xd8\xff"),
'truncated-bmp' => fn() => imagecreatefromstring("BM"),
'jpeg-quality' => fn() => imagejpeg($image, null, 200),
'png-quality' => fn() => imagepng($image, null, -2),
'webp-quality' => fn() => imagewebp($image, null, -2),
'output-invalid' => fn() => imagepng($image, []),
'output-nul' => fn() => imagepng($image, "a\0b"),
'output-empty' => fn() => imagepng($image, ""),
'flip-mode' => fn() => imageflip($image, 4),
'scale-mode' => fn() => imagescale($image, 4, 4, -1),
'resize-negatives' => fn() => imagescale($image, -1, -1),
'crop-key' => fn() => imagecrop($image, ['x' => 0]),
'polygon-length' => fn() => imagepolygon($image, [0, 1, 2], 0),
'polygon-shape' => fn() => imagepolygon($image, [0, 1], 0),
'filter-mode' => fn() => imagefilter($image, 30),
'filter-options' => fn() => imagefilter($image, IMG_FILTER_COLORIZE, [], 0, 0),
'filter-count' => fn() => imagefilter($image, IMG_FILTER_GRAYSCALE, 0),
'gamma-input' => fn() => imagegammacorrect($image, 0, 1),
'resolution-x' => fn() => imageresolution($image, -1, 0),
'imagedestroy' => fn() => imagedestroy($image),
'imagedestroy-type' => fn() => imagedestroy(null),
'imagedestroy-missing' => fn() => imagedestroy(),
] as $name => $case) {
    echo $name,'|';
    try { $value = $case(); echo $value===false?'false':($value===true?'true':gettype($value)); }
    catch (Throwable $exception) { echo get_class($exception),':',$exception->getMessage(); }
    echo "\n";
}
