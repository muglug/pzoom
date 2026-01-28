<?php
namespace NS;

use Closure;

/**
 * @template TKey as array-key
 * @template TValue
 * @psalm-consistent-constructor
 * @psalm-consistent-templates
 */
class ArrayCollection {
    /** @var array<TKey,TValue> */
    private $data;

    /** @param array<TKey,TValue> $data */
    public function __construct(array $data) {
        $this->data = $data;
    }

    /**
     * @template T
     * @param Closure(TValue):T $func
     * @return ArrayCollection<TKey,T>
     */
    public function map(Closure $func) {
        return new static(array_map($func, $this->data));
    }
}

class Item {}
/**
 * @param ArrayCollection<array-key,Item> $i
 */
function takesCollectionOfItems(ArrayCollection $i): void {}

$c = new ArrayCollection([ new Item ]);
takesCollectionOfItems($c);
takesCollectionOfItems($c->map(function(Item $i): Item { return $i;}));
takesCollectionOfItems($c->map(function(Item $i): Item { return $i;}));