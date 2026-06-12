<?php
/**
 * @template T
 */
interface AInterface
{
    /**
     * @param T $i
     * @return T
     */
    public function foo($i);
}

/**
 * @implements AInterface<A::*>
 */
class A implements AInterface {
    const C_1 = 1;
    const C_2 = 2;
    const C_3 = 3;
    const D_4 = 4;

    public function foo($i)
    {
        return $i;
    }
}

$a = new A();
$a->foo(1);
$a->foo(2);
$a->foo(3);
$a->foo(A::D_4);
