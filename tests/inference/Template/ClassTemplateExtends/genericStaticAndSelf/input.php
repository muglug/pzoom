<?php
/**
 * @template T
 */
interface Functor
{
    /**
     * @template U
     *
     * @param Closure(T):U $c
     *
     * @return static<U>
     */
    public function map(Closure $c);
}
/**
 * @template T
 * @implements Functor<T>
 */
final class Box implements Functor
{
    /**
     * @var T
     */
    public $value;
    /**
     * @param T $x
     */
    public function __construct($x)
    {
        $this->value = $x;
    }
    /**
     * @template U
     *
     * @param Closure(T):U $c
     *
     * @return self<U>
     */
    public function map(Closure $c)
    {
        return new Box($c($this->value));
    }
}