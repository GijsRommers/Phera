<?php
function pixels(GdImage $image): string {
    $bytes = '';
    for ($y = 0; $y < imagesy($image); $y++) {
        for ($x = 0; $x < imagesx($image); $x++) {
            $bytes .= pack('N', imagecolorat($image, $x, $y));
        }
    }
    return imagesx($image) . 'x' . imagesy($image) . ':' . hash('sha256', $bytes);
}

function canvas(): GdImage {
    $image = imagecreatetruecolor(13, 11);
    imagealphablending($image, false);
    for ($y = 0; $y < 11; $y++) {
        for ($x = 0; $x < 13; $x++) {
            $color = imagecolorallocatealpha($image, $x * 17, $y * 19, $x * 4 + $y * 4, 0);
            imagesetpixel($image, $x, $y, $color);
        }
    }
    return $image;
}

$red = 0xdc1421;
$blue = 0x1326d7;
$cases = [
    'pixel' => fn ($i) => imagesetpixel($i, 2, 4, $red),
    'fill' => fn ($i) => imagefill($i, 0, 0, $red),
    'filled-rect' => fn ($i) => imagefilledrectangle($i, 7, 8, 2, 3, $red),
    'rect' => fn ($i) => imagerectangle($i, 1, 2, 9, 6, $red),
    'line-clipped' => fn ($i) => imageline($i, -4, 2, 12, 8, $blue),
    'line-steep' => fn ($i) => imageline($i, 4, 9, 7, 1, $blue),
    'arc' => fn ($i) => imagearc($i, 6, 5, 8, 5, 20, 300, $red),
    'filled-arc' => fn ($i) => imagefilledarc($i, 6, 5, 8, 6, 30, 270, $red, IMG_ARC_PIE),
    'arc-chord' => fn ($i) => imagefilledarc($i, 6, 5, 10, 6, 150, 260, $blue, IMG_ARC_CHORD),
    'ellipse' => fn ($i) => imageellipse($i, 6, 5, 10, 5, $red),
    'filled-ellipse' => fn ($i) => imagefilledellipse($i, 5, 6, 7, 8, $blue),
    'polygon' => fn ($i) => imagepolygon($i, [1, 1, 9, 4, 7, 8, 3, 9], $red),
    'filled-polygon' => fn ($i) => imagefilledpolygon($i, [1, 1, 9, 4, 7, 8, 3, 9], $red),
    'copy' => fn ($i) => imagecopy($i, canvas(), 3, 2, 0, 1, 4, 4),
    'same-image-copy' => fn ($i) => imagecopy($i, $i, 3, 2, 0, 1, 6, 5),
    'merge' => fn ($i) => imagecopymerge($i, canvas(), 3, 2, 0, 0, 9, 8, 45),
    'resized' => fn ($i) => imagecopyresized($i, canvas(), 1, 1, 1, 3, 4, 7, 9, 5),
    'resampled' => fn ($i) => imagecopyresampled($i, canvas(), 1, 1, 1, 3, 4, 7, 9, 5),
    'flip-h' => fn ($i) => imageflip($i, IMG_FLIP_HORIZONTAL),
    'flip-v' => fn ($i) => imageflip($i, IMG_FLIP_VERTICAL),
    'flip-both' => fn ($i) => imageflip($i, IMG_FLIP_BOTH),
    'rotate-90' => fn ($i) => imagerotate($i, 90, $red),
    'rotate-180' => fn ($i) => imagerotate($i, 180, $red),
    'rotate-270' => fn ($i) => imagerotate($i, 270, $red),
    'crop' => fn ($i) => imagecrop($i, ['x' => 2, 'y' => 3, 'width' => 3, 'height' => 4]),
    'crop-auto' => fn ($i) => imagecropauto($i, IMG_CROP_BLACK),
    'scale-nearest' => fn ($i) => imagescale($i, 4, 9, IMG_NEAREST_NEIGHBOUR),
    'scale-bilinear' => fn ($i) => imagescale($i, 7, 2, IMG_BILINEAR_FIXED),
    'negate' => fn ($i) => imagefilter($i, IMG_FILTER_NEGATE),
    'gray' => fn ($i) => imagefilter($i, IMG_FILTER_GRAYSCALE),
    'brightness' => fn ($i) => imagefilter($i, IMG_FILTER_BRIGHTNESS, -20),
    'contrast' => fn ($i) => imagefilter($i, IMG_FILTER_CONTRAST, 30),
    'colorize' => fn ($i) => imagefilter($i, IMG_FILTER_COLORIZE, 30, -20, 0),
    'edge' => fn ($i) => imagefilter($i, IMG_FILTER_EDGEDETECT),
    'emboss' => fn ($i) => imagefilter($i, IMG_FILTER_EMBOSS),
    'gaussian' => fn ($i) => imagefilter($i, IMG_FILTER_GAUSSIAN_BLUR),
    'selective' => fn ($i) => imagefilter($i, IMG_FILTER_SELECTIVE_BLUR),
    'mean' => fn ($i) => imagefilter($i, IMG_FILTER_MEAN_REMOVAL),
    'smooth' => fn ($i) => imagefilter($i, IMG_FILTER_SMOOTH, 3.0),
    'pixelate' => fn ($i) => imagefilter($i, IMG_FILTER_PIXELATE, 3, true),
    'scatter-noop' => fn ($i) => imagefilter($i, IMG_FILTER_SCATTER, 0, 0),
    'convolution' => fn ($i) => imageconvolution($i, [[1, 1, 1], [1, 0, 1], [1, 1, 1]], 8, 0),
    'gamma' => fn ($i) => imagegammacorrect($i, 1.9, 1.0),
];

foreach ($cases as $name => $operation) {
    $image = canvas();
    $result = $operation($image);
    echo $name, '=', pixels($result instanceof GdImage ? $result : $image), "\n";
}
foreach (range(1, 5) as $font) {
    $image = imagecreatetruecolor(70, 32);
    imagestring($image, $font, 2, 1, "Hi! \xA0\xE9", $red);
    imagechar($image, $font, 15, 16, 'W', $blue);
    echo 'font-', $font, '=', imagefontwidth($font), ',', imagefontheight($font),
        ',', pixels($image), "\n";
}
