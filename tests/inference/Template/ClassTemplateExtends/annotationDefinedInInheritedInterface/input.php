<?php
/**
 * @template T1
 */
interface X {
    /**
     * @param T1 $x
     * @return T1
     */
    public function boo($x);
}

/**
 * @template T2
 * @extends X<T2>
 */
interface Y extends X {}

/**
 * @template T3
 * @implements Y<T3>
 */
class A implements Y {
    public function boo($x) {
        return $x;
    }
}

function foo(A $a) : void {
    $a->boo("boo");
}