<?php
/**
 * @template L
 * @template R
 */
interface Either{}

/**
 * @template L
 * @template-implements Either<L, mixed>
 */
final class Left implements Either {
    /**
     * @param L $value
     */
    public function __construct($value) {}
}

/**
 * @template R
 * @template-implements Either<mixed,R>
 */
final class Right implements Either {
    /**
     * @param R $value
     */
    public function __construct($value) {}
}

class A {}
class B {}

/**
 * @return Either<A,B>
 */
function result() {
    if (rand(0, 1)) {
        return new Left(new A());
    }

    return new Right(new B());
}