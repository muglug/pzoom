<?php
/** @template T */
class MyCollection {
    /** @param array<T> $members */
    public function __construct(public array $members) {}
}

/**
 * @param MyCollection<string> $c
 * @return MyCollection<mixed>
 */
function getMixedCollection(MyCollection $c): MyCollection {
    return $c;
}
