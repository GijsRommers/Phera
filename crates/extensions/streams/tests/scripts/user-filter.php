<?php
class SampleFilter extends php_user_filter
{
    public function onCreate(): bool
    {
        echo 'create ', $this->filtername, ' ', $this->params, "\n";
        return true;
    }
    public function filter($in, $out, &$consumed, bool $closing): int
    {
        echo 'filter ', (int) $closing, "\n";
        while ($bucket = stream_bucket_make_writeable($in)) {
            $consumed += $bucket->datalen;
            $bucket->data = strtoupper($bucket->data);
            stream_bucket_append($out, $bucket);
        }
        return PSFS_PASS_ON;
    }
    public function onClose(): void { echo "closed\n"; }
}

var_dump(stream_filter_register('example.*', SampleFilter::class));
var_dump(in_array('example.*', stream_get_filters(), true));
$stream = fopen('php://memory', 'w+');
$filter = stream_filter_prepend($stream, 'example.upper', STREAM_FILTER_WRITE, 'argument');
var_dump(fwrite($stream, 'abc'), fwrite($stream, '!'));
var_dump(stream_filter_remove($filter));
rewind($stream);
var_dump(stream_get_contents($stream));
fclose($stream);
