<?php
/**
 * @template T1
 */
class Clazz
{
    /**
     * @param mixed $x
     *
     * @psalm-assert T1 $x
     */
    public function is($x) : void {}
}

/**
 * @template T2
 *
 * @param Clazz<T2> $c
 *
 * @return T2
 */
function example(Clazz $c) {
    /** @var mixed */
    $x = 0;
    $c->is($x);
    return $x;
}