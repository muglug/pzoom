<?php
/**
 * @template T1
 * @template T2
 * @template TKey
 * @template TValue
 * @extends Traversable<TKey, TValue>
 */
interface Foo extends Traversable {}

/** @extends Foo<"a", "b", "c", "d"> */
interface Bar extends Foo {}

/**
 * @return array<"c", "d">
 */
function foobar(Bar $bar): array
{
    return [...$bar];
}
                
