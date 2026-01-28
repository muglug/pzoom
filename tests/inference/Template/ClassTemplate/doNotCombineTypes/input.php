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
     */
    public function get() {
        return $this->t;
    }
}

/**
 * @param C<A> $a
 * @param C<B> $b
 * @return C<A>|C<B>
 */
function randomCollection(C $a, C $b) : C {
    if (rand(0, 1)) {
        return $a;
    }

    return $b;
}

$random_collection = randomCollection(new C(new A), new C(new B));

$a_or_b = $random_collection->get();