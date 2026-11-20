<?php
class W {
    public $context;
    private int $index=0;
    public function dir_opendir($path,$options) { echo "open $options\n"; return 1; }
    public function dir_readdir() { return [17,null,true,false,'text'][$this->index++] ?? null; }
    public function dir_rewinddir() { echo "rewind\n"; $this->index=0; return false; }
    public function dir_closedir() { echo "close\n"; }
}
stream_wrapper_register('wrap',W::class);
$dir=new DirectoryIterator('wrap://directory');
for ($i=0;$i<5;$i++) {
    printf("entry %s %s\n",json_encode($dir->getFilename()),json_encode($dir->valid()));
    $dir->next();
}
unset($dir);
