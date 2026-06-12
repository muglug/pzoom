<?php
/**
 * @template A
 * @template B
 *
 * @param iterable<array-key, A> $_collection
 * @param callable(A): B $_ab
 * @return list<B>
 */
function map(iterable $_collection, callable $_ab) { return []; }

/** @template T */
final class Foo { }

/** @var list<Foo<string>> */
$items = [];

$inferred = map($items,
    /** @param Foo $i */
    function ($i) {
        return $i;
    }
);
