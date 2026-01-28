<?php
/**
 * @template T1
 */
interface A {
    /** @return T1 */
    public function get();
}

/**
 * @template T2
 * @extends A<T2>
 */
interface B extends A {}

/**
 * @template T3
 * @implements B<T3>
 */
class C implements B {
    /** @var T3 */
    private $val;

    /**
     * @psalm-param T3 $val
     */
    public function __construct($val) {
        $this->val = $val;
    }

    public function get() {
        return $this->val;
    }
}

$foo = (new C("foo"))->get();