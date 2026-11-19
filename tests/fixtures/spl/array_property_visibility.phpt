--TEST--
SPL ARRAY_AS_PROPS uses stored keys for unset and inaccessible object properties
--FILE--
<?php
class CollectionWithProperties extends ArrayObject {
    public $label = 'real label';
    private $key = 'private key';
    public string $uninitialized;
    public function privateKey() { return $this->key; }
}
$collection = new CollectionWithProperties(
    ['label' => 'stored label', 'key' => 'stored key', 'uninitialized' => 'stored value'],
    ArrayObject::ARRAY_AS_PROPS,
);
echo $collection->label, "\n";
unset($collection->label);
echo $collection->label, "\n";
$collection->label = 'new label';
echo $collection['label'], "\n";
echo $collection->privateKey(), "\n";
echo $collection->key, "\n";
$collection->key = 'public key';
echo $collection['key'], "\n";
echo $collection->privateKey(), "\n";
echo $collection->uninitialized, "\n";
unset($collection->label);
var_dump(isset($collection->label));
--EXPECT--
real label
stored label
new label
private key
stored key
public key
private key
stored value
bool(false)
