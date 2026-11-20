<?php
$truecolor = imagecreatetruecolor(1, 1);
imagealphablending($truecolor, false);
imagesetpixel($truecolor, 0, 0, 0x40203040);

$indexed = imagecreate(1, 1);
imagecolorallocate($indexed, 1, 2, 3);
$finfo = new finfo(FILEINFO_MIME_TYPE);

Phera\snapshot();

if (isset($_GET['mutate'])) {
    imagesetpixel($truecolor, 0, 0, 0x20224466);
    $new = imagecolorallocate($indexed, 4, 5, 6);
    imagesetpixel($indexed, 0, 0, $new);
    $finfo->set_flags(FILEINFO_MIME_ENCODING);
}

printf("%08x %d %s\n", imagecolorat($truecolor, 0, 0), imagecolorat($indexed, 0, 0),
    $finfo->buffer("some text\n"));
