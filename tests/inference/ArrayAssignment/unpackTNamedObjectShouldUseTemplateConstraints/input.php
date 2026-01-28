<?php
/**
 * @template TKey of "a"|"b"
 * @template TValue of "c"|"d"
 * @extends Traversable<TKey, TValue>
 */
interface Foo extends Traversable {}

/**
 * @return array<"a"|"b", "c"|"d">
 */
function foobar(Foo $foo): array
{
    return [...$foo];
}
                
