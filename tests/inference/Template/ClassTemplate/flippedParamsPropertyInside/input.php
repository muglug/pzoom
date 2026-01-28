<?php
/**
 * @template A
 * @template B
 */
abstract class Foo
{
    /** @var Traversable<A, B> */
    public $traversable;

    /**
     * @param Foo<B, A> $flipped
     * @return Traversable<B, A>
     */
    public function getFlippedTraversable(Foo $flipped): Traversable
    {
        return $flipped->traversable;
    }
}