<?php
namespace NS;

/**
 * @template TKey
 * @template TValue
 *
 * @extends \IteratorAggregate<TKey, TValue>
 */
interface ICollection extends \IteratorAggregate {
    /** @return \Traversable<TKey,TValue> */
    public function getIterator();
}

/**
 * @template TKey as array-key
 * @template TValue
 *
 * @implements ICollection<TKey, TValue>
 */
class Collection implements ICollection {
    /** @var array<TKey, TValue> */
    private $data;
    /**
     * @param array<TKey, TValue> $data
     */
    public function __construct(array $data) {
        $this->data = $data;
    }
    /**
     * @return \Traversable<TKey, TValue>
     */
    public function getIterator(): \Traversable {
        return new \ArrayIterator($this->data);
    }
}

$c = new Collection(["a" => 1]);

foreach ($c as $k => $v) { atan($v); strlen($k); }
