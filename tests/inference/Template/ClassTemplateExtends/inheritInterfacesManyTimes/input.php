<?php
/**
 * @template Tv
 *
 * @extends IteratorAggregate<int, Tv>
 */
interface C1 extends \IteratorAggregate
{
}

/**
 * @template Tv
 *
 * @extends C1<Tv>
 */
interface C2 extends C1
{
}

/**
 * @template Tv
 *
 * @extends C2<Tv>
 */
interface C3 extends C2
{
}

/**
 * @template Tv
 *
 * @extends C3<Tv>
 */
interface C4 extends C3
{
    /**
     * @psalm-return Traversable<int, Tv>
     */
    function getIterator(): Traversable;
}