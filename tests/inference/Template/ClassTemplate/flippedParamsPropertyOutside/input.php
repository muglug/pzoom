<?php
/**
 * @template B
 * @template A
 * @param Foo<B, A> $flipped
 * @return Traversable<B, A>
 */
function getFlippedTraversable(Foo $flipped): Traversable {
    return $flipped->traversable;
}

/**
 * @template A
 * @template B
 */
abstract class Foo
{
    /** @var Traversable<A, B> */
    public $traversable;
}