<?php
class AddTail extends php_user_filter
{
    public function filter($in, $out, &$consumed, bool $closing): int {
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            $bucket->data .= 'xy';
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
}
class WrapperBacking
{
    public $context;
    public static string $bytes = 'aBc';
    private int $offset = 0;
    public function stream_open($path, $mode, $flags, &$opened) { return true; }
    public function stream_read($n) {
        $bytes = substr(self::$bytes, $this->offset, $n);
        $this->offset += strlen($bytes);
        return $bytes;
    }
    public function stream_write($bytes) { self::$bytes .= $bytes; return strlen($bytes); }
    public function stream_eof() { return $this->offset >= strlen(self::$bytes); }
    public function stream_flush() { return true; }
    public function stream_stat() { return ['size' => strlen(self::$bytes)]; }
}
stream_filter_register('add-tail', AddTail::class);
stream_wrapper_register('filter-backing', WrapperBacking::class);

$stream = fopen('php://memory', 'w+');
$upper = stream_filter_append($stream, 'string.toupper', STREAM_FILTER_WRITE);
stream_filter_append($stream, 'add-tail', STREAM_FILTER_WRITE);
stream_filter_prepend($stream, 'string.rot13', STREAM_FILTER_WRITE);
fwrite($stream, 'aBc');
stream_filter_remove($upper);
fwrite($stream, 'aBc');
rewind($stream);
var_dump(stream_get_contents($stream));
fclose($stream);

$uri = 'php://filter/read=string.toupper|add-tail|convert.base64-encode/resource=';
var_dump(file_get_contents($uri . 'data:text/plain,aBc'));
var_dump(file_get_contents($uri . 'filter-backing://bytes'));
var_dump(file_put_contents('php://filter/write=add-tail|string.toupper/resource=filter-backing://bytes', 'xy'));
var_dump(WrapperBacking::$bytes);
