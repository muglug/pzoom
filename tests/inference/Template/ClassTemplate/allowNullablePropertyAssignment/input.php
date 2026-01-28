<?php
/**
 * @template T1
 */
interface I {
    /**
     * @return T1
     */
    public function get();
}

/**
 * @template T2
 */
class C {
    /**
     * @var T2|null
     */
    private $bar;

    /**
     * @param I<T2> $foo
     */
    public function __construct(I $foo) {
        $this->bar = $foo->get();
    }
}