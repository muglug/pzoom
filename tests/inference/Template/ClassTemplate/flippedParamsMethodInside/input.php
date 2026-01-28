<?php
/**
 * @template A
 * @template B
 */
abstract class Foo
{
    /** @return Traversable<A, B> */
    public abstract function getTraversable() : Traversable;

    /**
     * @param Foo<B, A> $flipped
     * @return Traversable<B, A>
     */
    public function getFlippedTraversable(Foo $flipped): Traversable
    {
        return $flipped->getTraversable();
    }
}