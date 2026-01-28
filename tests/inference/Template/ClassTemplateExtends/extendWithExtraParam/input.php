<?php
/**
 * @template Tk of array-key
 * @template Tv
 */
interface ICollection {
    /**
     * @psalm-return ICollection<Tk, Tv>
     */
    public function slice(int $start, int $length): ICollection;
}

/**
 * @template T
 *
 * @extends ICollection<int, T>
 */
interface IVector extends ICollection {
    /**
     * @psalm-return IVector<T>
     */
    public function slice(int $start, int $length): ICollection;
}