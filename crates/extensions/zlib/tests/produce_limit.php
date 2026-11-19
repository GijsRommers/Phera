$stream = fopen($argv[1], 'wb');
$ctx = deflate_init(ZLIB_ENCODING_GZIP, ['level' => 1]);
$chunk = str_repeat('x', 1024 * 1024);
for ($i = 0; $i < 2048; $i++) {
    fwrite($stream, deflate_add($ctx, $chunk, ZLIB_NO_FLUSH));
}
fwrite($stream, deflate_add($ctx, '', ZLIB_FINISH));
fclose($stream);
