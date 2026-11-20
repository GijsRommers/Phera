<?php
class TestWrapper
{
    public $context;
    private $position = 0;
    private $body = "first\nsecond\n";
    private $entry = 0;

    public function __construct() { echo 'construct: ', gettype($this->context), "\n"; }
    public function stream_open($url, $mode, $options, &$opened_path)
    {
        echo "open: $url $mode $options\n";
        if (str_ends_with($url, '/source')) {
            $this->body = '<?php return __FILE__;';
            $opened_path = 'resolved-source.php';
        }
        return true;
    }
    public function stream_read($count)
    {
        echo "read: $count\n";
        $data = substr($this->body, $this->position, $count);
        $this->position += strlen($data);
        return $data;
    }
    public function stream_write($data) { echo "write: $data\n"; return strlen($data); }
    public function stream_eof() { echo "eof\n"; return $this->position >= strlen($this->body); }
    public function stream_tell() { echo "tell\n"; return $this->position; }
    public function stream_seek($offset, $whence)
    {
        echo "seek: $offset $whence\n";
        $this->position = $offset;
        return true;
    }
    public function stream_flush() { echo "flush\n"; return true; }
    public function stream_close() { echo "close\n"; }
    public function stream_stat() { echo "stream-stat\n"; return ['size' => strlen($this->body)]; }
    public function url_stat($path, $flags)
    {
        echo "url-stat: $path $flags\n";
        return ['mode' => 0100666, 'size' => 13];
    }
    public function stream_set_option($option, $value, $extra)
    {
        echo "option: $option $value\n";
        return true;
    }
    public function stream_lock($operation) { echo "lock: $operation\n"; return true; }
    public function stream_truncate($size) { echo "truncate: $size\n"; return true; }
    public function dir_opendir($path, $options) { echo "opendir: $path $options\n"; return true; }
    public function dir_readdir()
    {
        echo "readdir\n";
        return ['second', 'first'][$this->entry++] ?? false;
    }
    public function dir_rewinddir() { echo "rewinddir\n"; $this->entry = 0; }
    public function dir_closedir() { echo "closedir\n"; }
    public function mkdir($path, $mode, $flags) { echo "mkdir: $path $mode $flags\n"; return true; }
    public function stream_metadata($path, $option, $value)
    {
        echo "metadata: $path $option ", json_encode($value), "\n";
        return true;
    }
}

$path = 'test-stream://base';
var_dump(stream_register_wrapper('test-stream', TestWrapper::class));
var_dump(in_array('test-stream', stream_get_wrappers(), true), stream_is_local($path));
$handle = fopen($path, 'r+', true, stream_context_create());
var_dump(feof($handle), fread($handle, 2), ftell($handle), fseek($handle, 0), fgets($handle));
var_dump(stream_supports_lock($handle), flock($handle, LOCK_EX), ftruncate($handle, 0));
var_dump(fseek($handle, 0), fwrite($handle, 'xyz'), fflush($handle), fclose($handle));
foreach (['file_exists', 'filesize'] as $function) {
    clearstatcache();
    var_dump($function($path));
}
var_dump(file_get_contents($path), file_get_contents($path, false, null, 0, 4));
var_dump(file_put_contents($path, 'hello'));
var_dump(file_put_contents($path, 'append', FILE_APPEND | LOCK_EX));

$d = opendir($path);
var_dump(readdir($d), rewinddir($d), readdir($d), closedir($d), scandir($path));
var_dump(mkdir($path, 0750, true), chmod($path, 0700), touch($path, 72));

var_dump(include 'test-stream://base/source');
var_dump(include_once 'test-stream://base/source');
var_dump(stream_wrapper_unregister('test-stream'));
var_dump(in_array('test-stream', stream_get_wrappers(), true));
var_dump(stream_wrapper_unregister('file'), stream_wrapper_register('file', TestWrapper::class));
var_dump(file_get_contents('/test-wrapper-path', false, null, 0, 4));
var_dump(stream_wrapper_restore('file'));
