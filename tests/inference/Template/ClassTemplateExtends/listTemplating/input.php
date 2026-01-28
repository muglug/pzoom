<?php
/**
 * @template T
 */
interface X {
    /**
     * @param list<T> $x
     * @return T
     */
    public function boo($x);
}

/**
 * @template T
 * @implements X<T>
 */
class A implements X {
    public function boo($x) {
        return $x[0];
    }
}