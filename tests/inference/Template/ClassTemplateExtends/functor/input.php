<?php
/**
 * @template T
 */
interface Functor
{
    /**
     * @template F
     * @param callable(T): F $function
     * @return Functor<F>
     */
    public function map(callable $function): Functor;
}

/**
 * @template T
 * @implements Functor<T>
 */
class FakeFunctor implements Functor
{
    /**
     * @var T
     */
    private $value;

    /**
     * @psalm-param T $value
     */
    public function __construct($value)
    {
        $this->value = $value;
    }

    public function map(callable $function): Functor
    {
        return new FakeFunctor($function($this->value));
    }
}

/** @return Functor<int<0, max>> */
function foo(string $s) : Functor {
    $foo = new FakeFunctor($s);
    $function = function (string $a): int {
        return strlen($a);
    };
    return $foo->map($function);
}
