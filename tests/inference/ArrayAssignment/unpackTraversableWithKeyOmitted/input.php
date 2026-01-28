<?php
/** @extends Traversable<int> */
interface Foo extends Traversable {}

/**
 * @return array<int, mixed>
 */
function foobar(Foo $foo): array
{
    return [...$foo];
}
                
