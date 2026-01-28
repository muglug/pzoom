<?php
/**
 * @template L
 * @template R
 */
interface Either {}

/**
 * @template L
 * @template-implements Either<L,mixed>
 */
class Left implements Either {
    /** @param L $value */
    public function __construct($value) { }
}

class A {}
class B {}

/** @return Either<A,B> */
function result(): Either {
    return new Left(new B());
}
