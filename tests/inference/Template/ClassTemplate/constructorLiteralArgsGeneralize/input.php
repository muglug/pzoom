<?php
/** @template T */
class SomeCollection {
    /** @param array<T> $c */
    public function __construct(array $c) {}
}

/** @param SomeCollection<int> $c */
function takesInts(SomeCollection $c): void {}

takesInts(new SomeCollection([1, 2, 3]));
