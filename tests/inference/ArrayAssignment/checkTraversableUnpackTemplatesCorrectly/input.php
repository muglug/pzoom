<?php
/**
 * @template T1
 * @template T2
 * @template TKey
 * @template TValue
 * @extends Traversable<TKey, TValue>
 */
interface Foo extends Traversable {}

/**
 * @param Foo<"a"|"b", "c"|"d", "e"|"f", "g"|"h"> $foo
 * @return array<"e"|"f", "g"|"h">
 */
function foobar(Foo $foo): array
{
    return [...$foo];
}
                
