<?php
/**
 * @template TKey of array-key
 * @template TValue
 * @template-extends \ArrayObject<TKey,TValue>
 */
class C extends \ArrayObject {
    /**
     * @param array<TKey,TValue> $kv
     */
    public function __construct(array $kv) {
        parent::__construct($kv);
    }
}

$c = new C(["a" => 1]);
$i = $c->getIterator();
