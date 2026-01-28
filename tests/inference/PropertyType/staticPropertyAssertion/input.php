<?php
class Foo {
    /** @var int */
    private static $transactionDepth;

    function bar(): void {
        if (self::$transactionDepth === 0) {
        } else {
            --self::$transactionDepth;

            if (self::$transactionDepth === 0) {

            }
        }
    }
}
