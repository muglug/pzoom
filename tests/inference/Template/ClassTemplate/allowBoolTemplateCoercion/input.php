<?php
/** @template T */
class TestPromise {
    /** @psalm-param T $value */
    public function __construct($value) {}
}

/** @return TestPromise<bool> */
function test(): TestPromise {
    return new TestPromise(true);
}