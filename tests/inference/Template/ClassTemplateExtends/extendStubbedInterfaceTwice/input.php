<?php
/**
 * @template Tk of array-key
 * @template Tv
 */
interface AA {}
/**
 * @template Tk of array-key
 * @template Tv
 * @extends ArrayAccess<Tk, Tv>
 */
interface A extends ArrayAccess {
    /**
     * @psalm-param Tk $k
     * @psalm-return Tv
     */
    public function at($k);
}

/**
 * @template Tk of array-key
 * @template Tv
 *
 * @extends A<Tk, Tv>
 */
interface B extends A {}

/**
 * @template Tk of array-key
 * @template Tv
 *
 * @implements B<Tk, Tv>
 */
abstract class C implements B
{
    /**
     * @psalm-param  Tk $k
     * @psalm-return Tv
     */
    public function at($k) { /** @var Tv */ return 1;  }
}