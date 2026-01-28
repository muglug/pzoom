<?php
/**
 * @template B
 * @template A
 * @param Foo<B, A> $flipped
 * @return Traversable<B, A>
 */
function getFlippedTraversable(Foo $flipped): Traversable {
    return $flipped->getTraversable();
}

/**
 * @template A
 * @template B
 */
abstract class Foo
{
    /** @return Traversable<A, B> */
    public abstract function getTraversable() : Traversable;
}