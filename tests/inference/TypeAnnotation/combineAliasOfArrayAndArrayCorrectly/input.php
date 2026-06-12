<?php

/**
 * @psalm-type C = array{c: string}
 */
class A {}

/**
 * @template F
 */
class D {
    /**
     * @param F $data
     */
    public function __construct(public $data) {}
}


/**
 * @psalm-import-type C from A
 */
class G {

    /**
     * @param D<array{b: bool}>|D<C> $_doesNotWork
     */
    public function doesNotWork($_doesNotWork): void {
        /** @psalm-check-type-exact $_doesNotWork = D<array{b?: bool, c?: string}> */;
    }
}
