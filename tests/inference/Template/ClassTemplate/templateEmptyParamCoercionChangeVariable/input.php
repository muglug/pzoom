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

/** @psalm-param Collection<string> $c */
function takesStringCollection(Collection $c): void {}

/** @psalm-param Collection<int> $c */
function takesIntCollection(Collection $c): void {}

$collection = new Collection();

takesStringCollection($collection);
takesIntCollection($collection);
