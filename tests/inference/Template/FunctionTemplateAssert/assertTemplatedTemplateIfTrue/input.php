<?php
/**
 * @template T1
 */
class Clazz
{
    /**
     * @param mixed $x
     *
     * @return bool
     *
     * @psalm-assert-if-true T1 $x
     */
    public function is($x) : bool {
        return true;
    }
}

/**
 * @template T2
 *
 * @param Clazz<T2> $c
 *
 * @return T2|false
 */
function example(Clazz $c) {
    /** @var mixed */
    $x = 0;
    return $c->is($x) ? $x : false;
}