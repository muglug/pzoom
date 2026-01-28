<?php
class A {}
class B {}

/**
 * @template T
 */
class C {
    /**
     * @var T
     */
    private $t;

    /**
     * @param T $t
     */
    public function __construct($t) {
        $this->t = $t;
    }

    /**
     * @return T
     * @psalm-mutation-free
     */
    public function get() {
        return $this->t;
    }
}

/** @var C<A>|C<B> $random_collection **/
$a_or_b = $random_collection->get();