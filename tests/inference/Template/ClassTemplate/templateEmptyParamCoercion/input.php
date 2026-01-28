<?php
namespace NS;
use Countable;

/** @template T */
class Collection
{
    /** @psalm-var iterable<T> */
    private $data;

    /** @psalm-param iterable<T> $data */
    public function __construct(iterable $data = []) {
        $this->data = $data;
    }
}

class Item {}
/** @psalm-param Collection<Item> $c */
function takesCollectionOfItems(Collection $c): void {}

takesCollectionOfItems(new Collection());
takesCollectionOfItems(new Collection([]));