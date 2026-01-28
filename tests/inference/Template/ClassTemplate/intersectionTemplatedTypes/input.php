<?php
namespace NS;
use Countable;

/** @template T */
class Collection
{
    /** @psalm-var iterable<T> */
    private $data;

    /** @psalm-param iterable<T> $data */
    public function __construct(iterable $data) {
        $this->data = $data;
    }
}

class Item {}
/** @psalm-param Collection<Item> $c */
function takesCollectionOfItems(Collection $c): void {}

/** @psalm-var iterable<Item> $data2 */
$data2 = [];
takesCollectionOfItems(new Collection($data2));

/** @psalm-var iterable<Item>&Countable $data */
$data = [];
takesCollectionOfItems(new Collection($data));